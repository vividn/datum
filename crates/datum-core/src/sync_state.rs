//! Per-DB sync-state watermarks stored in the `sync_state` table.
//!
//! Every device tracks two monotonic counters:
//! - `push_mark` — the largest `modified_at` epoch-ms the relay has
//!   accepted for this device.
//! - `pull_seq` — the largest global seq returned by the relay in a
//!   pull response, so we can ask for "newer than this" next time.
//!
//! This is local database state, not a network concern: every helper
//! here reads and writes the user's own `sync_state` table and speaks to
//! nothing. It lives beside the rest of the db layer so a command that
//! renders a local view (`datum now` reading `last_pull_at`) doesn't
//! reach through an HTTP client crate to read its own table (da-9d6j).

use crate::db::Db;
use crate::error::{DatumError, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, OptionalExtension};

/// Default `db_id` when the caller doesn't distinguish identities. One row
/// per DB is the common case; callers can override by writing directly to
/// `sync_state` if they need multi-identity.
pub const DEFAULT_DB_ID: &str = "local";

/// Read the local `push_mark` watermark. Returns 0 if no row exists yet
/// (fresh DB, never synced).
pub fn get_push_mark(db: &Db) -> Result<u64> {
    read_seq_column(db, "push_mark")
}

/// Read the local `pull_seq` watermark.
pub fn get_pull_seq(db: &Db) -> Result<u64> {
    read_seq_column(db, "pull_seq")
}

/// Read the local per-device push counter. This mirrors the relay's
/// server-assigned `seq` for this device and is used as the AAD seq when
/// sealing outgoing changesets (the relay uses `MAX(seq)+1` per device, so
/// local `device_seq + 1` is the value the server will stamp).
pub fn get_device_seq(db: &Db) -> Result<u64> {
    read_seq_column(db, "device_seq")
}

/// Advance `push_mark` and stamp `last_push_at` (epoch-ms). A rewind to a
/// lower value is rejected — `push_mark` is monotonic.
pub fn advance_push_mark(db: &Db, new_seq: u64) -> Result<()> {
    advance_seq(db, "push_mark", "last_push_at", new_seq)
}

/// Advance `pull_seq` and stamp `last_pull_at` (epoch-ms).
pub fn advance_pull_seq(db: &Db, new_seq: u64) -> Result<()> {
    advance_seq(db, "pull_seq", "last_pull_at", new_seq)
}

/// Snapshot of how local entries split relative to a cluster's
/// `generation_started_at` and the local `push_mark`. Built by
/// [`partition_unsynced_by_generation`] for da-74xx recovery.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UnsyncedPartition {
    /// Total entries with `modify_time < generation_started_at` —
    /// pre-reinit stale state, all discarded during recovery.
    pub pre_reinit_total: u64,
    /// Subset of pre-reinit entries with `modify_time_ms > push_mark` —
    /// recent local edits the user hadn't pushed before the reinit. The
    /// "exportable" subset.
    pub pre_reinit_unsynced: u64,
    /// Entries with `modify_time >= generation_started_at` — post-reinit
    /// fresh work, preserved through recovery.
    pub post_reinit: u64,
}

/// Partition local entries by their `modify_time_ms` against the cluster's
/// `generation_started_at` (RFC 3339) and the local `push_mark`. Returns
/// counts only — see [`UnsyncedPartition`].
///
/// Uses the indexed `modify_time_ms` generated column (da-x3da) so this
/// is a single round-trip with conditional aggregation, not a scan.
pub fn partition_unsynced_by_generation(
    db: &Db,
    generation_started_at: &str,
) -> Result<UnsyncedPartition> {
    let push_mark = get_push_mark(db)? as i64;
    let gen_ms = parse_rfc3339_to_epoch_ms(generation_started_at)?;

    let row: (i64, i64, i64) = db
        .conn()
        .query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN modify_time_ms < ?1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN modify_time_ms > ?2 AND modify_time_ms < ?1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN modify_time_ms >= ?1 THEN 1 ELSE 0 END), 0)
             FROM entries",
            params![gen_ms, push_mark],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
    Ok(UnsyncedPartition {
        pre_reinit_total: row.0 as u64,
        pre_reinit_unsynced: row.1 as u64,
        post_reinit: row.2 as u64,
    })
}

