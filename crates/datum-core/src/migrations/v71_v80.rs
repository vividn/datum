//! Migration bodies for schema versions 70 → 80 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 80,
//! start `v81_v90.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::table_exists;

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
