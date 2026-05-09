//! High-level "run a sync cycle" entry point.
//!
//! This module is the seam between the CLI / daemon and the wire protocol:
//! callers hand over a [`SyncConfig`] (server URL, bearer token, device id,
//! master key, direction, dry-run flag), an HTTP [`Client`], and an open
//! [`Db`], and get back a [`SyncReport`] tallying entries pushed, applied,
//! and conflicts queued. No `clap`, no stdout, no XDG paths — everything
//! the CLI surface in `datum-commands` once owned.
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
//! Push advances two counters: `push_seq` (max `modify_time` epoch-ms
//! packed) and `my_push_seq` (per-device counter that stays in lockstep
//! with the relay). Pull advances `pull_seq` to the relay's global row id.
//! See `docs/sync-protocol.md` §Sequence semantics.

use anyhow::Result;
use datum_core::conflicts::{self, DetectedBy};
use datum_core::db::Db;
use datum_core::entry::Entry;
use reqwest::blocking::Client;

use crate::crypto::changeset::{decrypt_changeset, encrypt_changeset, Changeset, EncryptedBlob};
use crate::http::session::Session;
use crate::http::{pull_batch, push_blob, PulledChangeset};
use crate::ops::{apply_changeset, pack_changeset};
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

/// Result of a push run — how many rows went up the wire this time.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PushOutcome {
    pub pushed: usize,
}

/// Result of a pull run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PullOutcome {
    /// Entries applied to the local DB (including LWW no-ops).
    pub applied: usize,
    /// Rows filed into the `sync_conflicts` queue with
    /// `detected_by='sync_pull'`.
    pub conflicts_queued: usize,
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
    let watermark = sync_state::get_push_seq(db)?;
    let changeset = pack_changeset(db, watermark)?;
    let entry_count = changeset_entry_count(&changeset);
    if entry_count == 0 {
        return Ok(PushOutcome::default());
    }

    if dry_run {
        return Ok(PushOutcome {
            pushed: entry_count,
        });
    }

    // Pre-compute next per-device seq, use it as AAD during encryption,
    // then assert the relay echoes it back. Any divergence means another
    // writer is using our device_id and we must abort before advancing
    // watermarks — peers replaying the AAD against the relay-assigned seq
    // would otherwise fail to decrypt.
    let next_seq = sync_state::get_my_push_seq(db)?.saturating_add(1);
    let blob = encrypt_changeset(&changeset, master_key, device_id, next_seq)?;
    let assigned_seq = session.run(client, |bearer| {
        push_blob(client, server, bearer, device_id, blob.as_bytes())
    })?;

    if assigned_seq as u64 != next_seq {
        anyhow::bail!(
            "relay assigned seq {} but we predicted {}; refusing to advance \
             watermarks — another writer may be using device_id {}",
            assigned_seq,
            next_seq,
            device_id,
        );
    }
    sync_state::advance_my_push_seq(db, next_seq)?;

    let advance_to = max_modify_time_ms(&changeset).unwrap_or(watermark);
    if advance_to > watermark {
        sync_state::advance_push_seq(db, advance_to)?;
    }

    Ok(PushOutcome {
        pushed: entry_count,
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
    let rows = session.run(client, |bearer| {
        pull_batch(client, server, bearer, device_id, since)
    })?;
    if rows.is_empty() {
        return Ok(PullOutcome::default());
    }

    if dry_run {
        return Ok(PullOutcome {
            applied: rows
                .iter()
                .map(|r| incoming_entry_count(r, master_key))
                .sum(),
            conflicts_queued: 0,
        });
    }

    let mut outcome = PullOutcome::default();
    let mut max_seq: u64 = since as u64;

    for row in rows {
        let blob = EncryptedBlob::from_bytes(row.payload.clone());
        // Per the spec, every decrypt failure must look alike — no oracle.
        let cs =
            decrypt_changeset(&blob, master_key, &row.device_id, row.seq as u64).map_err(|_| {
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

        outcome.conflicts_queued += file_conflicts_for_incoming(db, &incoming)?;

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

fn max_modify_time_ms(cs: &Changeset) -> Option<u64> {
    let arr = entries_array(cs)?;
    let mut max: Option<u64> = None;
    for entry in arr {
        let Some(mt) = entry.get("modify_time").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(dt) = chrono::DateTime::parse_from_rfc3339(mt) else {
            continue;
        };
        let ms = dt.timestamp_millis();
        if ms >= 0 {
            let ms = ms as u64;
            max = Some(match max {
                Some(cur) => cur.max(ms),
                None => ms,
            });
        }
    }
    max
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
/// tag each detected row as `detected_by='sync_pull'` so the conflict-
/// resolution UX (C4e) can distinguish the two sources.
fn file_conflicts_for_incoming(db: &Db, incoming: &[Entry]) -> Result<usize> {
    if incoming.is_empty() {
        return Ok(0);
    }
    let tx = db.conn().unchecked_transaction()?;
    let mut filed = 0usize;
    for entry in incoming {
        let found = conflicts::detect_for_entry(&tx, entry, DetectedBy::SyncPull)?;
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
    let Ok(cs) = decrypt_changeset(&blob, master_key, &row.device_id, row.seq as u64) else {
        return 0;
    };
    parse_entries(cs.as_bytes()).map(|v| v.len()).unwrap_or(0)
}
