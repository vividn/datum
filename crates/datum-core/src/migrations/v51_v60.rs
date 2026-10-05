//! Migration bodies for schema versions 50 → 60 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 60,
//! start `v61_v70.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::table_exists;

use crate::db::KIT_FIELD;
use crate::error::{DatumError, Result};

/// da-xnzc7: `kit` is reserved for kits' `[kit]` tables, as `spec`, `alias`
/// and `view` / `command` / `index` were before it. Nothing moves; a database
/// with life data logged under `kit` is refused rather than having that data
/// hidden from every surface and its field made unwritable.
pub(super) fn m50_to_51_kit_is_reserved(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    let clashing: i64 = conn.query_row(
        // A deleted log is not in the way: nothing reads it as life data. Nor
        // is a kit's table itself — synced in from a v51 peer (sync carries
        // no schema version), or kept by the down.
        "SELECT COUNT(*) FROM entries WHERE field = ?1 AND deleted_at IS NULL
           AND NOT (slug = ?1 || ':' || json_extract(data, '$.name')
                    AND json_extract(data, '$.kit_id') IS NOT NULL)",
        [KIT_FIELD],
        |row| row.get(0),
    )?;
    if clashing > 0 {
        return Err(DatumError::Other(format!(
            "cannot reserve `{KIT_FIELD}` for kits: this database has {clashing} \
             entr{} of a field named `{KIT_FIELD}`. Rename that field with an older \
             datum first; nothing was changed.",
            if clashing == 1 { "y" } else { "ies" }
        )));
    }
    Ok(())
}

/// Reverse v50→v51. The `kit:<name>` entries stay: to a v50 binary they are
/// plain logs of a field named `kit`.
pub(super) fn down_51_to_50_kit_is_reserved(_conn: &Connection) -> Result<()> {
    Ok(())
}

/// da-z0zh3: the undo stack — `undo_actions`, and the action each
/// `entry_history` row was written by. Older rows have none, and undo stops
/// at them rather than guess how they grouped.
pub(super) fn m51_to_52_undo_actions(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE entry_history ADD COLUMN action_id INTEGER;
         CREATE INDEX IF NOT EXISTS entry_history_action
             ON entry_history(action_id) WHERE action_id IS NOT NULL;
         CREATE TABLE IF NOT EXISTS undo_actions (
             id        INTEGER PRIMARY KEY AUTOINCREMENT,
             time      TEXT NOT NULL,
             undone    INTEGER NOT NULL DEFAULT 0,
             untracked TEXT
         );",
    )?;
    Ok(())
}

/// Reverse v51→v52. Local: the stack is forgotten, the history stays.
pub(super) fn down_52_to_51_undo_actions(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entry_history_action;
         ALTER TABLE entry_history DROP COLUMN action_id;
         DROP TABLE IF EXISTS undo_actions;",
    )?;
    Ok(())
}
