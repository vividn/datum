//! Migration bodies for schema versions 60 → 70 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 70,
//! start `v71_v80.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::table_exists;

use crate::error::Result;

/// da-2vgj1: the listing orders by `occur_time ?? create_time`, so a
/// timeless row sits at its create time; these keep `tail` / `head` an
/// index walk, bare and with a field named. Local: an index never syncs.
pub(super) fn m60_to_61_hybrid_indexes(conn: &Connection) -> Result<()> {
    if table_exists(conn, "entries")? {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS entries_hybrid
                 ON entries (COALESCE(occur_time, create_time), slug)
                 WHERE deleted_at IS NULL;
             CREATE INDEX IF NOT EXISTS entries_live_field_hybrid
                 ON entries (field, COALESCE(occur_time, create_time), slug)
                 WHERE deleted_at IS NULL;",
        )?;
    }
    Ok(())
}

/// Reverse v60→v61.
pub(super) fn down_61_to_60_hybrid_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_hybrid; DROP INDEX IF EXISTS entries_live_field_hybrid;",
    )?;
    Ok(())
}
