//! Migration bodies for schema versions 80 → 90 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 90,
//! start `v91_v100.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::table_exists;

use crate::error::Result;

/// da-7z5i9: `dayview --digests`' kept days and the triggers that say which
/// a write reached (`dayview_kept.sql`), laid empty. Local and derived.
pub(super) fn m80_to_81_dayview_kept(conn: &Connection) -> Result<()> {
    if table_exists(conn, "entries")? && table_exists(conn, "integrity_warnings")? {
        conn.execute_batch(crate::db::DAYVIEW_KEPT_SQL)?;
    }
    Ok(())
}

/// Reverse v80→v81: a v80 build draws every day it checks.
pub(super) fn down_81_to_80_dayview_kept(conn: &Connection) -> Result<()> {
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
