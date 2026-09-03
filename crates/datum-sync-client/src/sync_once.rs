//! High-level "run a sync cycle" entry point.
//!
//! This module is the seam between the CLI / daemon and the wire protocol:
//! callers hand over a [`SyncConfig`] (server URL, bearer token, device id,
//! master key, direction, dry-run flag), an HTTP [`Client`], and an open
//! [`Db`], and get back a [`SyncReport`] tallying entries pushed, applied,
//! and conflicts queued. No `clap`, no stdout, no XDG paths — everything
//! the CLI surface in `datum-commands` once owned.
//!
//! Integration coverage for this orchestration (push/pull round-trips,
//! seq-mismatch recovery, generation mismatch, dry-run) lives in
//! `crates/datum-cli/tests/` (`sync_push_pull.rs`, `sync_e2e.rs`,
//! `sync_refresh.rs`) against the real in-process relay server.
//!
//! Gated by both `http` and `db` features because a single sync cycle
//! pairs an HTTP client with the local SQLite. WASM clients that bring
//! their own fetch shim should reimplement [`sync_once`] against their
//! storage layer rather than enabling this module.
//!
//! ## Direction
//!
//! [`Direction::Both`] always pulls *before* pushing so the push phase
//! sends incremental deltas instead of re-uploading rows the pull just
//! wrote. Pull-side conflicts land in the conflict queue but never block
//! the subsequent push.
//!
//! ## Watermarks
//!
//! Push advances two counters: `push_mark` (max `modify_time` epoch-ms
//! packed) and `device_seq` (per-device counter that stays in lockstep
//! with the relay). Pull advances `pull_seq` to the relay's global row id.
//! See `docs/sync-protocol.md` §Sequence semantics.
//!
//! ## Paging (da-5ua6)
//!
//! Neither direction sends a whole history in one request. Push walks the
//! local history in pages of [`SyncOpts::push_page_entries`] entries, and
//! pull asks for at most [`SyncOpts::pull_page_rows`] relay rows per
//! response. Both loop until a page comes back empty, advancing (and
//! persisting) their watermark per completed page — so a run that dies
//! mid-history resumes at the next page rather than restarting, and
//! neither side depends on a request or response that grows with the size
//! of the database.
//!
//! The two page sizes are separate numbers because they count different
//! things — entries versus relay rows — and only the pull one is bounded
//! by the relay (da-blcb).

use anyhow::Result;
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::integrity::{self, DetectedBy};
use reqwest::blocking::Client;

pub use datum_sync_protocol::messages::sync::MAX_PULL_ROWS;
use datum_sync_protocol::SyncProtocolError;

use crate::crypto::changeset::{decrypt_changeset, encrypt_changeset, Changeset, EncryptedBlob};
use crate::http::session::{Session, SessionError};
use crate::http::{pull_batch, push_blob, PulledChangeset, PushAck};
use crate::ops::{apply_changeset, changeset_entries, pack_changeset_page};
use datum_core::sync_state;

/// Direction of a sync cycle. `Both` is the default for `datum sync` with
/// no flags; the daemon's scheduler issues `PushOnly` and `PullOnly`
/// independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Both,
    PushOnly,
    PullOnly,
}

impl Direction {
    pub fn does_push(self) -> bool {
        matches!(self, Direction::Both | Direction::PushOnly)
    }
    pub fn does_pull(self) -> bool {
        matches!(self, Direction::Both | Direction::PullOnly)
    }
}

/// Inputs for a single sync cycle. The [`Session`] handles bearer +
/// refresh-token rotation transparently — callers no longer pass the
/// raw bearer; preflight builds a Session and threads it through here.
pub struct SyncConfig<'a> {
    pub server: &'a str,
    pub session: &'a Session,
    pub device_id: &'a str,
    pub master_key: &'a [u8; 32],
    pub direction: Direction,
    /// Pack and decrypt to count entries, but skip HTTP and watermark
    /// advances. Used by `datum sync --dry-run`.
    pub dry_run: bool,
    /// Page size + progress sink for this cycle.
    pub opts: SyncOpts<'a>,
}

