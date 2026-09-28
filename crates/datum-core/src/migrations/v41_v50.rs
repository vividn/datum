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

/// The Easy Sync tables (da-k329q): `ops`, every signed change received or
/// made here, and `sync_local`, this copy's endpoint key. See `schema.sql`.
pub(super) fn m41_to_42_sync_ops(conn: &Connection) -> Result<()> {
    conn.execute_batch(SYNC_OPS_SQL)?;
    Ok(())
}

/// Reverse v41→v42. Drops the ops and the endpoint key: a v41 binary has
/// nothing that reads them, and the entries they produced stay.
pub(super) fn down_42_to_41_sync_ops(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS ops; DROP TABLE IF EXISTS sync_local;")?;
    Ok(())
}

/// The v42 tables exactly as `schema.sql` creates them.
const SYNC_OPS_SQL: &str = "
CREATE TABLE IF NOT EXISTS ops (
    endpoint TEXT NOT NULL,
    seq      INTEGER NOT NULL,
    hlc      TEXT NOT NULL,
    hid      TEXT NOT NULL,
    slug     TEXT NOT NULL,
    kind     TEXT NOT NULL,
    applied  INTEGER NOT NULL,
    body     TEXT NOT NULL,
    PRIMARY KEY (endpoint, seq)
);
CREATE INDEX IF NOT EXISTS ops_hid ON ops(hid);
CREATE INDEX IF NOT EXISTS ops_slug ON ops(slug);
CREATE INDEX IF NOT EXISTS ops_hlc ON ops(hlc);
CREATE TABLE IF NOT EXISTS sync_local (
    id              INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
    endpoint_secret TEXT NOT NULL,
    endpoint_id     TEXT NOT NULL,
    device_id       TEXT NOT NULL
);
";
