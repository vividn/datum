//! Migration bodies for schema versions 70 → 80 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 80,
//! start `v81_v90.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::{has_column, table_exists};

use crate::db::SETTING_FIELD;
use crate::error::{DatumError, Result};

/// da-w6pp2: `setting` is reserved for the database's settings, the entries
/// `setting:<name>`, as `kit` was for kits (v51). Nothing moves; a database
/// with life data logged under `setting` is refused rather than having that
/// data hidden from every surface and its field made unwritable.
pub(super) fn m70_to_71_setting_is_reserved(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    let clashing: i64 = conn.query_row(
        // A deleted log is not in the way, nor a setting itself (synced in
        // from a v71 peer, or kept by the down).
        "SELECT COUNT(*) FROM entries WHERE field = ?1 AND deleted_at IS NULL
           AND slug IS NOT ?1 || ':' || json_extract(data, '$.name')",
        [SETTING_FIELD],
        |row| row.get(0),
    )?;
    if clashing > 0 {
        return Err(DatumError::Other(format!(
            "cannot reserve `{SETTING_FIELD}` for the database's settings: this database \
             has {clashing} entr{} of a field named `{SETTING_FIELD}`. Rename that field \
             with an older datum first; nothing was changed.",
            if clashing == 1 { "y" } else { "ies" }
        )));
    }
    Ok(())
}

/// Reverse v70→v71. The `setting:<name>` entries stay: to a v70 binary they
/// are plain logs of a field named `setting`.
pub(super) fn down_71_to_70_setting_is_reserved(_conn: &Connection) -> Result<()> {
    Ok(())
}

/// da-cthk5: `sync_peers.last_synced`, when a session with the peer last
/// ended well. Nothing in the row says when that was, so it starts as
/// `last_seen`, which a handshake moves too: the dim may start late after a
/// session cut short, as before this column. Every row, a failed one too: a
/// member out of reach at the last dial has an error, and an empty time
/// turns the dim off. The next session that ends well writes it truly.
pub(super) fn m71_to_72_sync_peer_last_synced(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "sync_peers")? {
        return Ok(());
    }
    if !has_column(conn, "sync_peers", "last_synced")? {
        conn.execute_batch("ALTER TABLE sync_peers ADD COLUMN last_synced TEXT;")?;
    }
    conn.execute_batch("UPDATE sync_peers SET last_synced = last_seen WHERE last_synced IS NULL;")?;
    Ok(())
}

/// Reverse v71→v72. Local: the time goes, the peers stay.
pub(super) fn down_72_to_71_sync_peer_last_synced(conn: &Connection) -> Result<()> {
    if has_column(conn, "sync_peers", "last_synced")? {
        conn.execute_batch("ALTER TABLE sync_peers DROP COLUMN last_synced;")?;
    }
    Ok(())
}

/// da-yc5vi: a spec's `glyph` keys are its `symbol` keys. The stored specs
/// are renamed by [`crate::db::Db`] as it opens, as signed edits every copy
/// folds alike, which a migration has no key to sign: this marks the file
/// as holding specs from before (`_meta` `symbol_keys`).
pub(super) fn m72_to_73_symbol_keys(conn: &Connection) -> Result<()> {
    if table_exists(conn, "entries")? {
        conn.execute_batch(
            "INSERT OR REPLACE INTO _meta (key, value) VALUES ('symbol_keys', 'glyph');",
        )?;
    }
    Ok(())
}

/// Reverse v73→v72. A spec already renamed stays so: a build before it
/// keeps the new keys as unknown ones (da-klaf8) and draws no symbol.
pub(super) fn down_73_to_72_symbol_keys(conn: &Connection) -> Result<()> {
    conn.execute_batch("DELETE FROM _meta WHERE key = 'symbol_keys';")?;
    Ok(())
}

/// da-3zq6g: what `datum check` kept and the log of what changed since, and
/// the index `check keys` reads (`check_kept.sql`). They start empty; the
/// first `check` reads everything.
pub(super) fn m73_to_74_check_kept(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? || !table_exists(conn, "ops")? {
        return Ok(());
    }
    conn.execute_batch(crate::db::CHECK_KEPT_SQL)?;
    Ok(())
}

/// Reverse v73→v74. Local and derived: the next `check` reads everything.
pub(super) fn down_74_to_73_check_kept(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS check_changes_insert;
         DROP TRIGGER IF EXISTS check_changes_update;
         DROP TRIGGER IF EXISTS check_changes_delete;
         DROP TRIGGER IF EXISTS check_changes_op_insert;
         DROP TRIGGER IF EXISTS check_changes_op_update;
         DROP TRIGGER IF EXISTS check_changes_op_delete;
         DROP INDEX IF EXISTS entries_asserted_base;
         DROP TABLE IF EXISTS check_changes;
         DROP TABLE IF EXISTS check_kept;",
    )?;
    Ok(())
}

/// da-l6roh: `entries_field_deciding`, the rows a state read walks back
/// through, and `entries_field_blocks` without the moments, which took the
/// places of the read's lookback and lookahead though they decide nothing.
pub(super) fn m74_to_75_field_deciding_index(conn: &Connection) -> Result<()> {
    if table_exists(conn, "entries")? {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS entries_field_deciding
                 ON entries (field, occur_time, slug)
                 WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
                   AND verb <> 'point';
             DROP INDEX IF EXISTS entries_field_blocks;
             CREATE INDEX entries_field_blocks
                 ON entries (field, occur_time, slug)
                 WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
                   AND dur IS NOT NULL
                   AND verb <> 'point';",
        )?;
    }
    Ok(())
}

/// Reverse v74→v75.
pub(super) fn down_75_to_74_field_deciding_index(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_field_deciding;
         DROP INDEX IF EXISTS entries_field_blocks;
         CREATE INDEX IF NOT EXISTS entries_field_blocks
             ON entries (field, occur_time, slug)
             WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
               AND dur IS NOT NULL
               AND NOT (verb = 'point' AND (state IS NULL OR state = 'true'));",
    )?;
    Ok(())
}
