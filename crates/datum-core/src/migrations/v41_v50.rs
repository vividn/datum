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

/// Easy Sync over iroh (da-rugg8): `sync_peers`, the copies this one syncs
/// with, and `sync_invites`, the one-time invites it has handed out. See
/// `schema.sql`.
pub(super) fn m42_to_43_sync_peers(conn: &Connection) -> Result<()> {
    conn.execute_batch(SYNC_PEERS_SQL)?;
    Ok(())
}

/// Reverse v42→v43: forget the peers and the invites. Both are local.
pub(super) fn down_43_to_42_sync_peers(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS sync_peers; DROP TABLE IF EXISTS sync_invites;")?;
    Ok(())
}

/// da-p79vq: `sync_local.path`, the file the endpoint key was minted at, so
/// a copy at another path mints its own. An existing key gets `''`, which
/// the next write takes to mean this path: no key is re-minted, and no
/// pairing lost.
pub(super) fn m43_to_44_sync_local_path(conn: &Connection) -> Result<()> {
    conn.execute_batch("ALTER TABLE sync_local ADD COLUMN path TEXT NOT NULL DEFAULT '';")?;
    Ok(())
}

/// Reverse v43→v44.
pub(super) fn down_44_to_43_sync_local_path(conn: &Connection) -> Result<()> {
    conn.execute_batch("ALTER TABLE sync_local DROP COLUMN path;")?;
    Ok(())
}

/// The sync chain (da-odwc7): `sync_notices`, the loud warning's memory of
/// which members and admins this copy has announced and had acknowledged.
pub(super) fn m44_to_45_sync_notices(conn: &Connection) -> Result<()> {
    conn.execute_batch(SYNC_NOTICES_SQL)?;
    Ok(())
}

/// Reverse v44→v45. Local: the notices are gone, the chain's ops stay.
pub(super) fn down_45_to_44_sync_notices(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS sync_notices;")?;
    Ok(())
}

/// da-rcb5m: `sync_peers` remembers each peer's last session — its vector,
/// verify and error, each with its time — for `datum sync status`.
pub(super) fn m45_to_46_sync_peer_sessions(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE sync_peers ADD COLUMN last_vector TEXT;
         ALTER TABLE sync_peers ADD COLUMN last_verify_at TEXT;
         ALTER TABLE sync_peers ADD COLUMN last_verify_match INTEGER;
         ALTER TABLE sync_peers ADD COLUMN last_error TEXT;
         ALTER TABLE sync_peers ADD COLUMN last_error_at TEXT;",
    )?;
    Ok(())
}

/// Reverse v45→v46. Local: the session results are gone, the peers stay.
pub(super) fn down_46_to_45_sync_peer_sessions(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE sync_peers DROP COLUMN last_vector;
         ALTER TABLE sync_peers DROP COLUMN last_verify_at;
         ALTER TABLE sync_peers DROP COLUMN last_verify_match;
         ALTER TABLE sync_peers DROP COLUMN last_error;
         ALTER TABLE sync_peers DROP COLUMN last_error_at;",
    )?;
    Ok(())
}

/// The v45 table exactly as `schema.sql` creates it.
const SYNC_NOTICES_SQL: &str = "
CREATE TABLE IF NOT EXISTS sync_notices (
    subject   TEXT NOT NULL PRIMARY KEY,
    kind      TEXT NOT NULL,
    label     TEXT NOT NULL,
    at        TEXT NOT NULL,
    via       TEXT NOT NULL,
    announced INTEGER NOT NULL DEFAULT 0,
    acked     INTEGER NOT NULL DEFAULT 0
);
";

/// The v43 tables exactly as `schema.sql` creates them.
const SYNC_PEERS_SQL: &str = "
CREATE TABLE IF NOT EXISTS sync_peers (
    endpoint_id TEXT NOT NULL PRIMARY KEY,
    label       TEXT NOT NULL,
    addrs       TEXT NOT NULL,
    added_at    TEXT NOT NULL,
    last_seen   TEXT,
    announced   INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS sync_invites (
    secret_hash TEXT NOT NULL PRIMARY KEY,
    expires_at  TEXT NOT NULL,
    used_at     TEXT
);
";

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