/// One completed page of a push or pull. A first sync of a real history
/// is many requests over many seconds, so callers get told about each one
/// rather than watching a silent process (da-5ua6). Library code never
/// prints (da-60gf) — the CLI renders these on stderr, the daemon logs
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncProgress {
    /// A push page was accepted by the relay.
    PushPage {
        /// 1-based page number within this run.
        page: usize,
        /// Entries in this page.
        entries: usize,
        /// Entries pushed so far in this run, this page included.
        total: usize,
    },
    /// A pull page was fetched and applied.
    PullPage {
        page: usize,
        /// Entries applied from this page.
        entries: usize,
        total: usize,
    },
    /// One changeset within a pull page was applied (da-j8cu). A relay row
    /// carries a whole push page, so one pull page can be the entire first
    /// sync — page-granular progress alone would stay silent through it.
    PullChangeset {
        /// 1-based changeset number within this run.
        changeset: usize,
        /// Entries this changeset carried.
        entries: usize,
        /// Entries applied so far in this run, this changeset included.
        total: usize,
    },
}

/// Tunables for one sync cycle: how big a page is, and where per-page
/// progress goes. [`Default`] is the right choice for every caller that
/// doesn't have a reason to differ — tests shrink the page sizes to
/// exercise the paging loops without building a huge DB.
///
/// Push and pull pages are measured in different units and carry separate
/// knobs. Sharing one number for both is what broke pull: a push page is
/// entries, a pull page is relay rows (each holding up to a whole push
/// page of entries), so the push default over-asked the relay's row
/// ceiling by 10× and every pull ended after one page (da-blcb).
#[derive(Clone, Copy)]
pub struct SyncOpts<'a> {
    /// Max entries per push request. Zero is treated as the default
    /// rather than as "no progress possible".
    pub push_page_entries: usize,
    /// Max relay rows per pull response. Zero is treated as the default;
    /// anything above [`MAX_PULL_ROWS`] is capped to it, since the relay
    /// rejects a larger ask and a client that asked for more than it can
    /// be granted would misread the granted page as end-of-stream.
    pub pull_page_rows: usize,
    pub progress: Option<&'a dyn Fn(SyncProgress)>,
}

impl Default for SyncOpts<'_> {
    fn default() -> Self {
        Self {
            push_page_entries: crate::ops::DEFAULT_PUSH_PAGE_ENTRIES,
            pull_page_rows: MAX_PULL_ROWS,
            progress: None,
        }
    }
}

impl SyncOpts<'_> {
    fn push_page_entries(&self) -> usize {
        if self.push_page_entries == 0 {
            crate::ops::DEFAULT_PUSH_PAGE_ENTRIES
        } else {
            self.push_page_entries
        }
    }

    fn pull_page_rows(&self) -> usize {
        if self.pull_page_rows == 0 {
            MAX_PULL_ROWS
        } else {
            self.pull_page_rows.min(MAX_PULL_ROWS)
        }
    }

    fn report(&self, progress: SyncProgress) {
        if let Some(sink) = self.progress {
            sink(progress);
        }
    }
}

impl std::fmt::Debug for SyncOpts<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncOpts")
            .field("push_page_entries", &self.push_page_entries)
            .field("pull_page_rows", &self.pull_page_rows)
            .field("progress", &self.progress.is_some())
            .finish()
    }
}

/// Emitted when the relay rejected our predicted per-device seq and the
/// push recovered in-band by re-basing to the relay's authoritative
/// value (crash between push and watermark advance, DB restore, relay
/// reset — see the recovery comment in [`push_once`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqRebase {
    /// Device whose seq diverged (the local device).
    pub device_id: String,
    /// Seq this device predicted from local sync_state.
    pub predicted: u64,
    /// Authoritative seq the relay expected; the retry used this value.
    pub expected: u64,
    /// The re-base was *downward*: the relay's next seq sat below what
    /// this device had already pushed, so the relay has lost rows rather
    /// than merely disagreed about the next one (da-od2t). The push
    /// rewound its watermarks and replayed the whole local history.
    pub relay_rewound: bool,
}

