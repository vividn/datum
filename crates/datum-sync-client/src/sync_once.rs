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

use anyhow::Result;
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::integrity::{self, DetectedBy};
use reqwest::blocking::Client;

use datum_sync_protocol::SyncProtocolError;

use crate::crypto::changeset::{decrypt_changeset, encrypt_changeset, Changeset, EncryptedBlob};
use crate::http::session::{Session, SessionError};
use crate::http::{pull_batch, push_blob, PulledChangeset};
use crate::ops::{apply_changeset, pack_changeset_since_ms};
use crate::sync_state;

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
}

/// Result of a push run — how many rows went up the wire this time.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PushOutcome {
    pub pushed: usize,
    /// `Some` when the push recovered from a seq desync. Library code
    /// never prints — callers decide how to surface it (CLI: stderr
    /// warning; daemon: tracing).
    pub seq_rebased: Option<SeqRebase>,
}

/// Result of a pull run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PullOutcome {
    /// Entries applied to the local DB (including LWW no-ops).
    pub applied: usize,
    /// Rows filed into the `integrity_warnings` queue with
    /// `detected_by='sync_pull'`.
    pub integrity_warnings_queued: usize,
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
        report.pull = pull_once(
            cfg.server,
            client,
            cfg.session,
            cfg.device_id,
            cfg.master_key,
            db,
            cfg.dry_run,
        )?;
    }
    if cfg.direction.does_push() {
        report.push = push_once(
            cfg.server,
            client,
            cfg.session,
            cfg.device_id,
            cfg.master_key,
            db,
            cfg.dry_run,
        )?;
    }
    Ok(report)
}

/// Pack changes since the last push watermark, encrypt, and upload.
///
/// On `dry_run`, packs and counts but skips both the HTTP call and the
/// watermark advance — useful for a "what would I push?" preview without
/// taking on the relay's per-device seq commitment.
pub fn push_once(
    server: &str,
    client: &Client,
    session: &Session,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    dry_run: bool,
) -> Result<PushOutcome> {
    let watermark = sync_state::get_push_mark(db)?;
    // `max_ms` is the largest `modify_time_ms` over exactly the packed rows,
    // computed in the same query as the filter — re-using it as the next
    // watermark cannot skip a row (see `pack_changeset_since_ms`).
    let (changeset, max_ms) = pack_changeset_since_ms(db, watermark)?;
    let entry_count = changeset_entry_count(&changeset);
    if entry_count == 0 {
        return Ok(PushOutcome::default());
    }

    if dry_run {
        return Ok(PushOutcome {
            pushed: entry_count,
            seq_rebased: None,
        });
    }

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

    // Predict our next per-device seq and bind it into the changeset AAD
    // at encrypt time (the relay can't stamp a seq into the AAD after the
    // fact, and the predict-and-check also detects device_id collisions).
    // The relay validates the prediction *before* storing: on a mismatch
    // it returns its authoritative `expected` seq and we recover in-band
    // — re-base `device_seq`, RE-ENCRYPT (the AAD depends on seq), and
    // retry once. This makes a seq desync (crash between push and the
    // watermark advance, DB restore, relay reset) recoverable instead of
    // a permanent softlock. Only a *second* mismatch is a genuine
    // device_id collision and is fatal (da-ca4q).
    let predicted = sync_state::get_device_seq(db)?.saturating_add(1);
    // The AAD also binds `cluster_id` (da-3nkx) so a blob cannot be
    // unsealed against a different cluster — pass `local_cluster` here and
    // again in the recovery re-encrypt below.
    let blob = encrypt_changeset(&changeset, master_key, device_id, predicted, &local_cluster)?;
    let push_with_seq = |seq: u64, blob: &EncryptedBlob| {
        session.run(client, |bearer| {
            push_blob(
                client,
                server,
                bearer,
                device_id,
                blob.as_bytes(),
                &local_cluster,
                local_generation,
                seq as i64,
            )
        })
    };

    let (ack, seq_rebased) = match push_with_seq(predicted, &blob) {
        Ok(ack) => {
            sync_state::advance_device_seq(db, predicted)?;
            (ack, None)
        }
        Err(SessionError::SeqMismatch(SyncProtocolError::SeqMismatch { expected, .. })) => {
            let expected = expected.max(0) as u64;
            let reblob =
                encrypt_changeset(&changeset, master_key, device_id, expected, &local_cluster)?;
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
            // The relay is authoritative here and `expected` may sit
            // *below* our stored value (relay reset / older-backup
            // restore), so force-set past the monotonic guard.
            sync_state::set_device_seq(db, expected)?;
            // Surface the recovery as a typed event in the outcome —
            // the CLI prints a warning, the daemon logs it. No printing
            // from library code (da-60gf).
            let rebase = SeqRebase {
                device_id: device_id.to_string(),
                predicted,
                expected,
            };
            (ack, Some(rebase))
        }
        Err(other) => return Err(other.into()),
    };

    // Persist the cluster_id the server confirmed on push acceptance.
    // First sync (local_was_unset) always writes, since the locally-used
    // value was a device-id default; subsequent syncs only write if the
    // server bumped the cluster between push and acceptance.
    if local_was_unset || ack.cluster_id != local_cluster {
        sync_state::set_cluster_id(db, &ack.cluster_id)?;
    }
    if ack.generation != local_generation {
        sync_state::set_cluster_generation(db, ack.generation)?;
    }

    let advance_to = max_ms.unwrap_or(watermark);
    if advance_to > watermark {
        sync_state::advance_push_mark(db, advance_to)?;
    }

    Ok(PushOutcome {
        pushed: entry_count,
        seq_rebased,
    })
}

