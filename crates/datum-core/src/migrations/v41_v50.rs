//! Migration bodies for schema versions 40 → 50 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 50,
//! start `v51_v60.rs` rather than growing this file.

use rusqlite::Connection;

use crate::error::Result;

/// da-i7hig: the relay sync is deleted, and `sync_state` held only its
/// bookkeeping — push/pull watermarks, the device seq, the cluster id and
/// generation. Nothing else reads it, so the table goes.
pub(super) fn m40_to_41_drop_sync_state(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS sync_state;")?;
    Ok(())
}

/// Reverse v40→v41: put the v40 `sync_state` back, EMPTY. What it held
/// described a relay that no longer exists, so there is nothing to restore.
pub(super) fn down_41_to_40_drop_sync_state(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sync_state (
            db_id              TEXT NOT NULL,
            push_mark          INTEGER NOT NULL DEFAULT 0,
            pull_seq           INTEGER NOT NULL DEFAULT 0,
            device_seq         INTEGER NOT NULL DEFAULT 0,
            last_push_at       INTEGER,
            last_pull_at       INTEGER,
            cluster_id         TEXT,
            cluster_generation INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (db_id)
        );",
    )?;
    Ok(())
}