/// Emitted when a run discovered that the relay no longer holds
/// changesets this device already pushed — a replaced relay, or one
/// restored from a backup predating them (da-od2t). The run rewound its
/// watermarks and re-sent the device's whole history rather than leaving
/// the cluster silently short. Library code never prints; the CLI turns
/// this into a warning and the daemon logs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayRewind {
    /// How many of this device's pushes the relay was missing.
    pub missing: u64,
}

/// Result of a push run — how many rows went up the wire this time.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PushOutcome {
    pub pushed: usize,
    /// Requests the run took (da-5ua6). One per page; 0 when there was
    /// nothing to push.
    pub pages: usize,
    /// `Some` when the push recovered from a seq desync. Library code
    /// never prints — callers decide how to surface it (CLI: stderr
    /// warning; daemon: tracing). Holds the most recent re-base of the
    /// run.
    pub seq_rebased: Option<SeqRebase>,
    /// `Some` when the push found the relay behind this device's own
    /// seq and re-sent the whole local history (da-od2t).
    pub relay_rewound: Option<RelayRewind>,
}

/// Result of a pull run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PullOutcome {
    /// Entries applied to the local DB (including LWW no-ops).
    pub applied: usize,
    /// Rows filed into the `integrity_warnings` queue with
    /// `detected_by='sync_pull'`.
    pub integrity_warnings_queued: usize,
    /// Requests the run took (da-5ua6).
    pub pages: usize,
    /// Relay rows skipped because they were written under a superseded
    /// generation (da-pjhg). Counted rather than dropped silently — a
    /// device that ignores incoming data should say how much.
    pub superseded_skipped: usize,
    /// `Some` when the pull found the relay behind this device's own
    /// seq and rewound the watermarks so the push phase re-sends the
    /// whole local history (da-od2t). Pull is where a device with
    /// nothing pending learns this at all.
    pub relay_rewound: Option<RelayRewind>,
}

/// Tally of a full [`sync_once`] cycle.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub push: PushOutcome,
    pub pull: PullOutcome,
}

/// Run one full sync cycle per [`SyncConfig::direction`]. Pulls first
/// (when both directions are requested) so the push phase sees the
/// freshest local state.
pub fn sync_once(client: &Client, db: &Db, cfg: &SyncConfig<'_>) -> Result<SyncReport> {
    let mut report = SyncReport::default();
    if cfg.direction.does_pull() {
        report.pull = pull_once_with(
            cfg.server,
            client,
            cfg.session,
            cfg.device_id,
            cfg.master_key,
            db,
            cfg.dry_run,
            &cfg.opts,
        )?;
    }
    if cfg.direction.does_push() {
        report.push = push_once_with(
            cfg.server,
            client,
            cfg.session,
            cfg.device_id,
            cfg.master_key,
            db,
            cfg.dry_run,
            &cfg.opts,
        )?;
    }
    Ok(report)
}

/// Pack changes since the last push watermark, encrypt, and upload — in
/// pages, with default [`SyncOpts`]. See [`push_once_with`].
pub fn push_once(
    server: &str,
    client: &Client,
    session: &Session,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    dry_run: bool,
) -> Result<PushOutcome> {
    push_once_with(
        server,
        client,
        session,
        device_id,
        master_key,
        db,
        dry_run,
        &SyncOpts::default(),
    )
}

