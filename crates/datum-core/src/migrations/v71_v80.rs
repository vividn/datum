//! Migration bodies for schema versions 70 → 80 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 80,
//! start `v81_v90.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::{has_column, table_exists, with_live_columns, RENAMED_COLUMNS};

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
    with_live_columns(conn, || Ok(conn.execute_batch(crate::db::CHECK_KEPT_SQL)?))
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

/// The index names that said the old name, with what each indexes. The column
/// rename carries an index's definition along, and not its name.
const AT_INDEXES: &[(&str, &str, &str)] = &[
    (
        "entries_field_occurtime",
        "entries_field_at_unique",
        "UNIQUE INDEX NAME
    ON entries (field, TIME)
    WHERE TIME IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
      AND state IS NOT NULL",
    ),
    (
        "entries_field_occur",
        "entries_field_at",
        "INDEX NAME
    ON entries (field, TIME DESC)",
    ),
    (
        "entries_live_field_occur",
        "entries_live_field_at",
        "INDEX NAME
    ON entries (field, TIME, slug)
    WHERE deleted_at IS NULL",
    ),
    (
        "entries_occur_time",
        "entries_at",
        "INDEX NAME
    ON entries (TIME, slug)
    WHERE deleted_at IS NULL",
    ),
];

/// Give each of `AT_INDEXES` the name a schema uses for it, on its `time` column.
fn rename_at_indexes(conn: &Connection, up: bool) -> Result<()> {
    for (old, new, shape) in AT_INDEXES {
        let (gone, name, time) = match up {
            true => (old, new, "at"),
            false => (new, old, "occur_time"),
        };
        let shape = shape.replace("NAME", name).replace("TIME", time);
        conn.execute_batch(&format!("DROP INDEX IF EXISTS {gone}; CREATE {shape};"))?;
    }
    Ok(())
}

/// Each `conflicts` that names `from` as a key names `to` instead. The column
/// is every copy's derivation from the ops, which now read as `at`, and `datum
/// check sync` compares the two.
fn rename_conflict_key(conn: &Connection, from: &str, to: &str) -> Result<()> {
    let rows: Vec<(String, String)> = conn
        .prepare("SELECT hid, conflicts FROM entries WHERE conflicts IS NOT NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (hid, conflicts) in rows {
        let mut v: serde_json::Value = serde_json::from_str(&conflicts)?;
        let Some(sides) = v.as_object_mut().and_then(|m| m.remove(from)) else {
            continue;
        };
        v[to] = sides;
        conn.execute(
            "UPDATE entries SET conflicts = ?2 WHERE hid = ?1",
            rusqlite::params![hid, crate::db::canonical_json(&v)],
        )?;
    }
    Ok(())
}

/// What was read through a row's old names, kept beside it: each is read again.
fn forget_row_hashes(conn: &Connection) -> Result<()> {
    for sql in [
        "DELETE FROM sync_row_hashes",
        "DELETE FROM sync_rows_changed",
        "DELETE FROM sync_state_sums",
        "DELETE FROM sync_state_hashes",
        "DELETE FROM check_kept",
    ] {
        if table_exists(conn, sql.rsplit(' ').next().unwrap_or_default())? {
            conn.execute(sql, [])?;
        }
    }
    conn.execute("DELETE FROM _meta WHERE key = 'sync_row_hashes'", [])?;
    Ok(())
}

/// da-95avu: the entry's time is `at`, and its family with it. Renaming a
/// column reads no row: `ALTER` rewrites the schema's text, the triggers and
/// indexes among it. Four indexes are built again for their names, and the old
/// names stay readable as virtual columns for SQL no migration can reach (see
/// `schema.sql`). Rows are unchanged but for the key in `conflicts`.
pub(super) fn m75_to_76_at(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? || !has_column(conn, "entries", "occur_time")? {
        return Ok(());
    }
    for (old, new) in &RENAMED_COLUMNS[1..] {
        conn.execute_batch(&format!(
            "ALTER TABLE entries RENAME COLUMN {old} TO {new};
             ALTER TABLE entries ADD COLUMN {old} GENERATED ALWAYS AS ({new}) VIRTUAL;"
        ))?;
    }
    rename_at_indexes(conn, true)?;
    rename_conflict_key(conn, "occur_time", "at")?;
    forget_row_hashes(conn)
}

