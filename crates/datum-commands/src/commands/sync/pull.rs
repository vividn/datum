//! `datum sync [--pull]` — fetch peers' encrypted changesets and apply them
//! to the local DB.
//!
//! For each pulled row:
//! 1. base64 is already decoded by `datum_sync_client::http::pull_batch`
//! 2. decrypt with the master key, binding to `(peer_device_id, seq)`
//! 3. apply entries via LWW (last-writer-wins on `modify_time`)
//! 4. run post-insert conflict detection for every applied entry — any
//!    double-open / orphan-end / overlap lands in `sync_conflicts` with
//!    `detected_by = 'sync_pull'`. This is non-blocking: the pull
//!    continues even if a conflict is queued.
//!
//! Watermark: the per-device `pull_seq` tracks the relay's *global* row id
//! (not the per-device seq). After a successful batch, we advance it to
//! the last row's `seq` so the next pull asks for rows strictly newer.

use anyhow::Result;
use datum_core::conflicts::{self, DetectedBy};
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_sync_client::crypto::changeset::{decrypt_changeset, EncryptedBlob};
use datum_sync_client::http::{pull_batch, PulledChangeset};
use datum_sync_client::ops::apply_changeset;
use datum_sync_client::sync_state;
use reqwest::blocking::Client;

/// Output of a pull run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PullOutcome {
    /// Entries applied to the local DB (including LWW no-ops).
    pub applied: usize,
    /// Rows filed into the `sync_conflicts` queue with `detected_by='sync_pull'`.
    pub conflicts_queued: usize,
}

pub fn run(
    server: &str,
    client: &Client,
    bearer: &str,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    dry_run: bool,
) -> Result<PullOutcome> {
    let since = sync_state::get_pull_seq(db)? as i64;
    let rows = pull_batch(client, server, bearer, device_id, since)?;
    if rows.is_empty() {
        return Ok(PullOutcome::default());
    }

    if dry_run {
        return Ok(PullOutcome {
            applied: rows.iter().map(|r| incoming_entry_count(r, master_key)).sum(),
            conflicts_queued: 0,
        });
    }

    let mut outcome = PullOutcome::default();
    let mut max_seq: u64 = since as u64;

    for row in rows {
        let blob = EncryptedBlob::from_bytes(row.payload.clone());
        // Per the spec, every decrypt failure must look alike — no oracle.
        let cs = decrypt_changeset(&blob, master_key, &row.device_id, row.seq as u64)
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

/// After applying incoming entries via LWW, run the same non-blocking
/// overlap/double-open/orphan-end detector that `D5` uses for local
/// writes — but tag each detected row as `detected_by='sync_pull'` so the
/// conflict-resolution UX (C4e) can distinguish the two sources.
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