/// Pack changes since the last push watermark, encrypt, and upload,
/// walking the history in pages of `opts.push_page_entries` entries until a page
/// comes back empty (da-5ua6). Each accepted page advances *and persists*
/// the watermark, so an interrupted run resumes at the first page that
/// never landed instead of re-uploading everything — and no single
/// request carries more than a page, which is what keeps a first sync of
/// a long history under the relay's body limit.
///
/// On `dry_run`, packs and counts every pending page but skips the HTTP
/// calls and the watermark advances — a "what would I push?" preview that
/// takes on none of the relay's per-device seq commitment.
#[allow(clippy::too_many_arguments)]
pub fn push_once_with(
    server: &str,
    client: &Client,
    session: &Session,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    dry_run: bool,
    opts: &SyncOpts<'_>,
) -> Result<PushOutcome> {
    // Send the locally-stored cluster_id and last-seen cluster generation.
    // A [`SyncProtocolError::GenerationMismatch`] from the relay propagates
    // through Session as `SessionError::GenerationMismatch` and bubbles
    // out to the caller for re-init handling.
    //
    // cluster_id is required on the wire (post-pre-prod cleanup). If no
    // local cluster is set yet (first sync from this device, no `sync
    // login --cluster <name>` override), default to the DB's file name
    // (da-3fnd) — `vremya.db` → `vremya`. Two devices tracking a
    // same-named DB land on the same cluster and sync out of the box;
    // differently-named DBs stay isolated. The server scopes clusters by
    // user_id, so two users' same-named defaults don't collide.
    //
    // `local_was_unset` flips the persistence path below from "only
    // persist if server bumped us" to "always persist on first sync".
    let stored_cluster = sync_state::get_cluster_id(db)?;
    let local_was_unset = stored_cluster.is_none();
    let local_cluster = stored_cluster.unwrap_or_else(|| sync_state::default_cluster_id(db));
    let local_generation = sync_state::get_cluster_generation(db)?;

    let page_size = opts.push_page_entries();
    // Local cursor: it tracks the persisted `push_mark` on a real run, and
    // stands in for it on a dry run (which must still walk every page to
    // count, without committing anything to the DB).
    let mut watermark = sync_state::get_push_mark(db)?;
    let mut outcome = PushOutcome::default();
    // Set once, by the first page whose ack revealed a relay behind our
    // own seq (da-od2t); cleared when the walk restarts from zero.
    let mut replay_from_zero = false;

    loop {
        // `max_ms` is the largest `modify_time_ms` over exactly the packed
        // rows, computed in the same query as the filter — re-using it as
        // the next watermark cannot skip a row (see `pack_changeset_page`).
        let (changeset, max_ms) = pack_changeset_page(db, watermark, page_size)?;
        let entry_count = changeset_entry_count(&changeset);
        if entry_count == 0 {
            break;
        }

        if !dry_run {
            let (ack, seq_rebased) = push_page(
                server,
                client,
                session,
                device_id,
                master_key,
                db,
                &changeset,
                &local_cluster,
                local_generation,
            )?;
            if seq_rebased.as_ref().is_some_and(|r| r.relay_rewound)
                && outcome.relay_rewound.is_none()
            {
                // `push_mark` is 0 now; the whole local history is pending
                // again, so re-walk it from the start after this page.
                replay_from_zero = true;
                outcome.relay_rewound = Some(RelayRewind {
                    missing: seq_rebased
                        .as_ref()
                        .map(|r| r.predicted.saturating_sub(r.expected))
                        .unwrap_or(0),
                });
            }
            if seq_rebased.is_some() {
                outcome.seq_rebased = seq_rebased;
            }

            // Persist the cluster_id the server confirmed on push
            // acceptance. First sync (local_was_unset) always writes, since
            // the locally-used value was a db-name default; subsequent
            // pushes only write if the server bumped the cluster between
            // push and acceptance.
            if local_was_unset || ack.cluster_id != local_cluster {
                sync_state::set_cluster_id(db, &ack.cluster_id)?;
            }
            if ack.generation != local_generation {
                sync_state::set_cluster_generation(db, ack.generation)?;
            }
        }

        outcome.pages += 1;
        outcome.pushed += entry_count;
        opts.report(SyncProgress::PushPage {
            page: outcome.pages,
            entries: entry_count,
            total: outcome.pushed,
        });

        // da-od2t: `push_page` rewound `push_mark` to 0 because the relay
        // turned out not to hold our history. Restart the walk from there
        // instead of advancing past a page the relay never had — once per
        // run, since the relay is level with us from here on. Re-sending
        // the page we just sent is harmless: apply is content-addressed
        // and last-write-wins.
        if replay_from_zero {
            replay_from_zero = false;
            watermark = 0;
            continue;
        }

        // Advance past the page we just sent. A cursor that doesn't move
        // (no packed row carried a usable epoch-ms metric) would re-send
        // this page forever, so stop instead — the rows stay pending and
        // the next run sees the same state, no data lost.
        let Some(next) = max_ms.filter(|ms| *ms > watermark) else {
            break;
        };
        // Persist per completed page, so an interrupted run resumes here.
        if !dry_run {
            sync_state::advance_push_mark(db, next)?;
        }
        watermark = next;
    }

    Ok(outcome)
}

