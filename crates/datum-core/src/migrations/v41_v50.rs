//! Migration bodies for schema versions 40 → 50.
//!
//! Closed: v51 on lives in `v51_v60.rs`.

use rusqlite::Connection;

use super::support::{has_column, table_exists};

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

/// da-6177s: every sync is between members of a chain, whose invites are
/// chain records; the pairwise invites' table goes. Local, and nothing
/// read it in production.
pub(super) fn m46_to_47_drop_sync_invites(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS sync_invites;")?;
    Ok(())
}

/// Reverse v46→v47: the table back, empty.
pub(super) fn down_47_to_46_drop_sync_invites(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sync_invites (
            secret_hash TEXT NOT NULL PRIMARY KEY,
            expires_at  TEXT NOT NULL,
            used_at     TEXT
        );",
    )?;
    Ok(())
}

/// da-lfsei: `device_id` names an installation — a CLI data directory, a
/// browser profile — not a machine, so it becomes `client_id` on `entries`,
/// `entry_history` and `sync_local`, and in every history snapshot. Op
/// bodies keep the old key: they are signed, so the fold renames it as it
/// reads them (`sync.rs`).
pub(super) fn m47_to_48_client_id(conn: &Connection) -> Result<()> {
    rename_client_id(conn, "device_id", "client_id")
}

/// Reverse v47→v48.
pub(super) fn down_48_to_47_client_id(conn: &Connection) -> Result<()> {
    rename_client_id(conn, "client_id", "device_id")
}

fn rename_client_id(conn: &Connection, from: &str, to: &str) -> Result<()> {
    for table in ["entries", "entry_history", "sync_local"] {
        if has_column(conn, table, from)? {
            conn.execute_batch(&format!(
                "ALTER TABLE {table} RENAME COLUMN {from} TO {to};"
            ))?;
        }
    }
    if table_exists(conn, "entry_history")? {
        conn.execute(
            &format!(
                "UPDATE entry_history
                    SET snapshot = json_set(json_remove(snapshot, '$.{from}'),
                                            '$.{to}', json_extract(snapshot, '$.{from}'))
                  WHERE json_type(snapshot, '$.{from}') IS NOT NULL"
            ),
            [],
        )?;
    }
    Ok(())
}

/// da-wlbyx: `sync_local.paused_at` and `warned_at`.
pub(super) fn m48_to_49_sync_pause(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE sync_local ADD COLUMN paused_at TEXT;
         ALTER TABLE sync_local ADD COLUMN warned_at TEXT;",
    )?;
    Ok(())
}

/// Reverse v48→v49.
pub(super) fn down_49_to_48_sync_pause(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE sync_local DROP COLUMN warned_at;
         ALTER TABLE sync_local DROP COLUMN paused_at;",
    )?;
    Ok(())
}

/// da-u81u1: `sync_caught_up`, as `schema.sql` creates it.
pub(super) fn m49_to_50_sync_caught_up(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sync_caught_up (
             endpoint TEXT NOT NULL,
             fields   TEXT NOT NULL,
             seq      INTEGER NOT NULL,
             PRIMARY KEY (endpoint, fields)
         );",
    )?;
    Ok(())
}

/// Reverse v49→v50.
pub(super) fn down_50_to_49_sync_caught_up(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS sync_caught_up;")?;
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