/// Parse an RFC 3339 UTC timestamp into epoch-ms for comparison against
/// the indexed `modify_time_ms` generated column.
fn parse_rfc3339_to_epoch_ms(s: &str) -> Result<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map_err(|e| {
            DatumError::Other(format!(
                "generation_started_at must be RFC 3339, got {s:?}: {e}"
            ))
        })
        .map(|dt| dt.timestamp_millis())
}

/// Write the pre-reinit-unsynced subset (entries with
/// `modify_time_ms > push_mark AND modify_time_ms < generation_started_at`)
/// to `path` as one JSON-encoded entry per line. Returns the count
/// written. Used by da-74xx recovery to dump the user's recoverable
/// stale-edit set before the pre-reinit delete runs.
pub fn export_pre_reinit_unsynced_to_jsonl(
    db: &Db,
    generation_started_at: &str,
    path: &std::path::Path,
) -> Result<u64> {
    use std::io::Write;
    let push_mark = get_push_mark(db)? as i64;
    let gen_ms = parse_rfc3339_to_epoch_ms(generation_started_at)?;
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT * FROM entries \
         WHERE modify_time_ms > ?1 AND modify_time_ms < ?2 \
         ORDER BY modify_time_ms",
    )?;
    let rows = stmt.query_map(params![push_mark, gen_ms], crate::db::row_to_entry_public)?;
    // 0600 on Unix + `create_new` (O_EXCL) so the export isn't
    // group/world-readable and we never silently overwrite an existing
    // file (Copilot review on PR #8). The caller's filename embeds
    // millisecond precision, so a collision here is a real anomaly we
    // want to surface as an error rather than hide.
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(path)
        .map_err(|e| DatumError::Other(format!("create recovery export {path:?}: {e}")))?;
    let mut count = 0u64;
    for row in rows {
        let entry = row?;
        let line = serde_json::to_string(&entry)
            .map_err(|e| DatumError::Other(format!("serialise entry to JSONL: {e}")))?;
        writeln!(file, "{line}")
            .map_err(|e| DatumError::Other(format!("write entry to {path:?}: {e}")))?;
        count += 1;
    }
    Ok(count)
}

/// Delete all entries with `modify_time_ms < generation_started_at`. The
/// peer mirrors the cluster's reinit by discarding pre-reinit state
/// (da-74xx); the caller must have already exported any user-recoverable
/// subset via [`export_pre_reinit_unsynced_to_jsonl`] when appropriate.
/// Returns the deletion count.
pub fn delete_pre_reinit_entries(db: &Db, generation_started_at: &str) -> Result<u64> {
    let gen_ms = parse_rfc3339_to_epoch_ms(generation_started_at)?;
    let n = db.conn().execute(
        "DELETE FROM entries WHERE modify_time_ms < ?1",
        params![gen_ms],
    )?;
    Ok(n as u64)
}

/// Apply the DB-mutation portion of GenerationMismatch recovery
/// atomically: delete all pre-reinit entries, reset the pull watermark
/// to 0, adopt the new cluster generation — all under one BEGIN/COMMIT
/// so a process death partway through can't leave the DB half-recovered
/// (Copilot review on PR #8). Returns the deletion count; a ROLLBACK is
/// best-effort on inner failure before the original error is re-raised.
pub fn apply_recovery_mutations(
    db: &Db,
    generation_started_at: &str,
    expected_generation: u64,
) -> Result<u64> {
    let conn = db.conn();
    conn.execute("BEGIN", [])?;
    let inner: Result<u64> = (|| {
        let deleted = delete_pre_reinit_entries(db, generation_started_at)?;
        reset_pull_seq(db, 0)?;
        set_cluster_generation(db, expected_generation)?;
        Ok(deleted)
    })();
    match inner {
        Ok(deleted) => {
            conn.execute("COMMIT", [])?;
            Ok(deleted)
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", []);
            Err(e)
        }
    }
}