/// Upload one encrypted page, recovering in-band from a seq mismatch.
///
/// Predict our next per-device seq and bind it into the changeset AAD at
/// encrypt time (the relay can't stamp a seq into the AAD after the fact,
/// and the predict-and-check also detects device_id collisions). The relay
/// validates the prediction *before* storing: on a mismatch it returns its
/// authoritative `expected` seq and we recover in-band — re-base
/// `device_seq`, RE-ENCRYPT (the AAD depends on seq), and retry once. This
/// makes a seq desync (crash between push and the watermark advance, DB
/// restore, relay reset) recoverable instead of a permanent softlock. Only
/// a *second* mismatch is a genuine device_id collision and is fatal
/// (da-ca4q).
#[allow(clippy::too_many_arguments)]
fn push_page(
    server: &str,
    client: &Client,
    session: &Session,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    changeset: &Changeset,
    local_cluster: &str,
    local_generation: u64,
) -> Result<(PushAck, Option<SeqRebase>)> {
    let predicted = sync_state::get_device_seq(db)?.saturating_add(1);
    // The AAD also binds `cluster_id` (da-3nkx) so a blob cannot be
    // unsealed against a different cluster — pass `local_cluster` here and
    // again in the recovery re-encrypt below.
    let blob = encrypt_changeset(changeset, master_key, device_id, predicted, local_cluster)?;
    let push_with_seq = |seq: u64, blob: &EncryptedBlob| {
        session.run(client, |bearer| {
            push_blob(
                client,
                server,
                bearer,
                device_id,
                blob.as_bytes(),
                local_cluster,
                local_generation,
                seq as i64,
            )
        })
    };

    match push_with_seq(predicted, &blob) {
        Ok(ack) => {
            sync_state::advance_device_seq(db, predicted)?;
            Ok((ack, None))
        }
        Err(SessionError::SeqMismatch(SyncProtocolError::SeqMismatch { expected, .. })) => {
            let expected = expected.max(0) as u64;
            let reblob =
                encrypt_changeset(changeset, master_key, device_id, expected, local_cluster)?;
            let ack = push_with_seq(expected, &reblob).map_err(|e| match e {
                SessionError::SeqMismatch(SyncProtocolError::SeqMismatch {
                    expected,
                    predicted,
                }) => anyhow::anyhow!(
                    "sync seq still mismatched after recovery (retried {predicted}, relay now \
                     expects {expected}) — another device is using device_id {device_id}; \
                     refusing to push",
                ),
                other => anyhow::Error::new(other),
            })?;
            // da-od2t: `expected - 1` is the highest seq the relay still
            // holds for us. Below our own `device_seq` it has *lost*
            // pushes we made (replaced relay, or one restored from a
            // backup predating them) rather than merely disagreed about
            // the next one — so rewind the watermarks before adopting
            // its seq, and let the caller replay the whole history.
            let relay_rewound =
                sync_state::rewind_for_replaced_relay(db, expected.saturating_sub(1))?;
            // The relay is authoritative here and `expected` may sit
            // *below* our stored value (relay reset / older-backup
            // restore), so force-set past the monotonic guard.
            sync_state::set_device_seq(db, expected)?;
            // Surface the recovery as a typed event in the outcome —
            // the CLI prints a warning, the daemon logs it. No printing
            // from library code (da-60gf).
            Ok((
                ack,
                Some(SeqRebase {
                    device_id: device_id.to_string(),
                    predicted,
                    expected,
                    relay_rewound: relay_rewound.is_some(),
                }),
            ))
        }
        Err(other) => Err(other.into()),
    }
}

