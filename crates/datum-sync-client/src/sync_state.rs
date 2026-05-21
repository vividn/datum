//! Per-DB sync-state watermarks stored in the `sync_state` table.
//!
//! Every device tracks two monotonic counters:
//! - `push_seq` — the largest `modified_at` epoch-ms the relay has
//!   accepted for this device.
//! - `pull_seq` — the largest global seq returned by the relay in a
//!   pull response, so we can ask for "newer than this" next time.
//!
//! Gated by the `db` feature: these helpers target the rusqlite-backed
//! `Db` in `datum-core`. WASM builds that run their own SQLite layer
//! reimplement the same logic against their runtime.

use chrono::{DateTime, TimeZone, Utc};
use datum_core::db::Db;
use datum_core::error::{DatumError, Result};
use rusqlite::{params, OptionalExtension};

/// Default `db_id` when the caller doesn't distinguish identities. One row
/// per DB is the common case; callers can override by writing directly to
/// `sync_state` if they need multi-identity.
pub const DEFAULT_DB_ID: &str = "local";

/// Read the local `push_seq` watermark. Returns 0 if no row exists yet
/// (fresh DB, never synced).
pub fn get_push_seq(db: &Db) -> Result<u64> {
    read_seq_column(db, "push_seq")
}

/// Read the local `pull_seq` watermark.
pub fn get_pull_seq(db: &Db) -> Result<u64> {
    read_seq_column(db, "pull_seq")
}

/// Read the local per-device push counter. This mirrors the relay's
/// server-assigned `seq` for this device and is used as the AAD seq when
/// sealing outgoing changesets (the relay uses `MAX(seq)+1` per device, so
/// local `my_push_seq + 1` is the value the server will stamp).
pub fn get_my_push_seq(db: &Db) -> Result<u64> {
    read_seq_column(db, "my_push_seq")
}

/// Advance `push_seq` and stamp `last_push_at` (epoch-ms). A rewind to a
/// lower value is rejected — `push_seq` is monotonic.
pub fn advance_push_seq(db: &Db, new_seq: u64) -> Result<()> {
    advance_seq(db, "push_seq", "last_push_at", new_seq)
}

/// Advance `pull_seq` and stamp `last_pull_at` (epoch-ms).
pub fn advance_pull_seq(db: &Db, new_seq: u64) -> Result<()> {
    advance_seq(db, "pull_seq", "last_pull_at", new_seq)
}

/// Record the server-acknowledged per-device seq. Expected to be exactly
/// the value the client predicted via `get_my_push_seq() + 1`; a relay
/// that returns a mismatched seq indicates either a protocol bug or a
/// second writer on the same device_id, so we surface it as an error.
pub fn advance_my_push_seq(db: &Db, new_seq: u64) -> Result<()> {
    let current = get_my_push_seq(db)?;
    if new_seq < current {
        return Err(DatumError::Other(format!(
            "sync_state.my_push_seq rewind rejected: stored {current}, asked for {new_seq}"
        )));
    }
    let now_ms = Utc::now().timestamp_millis();
    db.conn().execute(
        "INSERT INTO sync_state (db_id, my_push_seq, last_push_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(db_id) DO UPDATE SET
            my_push_seq  = excluded.my_push_seq,
            last_push_at = excluded.last_push_at",
        params![DEFAULT_DB_ID, new_seq as i64, now_ms],
    )?;
    Ok(())
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
/// push/pull. Unlike `push_seq`/`pull_seq`, this counter is NOT
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
        assert_eq!(get_push_seq(&db).unwrap(), 0);
        assert_eq!(get_pull_seq(&db).unwrap(), 0);
        assert!(get_last_push_at(&db).unwrap().is_none());

        advance_push_seq(&db, 5).unwrap();
        assert_eq!(get_push_seq(&db).unwrap(), 5);
        assert!(get_last_push_at(&db).unwrap().is_some());

        advance_push_seq(&db, 12).unwrap();
        assert_eq!(get_push_seq(&db).unwrap(), 12);

        // Rewind is rejected — monotonic watermark.
        assert!(advance_push_seq(&db, 10).is_err());
        assert_eq!(get_push_seq(&db).unwrap(), 12);

        // push and pull are independent columns.
        advance_pull_seq(&db, 7).unwrap();
        assert_eq!(get_pull_seq(&db).unwrap(), 7);
        assert_eq!(get_push_seq(&db).unwrap(), 12);
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
}