/// Force `pull_seq` to an exact value, bypassing the monotonic guard in
/// [`advance_pull_seq`]. Used ONLY by GenerationMismatch recovery
/// (da-74xx): on cluster reinit, the relay's new-generation row ids
/// restart from a lower value than the peer's stored watermark, so the
/// watermark MUST rewind to re-pull the post-reinit snapshot. Normal pull
/// paths keep using `advance_pull_seq` — its guard catches programming
/// errors where the recovery isn't the cause of the rewind.
pub fn reset_pull_seq(db: &Db, new_seq: u64) -> Result<()> {
    let now_ms = Utc::now().timestamp_millis();
    db.conn().execute(
        "INSERT INTO sync_state (db_id, pull_seq, last_pull_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(db_id) DO UPDATE SET
            pull_seq     = excluded.pull_seq,
            last_pull_at = excluded.last_pull_at",
        params![DEFAULT_DB_ID, new_seq as i64, now_ms],
    )?;
    Ok(())
}

/// Record the server-acknowledged per-device seq. Expected to be exactly
/// the value the client predicted via `get_device_seq() + 1`; a relay
/// that returns a mismatched seq indicates either a protocol bug or a
/// second writer on the same device_id, so we surface it as an error.
pub fn advance_device_seq(db: &Db, new_seq: u64) -> Result<()> {
    let current = get_device_seq(db)?;
    if new_seq < current {
        return Err(DatumError::Other(format!(
            "sync_state.device_seq rewind rejected: stored {current}, asked for {new_seq}"
        )));
    }
    set_device_seq(db, new_seq)
}

/// Force `device_seq` to an exact value, bypassing the monotonic guard
/// in [`advance_device_seq`]. Used ONLY by the seq-mismatch recovery
/// path (da-ca4q): the relay handed back its authoritative next seq, and
/// that value may legitimately be *lower* than our stored one (relay
/// reset, or a DB restored from a backup the relay has since moved past).
/// Treating the relay as authoritative here is what un-softlocks the
/// client — the normal push path keeps using `advance_device_seq`.
pub fn set_device_seq(db: &Db, new_seq: u64) -> Result<()> {
    let now_ms = Utc::now().timestamp_millis();
    db.conn().execute(
        "INSERT INTO sync_state (db_id, device_seq, last_push_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(db_id) DO UPDATE SET
            device_seq  = excluded.device_seq,
            last_push_at = excluded.last_push_at",
        params![DEFAULT_DB_ID, new_seq as i64, now_ms],
    )?;
    Ok(())
}

/// Force `push_mark` to an exact value, bypassing the monotonic guard in
/// [`advance_push_mark`]. Used ONLY by the replaced-relay recovery
/// (da-od2t): the watermark's whole meaning is "the relay already has
/// everything up to here", so once the relay demonstrably does not, the
/// watermark is not stale — it is wrong, and rewinding it is the repair.
pub fn reset_push_mark(db: &Db, new_seq: u64) -> Result<()> {
    let now_ms = Utc::now().timestamp_millis();
    db.conn().execute(
        "INSERT INTO sync_state (db_id, push_mark, last_push_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(db_id) DO UPDATE SET
            push_mark    = excluded.push_mark,
            last_push_at = excluded.last_push_at",
        params![DEFAULT_DB_ID, new_seq as i64, now_ms],
    )?;
    Ok(())
}

/// React to the relay stating that the highest per-device `seq` it holds
/// for us is `relay_high_seq` (da-od2t).
///
/// A relay below our own `device_seq` has lost changesets we already
/// pushed — it was replaced, or restored from a backup predating them.
/// Nothing local is wrong, but three watermarks now lie:
///
/// - `device_seq` claims a push counter the relay never reached,
/// - `push_mark` claims the relay holds our history up to that point,
/// - `pull_seq` claims a relay-global row id the restarted id space has
///   not issued yet, so every re-pushed peer row would sort below it.
///
/// Rewinding all three is what makes a relay genuinely replaceable: the
/// next push re-sends this device's whole history rather than declining
/// to re-send what the relay is silently missing. Re-pushing and
/// re-pulling are idempotent (content-hash dedup + LWW apply), so the
/// only cost of a false positive is bandwidth.
///
/// Returns `Some(missing)` — how many of our pushes the relay has lost —
/// when a rewind happened, `None` when the relay is level or ahead.
pub fn rewind_for_replaced_relay(db: &Db, relay_high_seq: u64) -> Result<Option<u64>> {
    let local = get_device_seq(db)?;
    if relay_high_seq >= local {
        return Ok(None);
    }
    let conn = db.conn();
    conn.execute("BEGIN", [])?;
    let inner: Result<()> = (|| {
        set_device_seq(db, relay_high_seq)?;
        reset_push_mark(db, 0)?;
        reset_pull_seq(db, 0)?;
        Ok(())
    })();
    match inner {
        Ok(()) => {
            conn.execute("COMMIT", [])?;
            Ok(Some(local - relay_high_seq))
        }
        Err(e) => {
            let _ = conn.execute("ROLLBACK", []);
            Err(e)
        }
    }
}