/// Reverse v75→v76. No command runs this: the tests walk down to stage an old
/// file. An op signed since keeps saying `at`, which a v75 binary would read as
/// an entry with no time, so for a person the way back is the `.pre-v75` copy.
pub(super) fn down_76_to_75_at(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? || !has_column(conn, "entries", "at")? {
        return Ok(());
    }
    // A kit's index on an old name is on its virtual column, and derived.
    let on_old: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 'entries'
               AND sql LIKE '%occur_time%'",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for index in on_old {
        conn.execute_batch(&format!("DROP INDEX \"{index}\";"))?;
    }
    for (old, new) in &RENAMED_COLUMNS[1..] {
        conn.execute_batch(&format!(
            "ALTER TABLE entries DROP COLUMN {old};
             ALTER TABLE entries RENAME COLUMN {new} TO {old};"
        ))?;
    }
    rename_at_indexes(conn, false)?;
    rename_conflict_key(conn, "at", "occur_time")?;
    forget_row_hashes(conn)
}

/// da-il29m: `undo <field>` takes back part of an action, and every undo is a
/// numbered step for `redo` to take the newest of. An action already undone
/// gets its number so the oldest is still redone first. Local: nothing here syncs.
pub(super) fn m76_to_77_undo_taken(conn: &Connection) -> Result<()> {
    if table_exists(conn, "undo_actions")? {
        conn.execute_batch(
            "UPDATE undo_actions SET undone = (
                 SELECT COUNT(*) FROM undo_actions b
                  WHERE b.undone <> 0 AND b.id >= undo_actions.id)
              WHERE undone <> 0;
             DROP INDEX IF EXISTS undo_actions_undone;
             CREATE INDEX undo_actions_undone ON undo_actions(id) WHERE undone <> 0;
             CREATE TABLE IF NOT EXISTS undo_taken (
                 action_id INTEGER NOT NULL,
                 hid       TEXT NOT NULL,
                 step      INTEGER NOT NULL,
                 PRIMARY KEY (action_id, hid)
             ) WITHOUT ROWID;",
        )?;
    }
    Ok(())
}

/// Reverse v76→v77: an action partly taken back stands whole in the stack again.
pub(super) fn down_77_to_76_undo_taken(conn: &Connection) -> Result<()> {
    if table_exists(conn, "undo_actions")? {
        conn.execute_batch(
            "DROP TABLE IF EXISTS undo_taken;
             UPDATE undo_actions SET undone = 1 WHERE undone <> 0;
             DROP INDEX IF EXISTS undo_actions_undone;
             CREATE INDEX undo_actions_undone ON undo_actions(id) WHERE undone = 1;",
        )?;
    }
    Ok(())
}

/// da-k5vg1: `undo` takes no part of a command any more, so the partial undo's
/// table and step numbers go. An entry already taken back stays out of what the
/// action's undo moves, and its redo, as `skipped`: where the rest of its action
/// stands, and where a new command dropped that entry's redo. An undone action
/// with a part's redo still held is redone whole, that part with it. `took` is what a
/// `revert <field>` took back, for the next to go on past. Local.
pub(super) fn m77_to_78_no_undo_taken(conn: &Connection) -> Result<()> {
    if table_exists(conn, "undo_actions")? {
        conn.execute_batch(
            "UPDATE undo_actions SET skipped = (
                 SELECT json_group_array(hid) FROM (
                     SELECT value AS hid FROM json_each(COALESCE(undo_actions.skipped, '[]'))
                     UNION
                     SELECT t.hid FROM undo_taken t
                      WHERE t.action_id = undo_actions.id
                        AND (undo_actions.undone = 0 OR t.step = -1)))
              WHERE EXISTS (SELECT 1 FROM undo_taken t
                             WHERE t.action_id = undo_actions.id
                               AND (undo_actions.undone = 0 OR t.step = -1));
             UPDATE undo_actions SET undone = 1 WHERE undone <> 0;
             DROP TABLE IF EXISTS undo_taken;
             DROP INDEX IF EXISTS undo_actions_undone;
             CREATE INDEX undo_actions_undone ON undo_actions(id) WHERE undone = 1;",
        )?;
        if !has_column(conn, "undo_actions", "took")? {
            conn.execute_batch("ALTER TABLE undo_actions ADD COLUMN took TEXT;")?;
        }
    }
    Ok(())
}