/// Fetch peers' changesets, decrypt, apply via LWW, and queue any
/// conflicts the apply uncovered.
pub fn pull_once(
    server: &str,
    client: &Client,
    session: &Session,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    dry_run: bool,
) -> Result<PullOutcome> {
    let since = sync_state::get_pull_seq(db)? as i64;
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
    let rows = session.run(client, |bearer| {
        pull_batch(
            client,
            server,
            bearer,
            device_id,
            since,
            &local_cluster,
            local_generation,
        )
    })?;
    if rows.is_empty() {
        return Ok(PullOutcome::default());
    }

    // Confirm the cluster_id / generation echoed back by the relay on
    // the first row. Every row in the batch shares the same
    // (cluster_id, generation) because the server filters by cluster
    // before generation checks. First sync (local_was_unset) always
    // writes, since the locally-used value was a default-string
    // placeholder; subsequent syncs only write if the server changed it.
    if let Some(first) = rows.first() {
        if local_was_unset || first.cluster_id != local_cluster {
            sync_state::set_cluster_id(db, &first.cluster_id)?;
        }
        if first.generation != local_generation {
            sync_state::set_cluster_generation(db, first.generation)?;
        }
    }

    if dry_run {
        return Ok(PullOutcome {
            applied: rows
                .iter()
                .map(|r| incoming_entry_count(r, master_key))
                .sum(),
            integrity_warnings_queued: 0,
        });
    }

    let mut outcome = PullOutcome::default();
    let mut max_seq: u64 = since as u64;

    for row in rows {
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

        // Snapshot which entries the incoming changeset carries BEFORE we
        // apply it — `apply_changeset` may drop older-modify_time rows via
        // LWW, but conflict detection cares about every row that made it
        // into the local table, not only the ones that actually changed.
        let incoming = parse_entries(cs.as_bytes())?;
        apply_changeset(db, &cs)?;
        outcome.applied += incoming.len();

        outcome.integrity_warnings_queued += file_warnings_for_incoming(db, &incoming)?;

        if (row.id as u64) > max_seq {
            max_seq = row.id as u64;
        }
    }

    if max_seq > since as u64 {
        sync_state::advance_pull_seq(db, max_seq)?;
    }

    Ok(outcome)
}

fn changeset_entry_count(cs: &Changeset) -> usize {
    entries_array(cs).map(|a| a.len()).unwrap_or(0)
}

/// Peek at the `entries` JSON array inside a packed changeset. The wire
/// format is locked in `docs/sync-protocol.md` as a 4-byte magic prefix +
/// JSON.
fn entries_array(cs: &Changeset) -> Option<Vec<serde_json::Value>> {
    let bytes = cs.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    let (_magic, json) = bytes.split_at(4);
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    value.get("entries").and_then(|v| v.as_array()).cloned()
}

/// After applying incoming entries via LWW, run the same non-blocking
/// overlap/double-open/orphan-end detector D5 uses for local writes — but
/// tag each detected row as `detected_by='sync_pull'` so the integrity-
/// warning UX (C4e) can distinguish the two sources.
fn file_warnings_for_incoming(db: &Db, incoming: &[Entry]) -> Result<usize> {
    if incoming.is_empty() {
        return Ok(0);
    }
    let tx = db.conn().unchecked_transaction()?;
    let mut filed = 0usize;
    for entry in incoming {
        let found = integrity::detect_for_entry(&tx, entry, DetectedBy::SyncPull)?;
        filed += found.len();
    }
    tx.commit()?;
    Ok(filed)
}

fn parse_entries(bytes: &[u8]) -> Result<Vec<Entry>> {
    if bytes.len() < 4 {
        return Ok(vec![]);
    }
    let (_magic, json) = bytes.split_at(4);
    let payload: serde_json::Value = serde_json::from_slice(json)?;
    let Some(arr) = payload.get("entries").and_then(|v| v.as_array()) else {
        return Ok(vec![]);
    };
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let entry: Entry = serde_json::from_value(v.clone())?;
        out.push(entry);
    }
    Ok(out)
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
    parse_entries(cs.as_bytes()).map(|v| v.len()).unwrap_or(0)
}
