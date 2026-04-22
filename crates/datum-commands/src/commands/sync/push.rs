//! `datum sync [--push]` — pack local entries since the last push watermark,
//! encrypt them, and upload to the relay.
//!
//! Two counters move forward per successful push:
//!
//! - **`push_seq`** — the max `modified_at` epoch-ms we've packed. Becomes
//!   the `since_seq` filter for the next call to `pack_changeset`.
//! - **`my_push_seq`** — a per-device push counter that stays in lockstep
//!   with the relay's server-assigned seq. We pre-compute
//!   `next = my_push_seq + 1`, use it as the AAD seq during encryption,
//!   then assert the relay returns that same value. This keeps every
//!   peer pulling our changesets able to reconstruct the AAD.

use anyhow::Result;
use datum_core::db::Db;
use datum_sync_client::crypto::changeset::encrypt_changeset;
use datum_sync_client::http::push_blob;
use datum_sync_client::ops::pack_changeset;
use datum_sync_client::sync_state;
use datum_sync_client::Changeset;
use reqwest::blocking::Client;

/// Result of a push run — how many rows went up the wire this time.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PushOutcome {
    pub pushed: usize,
}

/// Pack changes since the last push watermark, encrypt, and upload. A
/// `dry_run` packs and counts but skips both the HTTP call and the
/// watermark advance.
pub fn run(
    server: &str,
    client: &Client,
    bearer: &str,
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

    let next_seq = sync_state::get_my_push_seq(db)?.saturating_add(1);
    let blob = encrypt_changeset(&changeset, master_key, device_id, next_seq)?;
    let assigned_seq = push_blob(client, server, bearer, device_id, blob.as_bytes())?;

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

/// Peek at the `entries` JSON array inside a packed changeset without
/// exposing an internal helper from `datum-sync-client`. The wire format
/// is locked in `docs/sync-protocol.md` as a 4-byte magic prefix + JSON.
fn entries_array(cs: &Changeset) -> Option<Vec<serde_json::Value>> {
    let bytes = cs.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    let (_magic, json) = bytes.split_at(4);
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    value
        .get("entries")
        .and_then(|v| v.as_array())
        .cloned()
}
