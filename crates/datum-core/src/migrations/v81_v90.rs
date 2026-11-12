//! Migration bodies for schema versions 80 → 90 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 90,
//! start `v91_v100.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::{has_column, table_exists};

use crate::db::ATTACHMENT_FIELD;
use crate::error::{DatumError, Result};

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

/// da-wgf4a: `attachment` is reserved for images kept by their hash, the
/// entries `attachment:<sha256>`, as `setting` was (v71). Nothing moves; a
/// database with life data logged under `attachment` is refused rather than
/// having that data hidden from every surface and its field made unwritable.
pub(super) fn m82_to_83_attachment_is_reserved(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    let clashing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entries WHERE field = ?1 AND deleted_at IS NULL
           AND slug IS NOT ?1 || ':' || json_extract(data, '$.name')",
        [ATTACHMENT_FIELD],
        |row| row.get(0),
    )?;
    if clashing > 0 {
        return Err(DatumError::Other(format!(
            "cannot reserve `{ATTACHMENT_FIELD}` for kept images: this database has \
             {clashing} entr{} of a field named `{ATTACHMENT_FIELD}`. Rename that field \
             with an older datum first; nothing was changed.",
            if clashing == 1 { "y" } else { "ies" }
        )));
    }
    Ok(())
}

/// Reverse v82→v83. The `attachment:<sha256>` entries stay: to a v82 binary
/// they are plain logs of a field named `attachment`.
pub(super) fn down_83_to_82_attachment_is_reserved(_conn: &Connection) -> Result<()> {
    Ok(())
}