/// Fetch peers' changesets, decrypt, apply via LWW, and queue any
/// conflicts the apply uncovered — in pages, with default [`SyncOpts`].
/// See [`pull_once_with`].
pub fn pull_once(
    server: &str,
    client: &Client,
    session: &Session,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    dry_run: bool,
) -> Result<PullOutcome> {
    pull_once_with(
        server,
        client,
        session,
        device_id,
        master_key,
        db,
        dry_run,
        &SyncOpts::default(),
    )
}

/// Fetch peers' changesets, decrypt, apply via LWW, and queue any
/// conflicts the apply uncovered, at most `opts.pull_page_rows` relay rows per
/// request until a page comes back empty (da-5ua6).
///
/// Pull is the mirror of push: the second device syncing down a history
/// would otherwise ask the relay for every row since its watermark in one
/// response. Each applied page advances the pull watermark before the next
/// request, so an interrupted run resumes rather than re-downloading.
#[allow(clippy::too_many_arguments)]
pub fn pull_once_with(
    server: &str,
    client: &Client,
    session: &Session,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    dry_run: bool,
    opts: &SyncOpts<'_>,
) -> Result<PullOutcome> {
    // Pin the pull to the locally-stored cluster and last-seen generation.
    // The server runs `check_generation` before returning rows; a mismatch
    // surfaces as `SessionError::GenerationMismatch` and bubbles out
    // unchanged. Same default-cluster rule as push (see the long comment
    // there): auto-default to the DB file name when no local cluster is
    // set. `local_was_unset` lets the persistence below stamp the
    // cluster_id on first sync even when the server echoes the same
    // default name we sent.
    let stored_cluster = sync_state::get_cluster_id(db)?;
    let local_was_unset = stored_cluster.is_none();
    let local_cluster = stored_cluster.unwrap_or_else(|| sync_state::default_cluster_id(db));
    let local_generation = sync_state::get_cluster_generation(db)?;

    let page_rows = opts.pull_page_rows();
    // Local cursor over relay-global row ids; mirrors the persisted
    // `pull_seq` on a real run and stands in for it on a dry run.
    let mut since = sync_state::get_pull_seq(db)? as i64;
    let mut outcome = PullOutcome::default();
    let mut cluster_confirmed = false;
    // The generation this device is currently on. Starts at the stored
    // value and only ever moves forward (da-pjhg) — see the page-level
    // adoption below.
    let mut current_generation = local_generation;
    // 1-based changeset counter across the whole run, for within-page progress.
    let mut changesets = 0usize;
    // da-od2t: the relay states what it holds for *us* on every pull, so
    // one check per run is enough — and pull is the only call a device
    // with nothing pending makes at all.
    let mut relay_checked = dry_run;

    loop {
        let page = session.run(client, |bearer| {
            pull_batch(
                client,
                server,
                bearer,
                device_id,
                since,
                &local_cluster,
                current_generation,
                page_rows,
            )
        })?;

        // A relay holding fewer of our pushes than we made has lost them:
        // it was replaced, or restored from a backup predating them. Its
        // row-id space restarted too, so the rewind takes `pull_seq` back
        // to 0 and this run re-reads the cluster from the beginning; the
        // push phase then re-sends our whole history instead of declining
        // to re-send what the relay is silently missing.
        if !relay_checked {
            relay_checked = true;
            let relay_high = page.device_high_seq.max(0) as u64;
            if let Some(missing) = sync_state::rewind_for_replaced_relay(db, relay_high)? {
                outcome.relay_rewound = Some(RelayRewind { missing });
                since = 0;
                continue;
            }
        }

        let rows = page.rows;
        if rows.is_empty() {
            break;
        }
        let full_page = rows.len() >= page_rows;

        // Confirm the cluster_id the relay echoed back, on the first row.
        // Every row in the batch shares it because the pull is filtered
        // server-side by cluster. First sync (local_was_unset) always
        // writes, since the locally-used value was a default-string
        // placeholder; subsequent syncs only write if the server changed
        // it. Once per run — later pages carry the same cluster.
        //
        // `generation` is deliberately NOT read here (da-pjhg). The relay
        // filters a pull by cluster, device and user — not by generation —
        // so one batch can carry rows from several generations, and
        // `rows[0]`'s value is simply whichever row sorted first. Reading
        // it as "the batch's generation" let a batch whose first row
        // predated a bump assign this device a *lower* generation than it
        // already held, after which every push 409'd. Each row is now
        // judged on its own generation in the loop below.
        if !dry_run && !cluster_confirmed {
            let first = &rows[0];
            if local_was_unset || first.cluster_id != local_cluster {
                sync_state::set_cluster_id(db, &first.cluster_id)?;
            }
            cluster_confirmed = true;
        }

        // Adopt a newer generation before judging any row against it, so
        // a page that straddles a bump is measured against the newest
        // generation it carries rather than the first row's. Forward
        // only: a lower value is a superseded row, not a new truth, and
        // assigning it would push this device back onto a dead
        // generation. (Same distinction `advance_pull_seq` draws against
        // `reset_pull_seq` — a monotonic guard, with the deliberate
        // rewind confined to recovery.)
        let page_generation = rows
            .iter()
            .map(|r| r.generation)
            .max()
            .unwrap_or(current_generation);
        if page_generation > current_generation {
            current_generation = page_generation;
            if !dry_run {
                sync_state::set_cluster_generation(db, current_generation)?;
            }
        }

        let mut max_seq: u64 = since as u64;
        let mut applied = 0usize;
        // Entries applied but not yet walked through integrity detection.
        // Detection is batched (da-j8cu) so the per-field history it reads
        // is read once per flush instead of once per entry.
        let mut pending_detect: Vec<Entry> = Vec::new();
        for row in &rows {
            if (row.id as u64) > max_seq {
                max_seq = row.id as u64;
            }
            // da-pjhg: never apply a changeset written under a superseded
            // generation. A bump is the cluster stating that what came
            // before it is void — a purge means "this data is gone", a
            // restore means "the snapshot is the truth now". Applying a
            // pre-bump row merges exactly the state the bump discarded
            // back in, which is how a purged device re-acquired its old
            // rows on the next sync. The recovery flow deletes local
            // pre-reinit entries by timestamp *before* the retry pull, so
            // without this gate nothing re-examines what arrives after.
            //
            // The watermark still advances past a skipped row (above): a
            // superseded row can never become applicable, so leaving the
            // watermark behind would re-fetch it on every sync forever.
            if row.generation < current_generation {
                outcome.superseded_skipped += 1;
                continue;
            }
            if dry_run {
                applied += incoming_entry_count(row, master_key);
                continue;
            }

            let blob = EncryptedBlob::from_bytes(row.payload.clone());
            // Per the spec, every decrypt failure must look alike — no oracle.
            let cs = decrypt_changeset(
                &blob,
                master_key,
                &row.device_id,
                row.seq as u64,
                &row.cluster_id,
            )
            .map_err(|_| {
                anyhow::anyhow!(
                    "failed to decrypt changeset from {} seq {} — wrong key or tampered blob",
                    row.device_id,
                    row.seq,
                )
            })?;

            // Snapshot which entries the incoming changeset carries BEFORE
            // we apply it — `apply_changeset` may drop older-modify_time
            // rows via LWW, but conflict detection cares about every row
            // that made it into the local table, not only the ones that
            // actually changed.
            let incoming = changeset_entries(cs.as_bytes())?;
            apply_changeset(db, &cs)?;
            let incoming_len = incoming.len();
            applied += incoming_len;
            pending_detect.extend(incoming);
            if pending_detect.len() >= DETECT_BATCH_ENTRIES {
                outcome.integrity_warnings_queued +=
                    file_warnings_for_incoming(db, &pending_detect)?;
                pending_detect.clear();
            }

            // One relay row is a whole push page — up to
            // `DEFAULT_PUSH_PAGE_ENTRIES` entries — so a single pull page can
            // be the entire first sync. Report inside the page too, or a
            // minutes-long apply says nothing until it is over (da-j8cu).
            changesets += 1;
            opts.report(SyncProgress::PullChangeset {
                changeset: changesets,
                entries: incoming_len,
                total: outcome.applied + applied,
            });
        }
        outcome.integrity_warnings_queued += file_warnings_for_incoming(db, &pending_detect)?;

        outcome.applied += applied;
        outcome.pages += 1;
        opts.report(SyncProgress::PullPage {
            page: outcome.pages,
            entries: applied,
            total: outcome.applied,
        });

        // Advance only after the whole page has landed locally, so an
        // interrupted run re-pulls the page it never finished.
        if max_seq <= since as u64 {
            break;
        }
        if !dry_run {
            sync_state::advance_pull_seq(db, max_seq)?;
        }
        since = max_seq as i64;
        // A short page means the relay had nothing more; skip the extra
        // round-trip that would only confirm it. Sound only because
        // `page_rows` is what the relay actually granted — it never
        // exceeds MAX_PULL_ROWS, and the relay rejects a larger ask
        // rather than substituting its own number (da-blcb).
        if !full_page {
            break;
        }
    }

    Ok(outcome)
}

