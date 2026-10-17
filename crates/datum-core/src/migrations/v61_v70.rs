//! Migration bodies for schema versions 60 → 70 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 70,
//! start `v71_v80.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::{has_column, table_exists};

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

/// da-qctsr: every command's first write runs `DELETE FROM undo_actions
/// WHERE undone = 1`, which scanned every action this copy ever made.
pub(super) fn m61_to_62_undo_undone_index(conn: &Connection) -> Result<()> {
    if table_exists(conn, "undo_actions")? {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS undo_actions_undone
                 ON undo_actions(id) WHERE undone = 1;",
        )?;
    }
    Ok(())
}

/// Reverse v61→v62.
pub(super) fn down_62_to_61_undo_undone_index(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS undo_actions_undone;")?;
    Ok(())
}

/// da-ulu8f: `spec_alias_names`, every derived alias name by the spec row
/// carrying it, so the first word of a command is looked up by name and
/// not by reading every spec. Local: derived from the `spec` rows.
pub(super) fn m62_to_63_spec_alias_names(conn: &Connection) -> Result<()> {
    if table_exists(conn, "entries")? {
        crate::db::fill_spec_alias_names(conn)?;
    }
    Ok(())
}

/// Reverse v62→v63.
pub(super) fn down_63_to_62_spec_alias_names(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS spec_alias_names_insert;
         DROP TRIGGER IF EXISTS spec_alias_names_update;
         DROP TRIGGER IF EXISTS spec_alias_names_delete;
         DROP TABLE IF EXISTS spec_alias_names;",
    )?;
    Ok(())
}

/// da-ueb4p: whether a session's failure came before it reached the peer,
/// so `sync status` says a member that is not up is `offline`, not `error`.
pub(super) fn m63_to_64_sync_peer_offline(conn: &Connection) -> Result<()> {
    if table_exists(conn, "sync_peers")? && !has_column(conn, "sync_peers", "last_error_offline")? {
        conn.execute_batch(
            "ALTER TABLE sync_peers ADD COLUMN last_error_offline INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    Ok(())
}

/// Reverse v63→v64. Local: an offline failure reads as an error again.
pub(super) fn down_64_to_63_sync_peer_offline(conn: &Connection) -> Result<()> {
    if has_column(conn, "sync_peers", "last_error_offline")? {
        conn.execute_batch("ALTER TABLE sync_peers DROP COLUMN last_error_offline;")?;
    }
    Ok(())
}

/// da-38obu: `entries_spec_templates`, the spec rows whose field is a
/// template, so a field one covers finds it without reading every spec.
pub(super) fn m64_to_65_spec_templates_index(conn: &Connection) -> Result<()> {
    if table_exists(conn, "entries")? {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS entries_spec_templates
                 ON entries (slug) WHERE field = 'spec' AND slug GLOB '*%*%*';",
        )?;
    }
    Ok(())
}

/// Reverse v64→v65.
pub(super) fn down_65_to_64_spec_templates_index(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_spec_templates;")?;
    Ok(())
}

/// da-e853a: `entries_field_blocks`, the rows a state read looks ahead
/// through for a block still running at its instant. Without it the
/// lookahead walked every later row of a field with no later block, so a
/// past dayview cost by its distance from now.
pub(super) fn m65_to_66_field_blocks_index(conn: &Connection) -> Result<()> {
    if table_exists(conn, "entries")? {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS entries_field_blocks
                 ON entries (field, occur_time, slug)
                 WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
                   AND dur IS NOT NULL
                   AND NOT (verb = 'point' AND (state IS NULL OR state = 'true'));",
        )?;
    }
    Ok(())
}

/// Reverse v65→v66.
pub(super) fn down_66_to_65_field_blocks_index(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_field_blocks;")?;
    Ok(())
}
