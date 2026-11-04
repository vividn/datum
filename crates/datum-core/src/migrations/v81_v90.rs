//! Migration bodies for schema versions 80 → 90 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 90,
//! start `v91_v100.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::{has_column, table_exists};

use crate::error::Result;

/// da-52vdj: the sync floor each peer said in its handshake, beside its
/// schema, so a copy says which of two across a floor must update. Local.
pub(super) fn m80_to_81_peer_floor(conn: &Connection) -> Result<()> {
    if table_exists(conn, "sync_peers")? && !has_column(conn, "sync_peers", "floor")? {
        conn.execute_batch("ALTER TABLE sync_peers ADD COLUMN floor INTEGER;")?;
    }
    Ok(())
}

/// Reverse v80→v81: the floors go, read again at each peer's next handshake.
pub(super) fn down_81_to_80_peer_floor(conn: &Connection) -> Result<()> {
    if table_exists(conn, "sync_peers")? && has_column(conn, "sync_peers", "floor")? {
        conn.execute_batch("ALTER TABLE sync_peers DROP COLUMN floor;")?;
    }
    Ok(())
}

/// da-7z5i9: `dayview --digests`' kept days and the triggers that say which
/// a write reached (`dayview_kept.sql`), laid empty. Local and derived.
pub(super) fn m81_to_82_dayview_kept(conn: &Connection) -> Result<()> {
    if table_exists(conn, "entries")? && table_exists(conn, "integrity_warnings")? {
        conn.execute_batch(crate::db::DAYVIEW_KEPT_SQL)?;
    }
    Ok(())
}

/// Reverse v81→v82: a v81 build draws every day it checks.
pub(super) fn down_82_to_81_dayview_kept(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS dayview_kept_insert;
         DROP TRIGGER IF EXISTS dayview_kept_update;
         DROP TRIGGER IF EXISTS dayview_kept_delete;
         DROP TRIGGER IF EXISTS dayview_kept_warning_insert;
         DROP TRIGGER IF EXISTS dayview_kept_warning_update;
         DROP TRIGGER IF EXISTS dayview_kept_warning_delete;
         DROP TABLE IF EXISTS dayview_kept;
         DROP TABLE IF EXISTS dayview_kept_dirty;",
    )?;
    Ok(())
}