fn read_seq_column(db: &Db, col: &str) -> Result<u64> {
    let sql = format!("SELECT {col} FROM sync_state WHERE db_id = ?1",);
    let got: Option<i64> = db
        .conn()
        .query_row(&sql, params![DEFAULT_DB_ID], |row| row.get(0))
        .optional()?;
    Ok(got.unwrap_or(0) as u64)
}

fn advance_seq(db: &Db, seq_col: &str, at_col: &str, new_seq: u64) -> Result<()> {
    let current = read_seq_column(db, seq_col)?;
    if new_seq < current {
        return Err(DatumError::Other(format!(
            "sync_state.{seq_col} rewind rejected: stored {current}, asked for {new_seq}"
        )));
    }
    let now_ms = Utc::now().timestamp_millis();
    let sql = format!(
        "INSERT INTO sync_state (db_id, {seq_col}, {at_col})
         VALUES (?1, ?2, ?3)
         ON CONFLICT(db_id) DO UPDATE SET
            {seq_col} = excluded.{seq_col},
            {at_col}  = excluded.{at_col}",
    );
    db.conn()
        .execute(&sql, params![DEFAULT_DB_ID, new_seq as i64, now_ms])?;
    Ok(())
}

/// Read the locally-stored cluster_id assigned by the relay on first
/// sync. Returns `None` when no sync has completed yet — the caller
/// should send `cluster_id = None` on the next push/pull and persist
/// whatever the server echoes back.
pub fn get_cluster_id(db: &Db) -> Result<Option<String>> {
    let got: Option<Option<String>> = db
        .conn()
        .query_row(
            "SELECT cluster_id FROM sync_state WHERE db_id = ?1",
            params![DEFAULT_DB_ID],
            |row| row.get(0),
        )
        .optional()?;
    Ok(got.flatten())
}