/// Reverse v77→v78: the table back, empty, and each undone action its number.
pub(super) fn down_78_to_77_no_undo_taken(conn: &Connection) -> Result<()> {
    if table_exists(conn, "undo_actions")? {
        if has_column(conn, "undo_actions", "took")? {
            conn.execute_batch("ALTER TABLE undo_actions DROP COLUMN took;")?;
        }
        conn.execute_batch(
            "UPDATE entry_history SET change_kind = 'user_edit' WHERE change_kind = 'revert';",
        )?;
    }
    m76_to_77_undo_taken(conn)
}

/// da-8hfta: the entries no op covers, kept by triggers
/// (`sync_uncovered.sql`), so a sync finds its baselines without reading every
/// entry. Filled once here; local and derived.
pub(super) fn m78_to_79_sync_uncovered(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? || !table_exists(conn, "ops")? {
        return Ok(());
    }
    conn.execute_batch(crate::db::SYNC_UNCOVERED_SQL)?;
    conn.execute_batch(
        "INSERT OR IGNORE INTO sync_uncovered (hid)
             SELECT hid FROM entries
              WHERE NOT EXISTS (SELECT 1 FROM ops WHERE ops.hid = entries.hid);",
    )?;
    Ok(())
}

/// Reverse v78→v79. Local and derived: a v78 build reads every entry.
pub(super) fn down_79_to_78_sync_uncovered(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS sync_uncovered_insert;
         DROP TRIGGER IF EXISTS sync_uncovered_rehid;
         DROP TRIGGER IF EXISTS sync_uncovered_delete;
         DROP TRIGGER IF EXISTS sync_uncovered_op_insert;
         DROP TRIGGER IF EXISTS sync_uncovered_op_delete;
         DROP TRIGGER IF EXISTS sync_uncovered_op_rehid;
         DROP TABLE IF EXISTS sync_uncovered;",
    )?;
    Ok(())
}

/// da-zpc98: who made each change, `user@host`, on its `entry_history` row
/// and an entry's latest on its row, kept by `entry_author.sql`'s trigger;
/// the rows before stay NULL. Local.
pub(super) fn m79_to_80_author(conn: &Connection) -> Result<()> {
    for table in ["entry_history", "entries"] {
        if table_exists(conn, table)? && !has_column(conn, table, "author")? {
            // `entries.author` is untyped, as schema.sql has it: an entries
            // column's declared type weighs on its index choices.
            let typed = if table == "entries" { "" } else { " TEXT" };
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN author{typed};"))?;
        }
    }
    if table_exists(conn, "entry_history")? && table_exists(conn, "entries")? {
        conn.execute_batch(crate::db::ENTRY_AUTHOR_SQL)?;
    }
    Ok(())
}

/// Reverse v79→v80: the authors go.
pub(super) fn down_80_to_79_author(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TRIGGER IF EXISTS entry_history_author;")?;
    for table in ["entry_history", "entries"] {
        if table_exists(conn, table)? && has_column(conn, table, "author")? {
            conn.execute_batch(&format!("ALTER TABLE {table} DROP COLUMN author;"))?;
        }
    }
    Ok(())
}