/// Peek at the entry count inside a packed changeset via the single
/// envelope-validating parser in [`crate::ops`]. An invalid envelope
/// counts as 0 — the real apply path surfaces the error.
fn changeset_entry_count(cs: &Changeset) -> usize {
    changeset_entries(cs.as_bytes())
        .map(|v| v.len())
        .unwrap_or(0)
}

/// How many applied entries may queue up before integrity detection runs.
/// Detection reads each touched field's history once per flush, so a bigger
/// batch means fewer reads; the cap is what keeps a first sync's entries from
/// all being held in memory at once. One relay row already carries up to
/// `DEFAULT_PUSH_PAGE_ENTRIES` (1000), so this spans several of them.
const DETECT_BATCH_ENTRIES: usize = 4096;

/// After applying incoming entries via LWW, run the same non-blocking
/// overlap/double-open/orphan-end detector D5 uses for local writes — but
/// tag each detected row as `detected_by='sync_pull'` so the integrity-
/// warning UX (C4e) can distinguish the two sources.
///
/// Batched on purpose (da-j8cu): the per-entry door re-reads the whole
/// history of the entry's field for every row, which made a first pull
/// quadratic — ten minutes of silence on a real history. `detect_for_entries`
/// reads each field once for the batch. Every entry passed here must already
/// have been applied, which is what the caller's flush points guarantee.
fn file_warnings_for_incoming(db: &Db, incoming: &[Entry]) -> Result<usize> {
    if incoming.is_empty() {
        return Ok(0);
    }
    let tx = db.conn().unchecked_transaction()?;
    let filed = integrity::detect_for_entries(&tx, incoming, DetectedBy::SyncPull)?;
    tx.commit()?;
    Ok(filed.len())
}

/// Dry-run helper: peek at the entry count inside a single pulled row
/// without mutating anything. Decrypt failures yield 0 silently (the
/// caller can still surface the error on the real run).
fn incoming_entry_count(row: &PulledChangeset, master_key: &[u8; 32]) -> usize {
    let blob = EncryptedBlob::from_bytes(row.payload.clone());
    let Ok(cs) = decrypt_changeset(
        &blob,
        master_key,
        &row.device_id,
        row.seq as u64,
        &row.cluster_id,
    ) else {
        return 0;
    };
    changeset_entry_count(&cs)
}