/// The cluster_id a DB falls back to when none has been explicitly
/// recorded — no prior sync and no `sync login --cluster <name>`
/// override. Per da-3fnd we default to the DB's file-name stem
/// (`vremya.db` → `vremya`), so two devices tracking a same-named DB
/// land on the same cluster and sync with zero config, while
/// differently-named DBs stay isolated. In-memory or path-less DBs
/// (tests) fall back to the literal `"default"`.
///
/// The DB's own path is read off `pragma_database_list` — the same seam
/// the CLI uses to print the bound path — so this needs no cross-layer
/// plumbing despite the (now-stale) TODO that claimed otherwise.
pub fn default_cluster_id(db: &Db) -> String {
    db.conn()
        .query_row(
            "SELECT file FROM pragma_database_list WHERE name = 'main'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .filter(|p| !p.is_empty())
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

/// Persist the relay-assigned `cluster_id` for this DB. Replacing a
/// stored cluster_id with a different one is unusual (would mean the
/// user re-bound the DB to a different cluster) but allowed — the
/// generation counter is unrelated and stays untouched here.
pub fn set_cluster_id(db: &Db, cluster_id: &str) -> Result<()> {
    db.conn().execute(
        "INSERT INTO sync_state (db_id, cluster_id)
         VALUES (?1, ?2)
         ON CONFLICT(db_id) DO UPDATE SET cluster_id = excluded.cluster_id",
        params![DEFAULT_DB_ID, cluster_id],
    )?;
    Ok(())
}

/// Read the last-seen cluster generation. Returns 0 when no row exists
/// or the column was never written.
pub fn get_cluster_generation(db: &Db) -> Result<u64> {
    read_seq_column(db, "cluster_generation")
}

/// Record the cluster generation the server echoed on the most recent
/// push/pull. Unlike `push_mark`/`pull_seq`, this counter is NOT
/// monotonic from the client's perspective — a fresh DB attaching to a
/// long-lived cluster observes whatever generation the server is
/// already on, which may be > 0. Overwrites unconditionally.
pub fn set_cluster_generation(db: &Db, generation: u64) -> Result<()> {
    db.conn().execute(
        "INSERT INTO sync_state (db_id, cluster_generation)
         VALUES (?1, ?2)
         ON CONFLICT(db_id) DO UPDATE SET cluster_generation = excluded.cluster_generation",
        params![DEFAULT_DB_ID, generation as i64],
    )?;
    Ok(())
}

/// Read the `last_push_at` stamp (epoch-ms), if any push has been recorded.
pub fn get_last_push_at(db: &Db) -> Result<Option<DateTime<Utc>>> {
    read_at_column(db, "last_push_at")
}

/// Read the `last_pull_at` stamp (epoch-ms), if any pull has been recorded.
pub fn get_last_pull_at(db: &Db) -> Result<Option<DateTime<Utc>>> {
    read_at_column(db, "last_pull_at")
}

fn read_at_column(db: &Db, col: &str) -> Result<Option<DateTime<Utc>>> {
    let sql = format!("SELECT {col} FROM sync_state WHERE db_id = ?1",);
    let got: Option<Option<i64>> = db
        .conn()
        .query_row(&sql, params![DEFAULT_DB_ID], |row| row.get(0))
        .optional()?;
    match got.flatten() {
        Some(ms) => Ok(Some(Utc.timestamp_millis_opt(ms).single().ok_or_else(
            || DatumError::Other(format!("sync_state.{col} out of range: {ms}")),
        )?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn seq_advances_correctly() {
        let db = fresh_db();
        assert_eq!(get_push_mark(&db).unwrap(), 0);
        assert_eq!(get_pull_seq(&db).unwrap(), 0);
        assert!(get_last_push_at(&db).unwrap().is_none());

        advance_push_mark(&db, 5).unwrap();
        assert_eq!(get_push_mark(&db).unwrap(), 5);
        assert!(get_last_push_at(&db).unwrap().is_some());

        advance_push_mark(&db, 12).unwrap();
        assert_eq!(get_push_mark(&db).unwrap(), 12);

        // Rewind is rejected — monotonic watermark.
        assert!(advance_push_mark(&db, 10).is_err());
        assert_eq!(get_push_mark(&db).unwrap(), 12);

        // push and pull are independent columns.
        advance_pull_seq(&db, 7).unwrap();
        assert_eq!(get_pull_seq(&db).unwrap(), 7);
        assert_eq!(get_push_mark(&db).unwrap(), 12);
    }

    /// da-ca4q: `advance_device_seq` is monotonic, but the seq-mismatch
    /// recovery path uses `set_device_seq` to force the relay's
    /// authoritative value — which may legitimately rewind *below* the
    /// stored seq (relay reset / older-backup restore). The forced set
    /// must succeed where the monotonic advance refuses.
    #[test]
    fn set_device_seq_forces_rebase_past_monotonic_guard() {
        let db = fresh_db();
        advance_device_seq(&db, 10).unwrap();
        assert_eq!(get_device_seq(&db).unwrap(), 10);

        // Monotonic guard rejects a rewind...
        assert!(advance_device_seq(&db, 3).is_err());
        assert_eq!(get_device_seq(&db).unwrap(), 10);

        // ...but the recovery force-set rebases down to the relay's value.
        set_device_seq(&db, 3).unwrap();
        assert_eq!(get_device_seq(&db).unwrap(), 3);

        // And forward re-base works too (the common crash-recovery case).
        set_device_seq(&db, 8).unwrap();
        assert_eq!(get_device_seq(&db).unwrap(), 8);
    }

    /// da-74xx: `reset_pull_seq` force-sets the pull watermark, including
    /// rewinding it past `advance_pull_seq`'s monotonic guard. Used on
    /// GenerationMismatch recovery — the relay's new-generation row ids
    /// restart from a lower value than the peer's stored watermark.
    #[test]
    fn reset_pull_seq_bypasses_monotonic_guard() {
        let db = fresh_db();
        advance_pull_seq(&db, 100).unwrap();
        assert_eq!(get_pull_seq(&db).unwrap(), 100);

        // Normal advance rejects the rewind.
        assert!(advance_pull_seq(&db, 50).is_err());
        assert_eq!(get_pull_seq(&db).unwrap(), 100);

        // reset_pull_seq force-rewinds.
        reset_pull_seq(&db, 5).unwrap();
        assert_eq!(get_pull_seq(&db).unwrap(), 5);

        // And resets to 0 (the canonical recovery case — re-pull
        // everything at the new generation).
        reset_pull_seq(&db, 0).unwrap();
        assert_eq!(get_pull_seq(&db).unwrap(), 0);
    }

    /// da-od2t: a relay that reports a lower per-device high seq than we
    /// hold has lost pushes we made, so all three watermarks rewind and
    /// the next push re-sends the whole history. A relay that is level
    /// or ahead leaves them untouched.
    #[test]
    fn rewind_for_replaced_relay_only_fires_when_the_relay_is_behind() {
        let db = fresh_db();
        advance_device_seq(&db, 3).unwrap();
        advance_push_mark(&db, 1_700_000_000_000).unwrap();
        advance_pull_seq(&db, 42).unwrap();

        // Level: nothing to repair.
        assert_eq!(rewind_for_replaced_relay(&db, 3).unwrap(), None);
        assert_eq!(get_push_mark(&db).unwrap(), 1_700_000_000_000);
        assert_eq!(get_pull_seq(&db).unwrap(), 42);

        // Ahead (a peer pushed under our device_id, or we crashed before
        // recording our own push): still not a replaced relay.
        assert_eq!(rewind_for_replaced_relay(&db, 9).unwrap(), None);
        assert_eq!(get_device_seq(&db).unwrap(), 3);

        // Behind: the relay is missing 3 of our pushes.
        assert_eq!(rewind_for_replaced_relay(&db, 0).unwrap(), Some(3));
        assert_eq!(get_device_seq(&db).unwrap(), 0);
        assert_eq!(get_push_mark(&db).unwrap(), 0);
        assert_eq!(get_pull_seq(&db).unwrap(), 0);
    }

    /// da-eu7o: a fresh DB reports no cluster_id and generation 0; the
    /// setters round-trip; persisting a cluster_id without touching the
    /// generation (and vice-versa) leaves the other column intact.
    #[test]
    fn cluster_id_and_generation_round_trip_independently() {
        let db = fresh_db();
        assert_eq!(get_cluster_id(&db).unwrap(), None);
        assert_eq!(get_cluster_generation(&db).unwrap(), 0);

        set_cluster_id(&db, "cluster-X").unwrap();
        assert_eq!(get_cluster_id(&db).unwrap().as_deref(), Some("cluster-X"));
        // Setting cluster_id must not disturb the generation default.
        assert_eq!(get_cluster_generation(&db).unwrap(), 0);

        set_cluster_generation(&db, 42).unwrap();
        assert_eq!(get_cluster_generation(&db).unwrap(), 42);
        assert_eq!(get_cluster_id(&db).unwrap().as_deref(), Some("cluster-X"));

        // Re-binding to a different cluster is allowed (uncommon but
        // legal — the cluster generation persists across the rebind so
        // the client doesn't pretend it's a fresh attach; the caller is
        // expected to bump-generation if it intends to fork).
        set_cluster_id(&db, "cluster-Y").unwrap();
        assert_eq!(get_cluster_id(&db).unwrap().as_deref(), Some("cluster-Y"));
        assert_eq!(get_cluster_generation(&db).unwrap(), 42);

        // Generation is NOT monotonic from the client's point of view
        // (a fresh DB attaching to an existing cluster may see a lower
        // value). Overwrite without a rewind check.
        set_cluster_generation(&db, 7).unwrap();
        assert_eq!(get_cluster_generation(&db).unwrap(), 7);
    }

    /// da-3fnd: the file-name stem of a file-backed DB is its default
    /// cluster; an in-memory DB (no path) falls back to `"default"`.
    #[test]
    fn default_cluster_id_is_file_stem_else_default() {
        // In-memory DBs have no backing file → literal "default".
        let mem = fresh_db();
        assert_eq!(default_cluster_id(&mem), "default");

        // A file-backed DB derives its cluster from the file stem.
        let dir = std::env::temp_dir().join(format!("datum-cluster-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vremya.db");
        let file_db = Db::open(path.to_str().unwrap()).unwrap();
        assert_eq!(default_cluster_id(&file_db), "vremya");
        // The setter still wins over the default once recorded.
        set_cluster_id(&file_db, "explicit").unwrap();
        assert_eq!(
            get_cluster_id(&file_db).unwrap().as_deref(),
            Some("explicit")
        );

        drop(file_db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
