//! Migration bodies for schema versions 30 → 40 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 40,
//! start `v41_v50.rs` rather than growing this file: the era boundary is
//! what keeps any one file from becoming the next 3,500-line monolith.

use rusqlite::{params, Connection, OptionalExtension};

use super::support::*;
use crate::db::{
    definition_data, new_alias_entry, new_definition_entry, new_spec_entry, ALIAS_FIELD,
    COMMAND_FIELD, INDEX_FIELD, SPEC_FIELD, VIEW_FIELD,
};
use crate::error::{DatumError, Result};
use crate::kit::{CommandEntry, IndexEntry, ViewEntry};

/// Rewrite the stored `CREATE TABLE` SQL for `table_name`, swapping the
/// `raw_input` column definition. Matches the trailing comma so the
/// replacement is anchored: `raw_input <from>,` → `raw_input <to>,`.
///
/// Idempotent: no-op when the target form is already present. The column
/// padding is whatever the table's author wrote — `entries` aligns its
/// types at 7 spaces, `entry_history` at 9 — so the clause is matched
/// past a run of whitespace rather than against an enumerated list of
/// paddings. An earlier version enumerated two, silently missed
/// `entry_history`, and left that column NOT NULL on every migrated
/// database while fresh ones (laid down from schema.sql) had it
/// nullable; the whole-chain shape test in `tests/migration_chain.rs`
/// is what surfaced the divergence.
fn rewrite_raw_input_column(
    conn: &Connection,
    table_name: &str,
    from: &str,
    to: &str,
) -> Result<()> {
    // Some migration tests stand up a partial schema (just `field_specs`,
    // say) and expect every later forward migration to no-op rather than
    // hard-fail on a missing table. Mirror that contract.
    let current_sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='table' AND name=?1",
            [table_name],
            |row| row.get(0),
        )
        .optional()?;
    let Some(current_sql) = current_sql else {
        return Ok(());
    };
    let Some(new_sql) = swap_column_type(&current_sql, "raw_input", from, to) else {
        return Ok(());
    };
    // SQLite caches the parsed schema in the connection. Rewriting
    // `sqlite_schema.sql` alone doesn't bump the cache; subsequent
    // INSERTs on the same connection would still enforce the OLD
    // constraint. Bumping `schema_version` forces a reparse on the next
    // SQL statement, which is the documented escape hatch for the
    // writable_schema editing pattern.
    let schema_version: i64 = conn.query_row("PRAGMA schema_version", [], |row| row.get(0))?;
    conn.execute_batch("PRAGMA writable_schema = 1;")?;
    conn.execute(
        "UPDATE sqlite_schema SET sql = ?1 WHERE type='table' AND name=?2",
        rusqlite::params![new_sql, table_name],
    )?;
    conn.execute_batch(&format!("PRAGMA schema_version = {};", schema_version + 1))?;
    conn.execute_batch("PRAGMA writable_schema = 0;")?;
    Ok(())
}

/// Swap the declared type of `column` in a stored `CREATE TABLE` statement,
/// preserving whatever padding the original used.
///
/// Matches `<column><whitespace><from>,` and requires the trailing comma so
/// that `from = "TEXT"` cannot match the `TEXT` prefix of a longer clause
/// like `TEXT NOT NULL`. Returns `None` when the column is absent or is
/// already in some other shape — the caller treats that as a no-op, which
/// is what makes the migration idempotent.
fn swap_column_type(sql: &str, column: &str, from: &str, to: &str) -> Option<String> {
    let mut cursor = 0;
    while let Some(offset) = sql[cursor..].find(column) {
        let name_end = cursor + offset + column.len();
        cursor = name_end;
        let rest = &sql[name_end..];
        let gap = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        // No whitespace after the name means this is a longer identifier
        // or a mention inside a comment, not the column clause.
        if gap == 0 {
            continue;
        }
        let Some(after) = rest[gap..].strip_prefix(from) else {
            continue;
        };
        if !after.starts_with(',') {
            continue;
        }
        let mut out = String::with_capacity(sql.len() + to.len());
        out.push_str(&sql[..name_end + gap]);
        out.push_str(to);
        out.push_str(after);
        return Some(out);
    }
    None
}

/// Walk `entries` (tombstones included) and rewrite each row's
/// `content_hash` to whatever `hash_of` returns. Keyed on `hid`, the one
/// identity that survives an edit. Idempotent, and a no-op on a partial
/// schema that has no `entries` table — some migration tests stand up only
/// a fragment and expect later forward steps to skip rather than fail.
fn rehash_all_entries(
    conn: &Connection,
    hash_of: impl Fn(&crate::entry::Entry) -> String,
) -> Result<()> {
    let has_entries: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='entries'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_entries {
        return Ok(());
    }
    // The live row mapper reads *today's* column names, but this helper
    // runs mid-walk, against whatever shape the schema has at that step.
    // da-b8rs renamed the two uncertainty columns at v34, so a v32 DB
    // being upgraded still calls them `*_precision` here. Alias them
    // forward for the read; any future rename of a mapped column has to
    // extend this list or the rehash breaks on real upgrades.
    let mut select = String::from("SELECT *");
    for (old, new) in [
        ("occur_time_precision", "occur_time_uncertainty"),
        ("dur_precision", "dur_uncertainty"),
    ] {
        if !has_column(conn, "entries", new)? && has_column(conn, "entries", old)? {
            select.push_str(&format!(", {old} AS {new}"));
        }
    }
    select.push_str(" FROM entries");

    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare(&select)?;
        let mapped = stmt.query_map([], |row| {
            let entry = crate::db::row_to_entry_public(row)?;
            Ok((entry.hid.to_string(), hash_of(&entry)))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut update = conn.prepare("UPDATE entries SET content_hash = ?2 WHERE hid = ?1")?;
    for (hid, hash) in rows {
        update.execute(params![hid, hash])?;
    }
    Ok(())
}

/// da-xuiw: rename `sync_state`'s two muddy "push seq" columns for clarity.
/// `push_seq` (the `modified_at` packing watermark) -> `push_mark`, and
/// `my_push_seq` (the per-device AAD push counter) -> `device_seq`. Pure
/// rename, no data change; `pull_seq` already reads cleanly and is left as
/// is. Guarded per-column so a re-run — or a fresh DB already on the new
/// names — is a no-op.
pub(super) fn m30_to_31_sync_state_rename_seq_columns(conn: &Connection) -> Result<()> {
    if has_column(conn, "sync_state", "push_seq")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN push_seq TO push_mark;")?;
    }
    if has_column(conn, "sync_state", "my_push_seq")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN my_push_seq TO device_seq;")?;
    }
    Ok(())
}

/// polecat §5 #11: `entries.raw_input` was declared `TEXT NOT NULL
/// DEFAULT ''`, with the contract pretending raw_input would always be
/// non-empty even though some write paths (legacy datum CouchDB import)
/// supply an empty string because the source never captured an argv.
/// Drop the NOT NULL and the DEFAULT — the column is genuinely
/// nullable now, with the empty-string sentinel maintained only on the
/// read side (the row mapper normalises empty → NULL) for backwards
/// compatibility with already-stored rows.
///
/// Implementation: SQLite's ALTER TABLE can't toggle a column's
/// NULL-ability directly, but it CAN parse a hand-edited
/// `sqlite_schema.sql`. We flip `writable_schema = 1`, rewrite the
/// `entries` table's stored CREATE TABLE string with the constraint
/// removed, then flip it back off. SQLite rebuilds its in-memory
/// schema on the next open; existing row data is unaffected.
///
/// The trick is the recommended path in the SQLite docs ("Making Other
/// Kinds Of Table Schema Changes") for the specific case of dropping
/// NOT NULL — a full table rebuild would be 2-3× the I/O for the same
/// outcome on a column that already only stores valid (non-NULL) text.
pub(super) fn m31_to_32_raw_input_nullable(conn: &Connection) -> Result<()> {
    rewrite_raw_input_column(conn, "entries", "TEXT NOT NULL DEFAULT ''", "TEXT")?;
    rewrite_raw_input_column(conn, "entry_history", "TEXT NOT NULL", "TEXT")?;
    Ok(())
}

/// da-4vyw: `content_hash` now covers `following_state` and deleted-ness on
/// top of the original `field · occur_time · dur · state · data`. Every
/// stored row's hash therefore has to be rewritten, or the whole database
/// reads as drifted to `datum check` and dedup compares old hashes against
/// new ones.
///
/// SHA-256 is not available to SQLite here, so this walks the rows in Rust
/// rather than running one `UPDATE`. It deliberately materializes each row
/// through the live row mapper and asks [`Entry::expected_content_hash`]
/// for the value: that is the exact predicate `datum check` uses, so the
/// migration cannot leave behind a row the checker would then flag. The
/// cost of that choice is that this migration is coupled to the current
/// hash definition rather than frozen at v33 — which is harmless, because
/// the *next* hash change must ship its own full rewrite anyway.
///
/// Structurally a no-op (an in-place UPDATE, like m21→m22), so
/// [`infer_legacy_version`] has nothing to probe for.
pub(super) fn m32_to_33_content_hash_v33(conn: &Connection) -> Result<()> {
    rehash_all_entries(conn, |entry| entry.expected_content_hash())
}

/// da-b8rs: `precision` and `uncertainty` were two names for one thing,
/// split across a crate boundary, which is why nobody could tell whether
/// uncertainty reached storage. It did — under the other name. Rename both
/// columns so the store says what the code says.
///
/// Pure rename: no data is read, rewritten or dropped, and the values keep
/// their meaning (an ISO-8601 ± radius, NULL = unknown). Guarded per column
/// so a re-run — or a fresh DB already laid down from `schema.sql` — is a
/// no-op.
///
/// The risk here is not SQL, it is rollout: `Entry` crosses the sync wire as
/// serde JSON, no struct sets `deny_unknown_fields`, and so a peer that does
/// not know a field silently drops it instead of erroring. Upgrade every
/// device before any device writes the new shape; `ops::parse_payload` reads
/// the old names for the transition window.
pub(super) fn m33_to_34_precision_to_uncertainty(conn: &Connection) -> Result<()> {
    if has_column(conn, "entries", "occur_time_precision")? {
        conn.execute_batch(
            "ALTER TABLE entries RENAME COLUMN occur_time_precision TO occur_time_uncertainty;",
        )?;
    }
    if has_column(conn, "entries", "dur_precision")? {
        conn.execute_batch("ALTER TABLE entries RENAME COLUMN dur_precision TO dur_uncertainty;")?;
    }
    Ok(())
}

/// da-uxgv: replace slug-parking with a `superseded_by` pointer.
///
/// Three schema moves and one data repair:
///
/// * add `entries.superseded_by` — the hid of the row that won an identity
///   this one also wanted;
/// * make `entries_slug` partial on `superseded_by IS NULL`, and add the
///   same term to `entries_field_occurtime`, so the uniqueness belief
///   becomes "one *live* row per identity" — which is what the hid-keyed
///   sync merge can actually hold;
/// * add the reverse-lookup partial index the conflict marker reads.
///
/// The indexes are dropped and recreated rather than left alone: `Db::open`
/// replays `schema.sql` with `CREATE … IF NOT EXISTS`, which would leave an
/// existing `entries_slug` at its old total definition forever.
///
/// The data repair un-parks rows the old mechanism moved (see
/// [`unpark_rows`]). It is not cosmetic: a parked slug is drift to the
/// post-da-uxgv `datum check`, and `--fix` would rewrite it straight onto
/// the winner's slug and trip the UNIQUE index.
pub(super) fn m34_to_35_superseded_by(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    if !has_column(conn, "entries", "superseded_by")? {
        conn.execute_batch("ALTER TABLE entries ADD COLUMN superseded_by TEXT;")?;
    }
    // The indexes go first: un-parking hands a slug back to a row the
    // winner already holds, and lifts a tombstone off a row that shares
    // the winner's `(field, occur_time)`. Both are exactly what the old
    // total indexes forbid and the new partial ones permit.
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_slug;
         CREATE UNIQUE INDEX entries_slug
             ON entries (slug)
             WHERE superseded_by IS NULL;
         DROP INDEX IF EXISTS entries_field_occurtime;
         CREATE UNIQUE INDEX entries_field_occurtime
             ON entries (field, occur_time)
             WHERE occur_time IS NOT NULL
               AND deleted_at IS NULL
               AND superseded_by IS NULL;
         CREATE INDEX IF NOT EXISTS entries_superseded_by
             ON entries (superseded_by)
             WHERE superseded_by IS NOT NULL;",
    )?;
    unpark_rows(conn)?;
    Ok(())
}

/// Move every row the retired parking mechanism displaced onto the
/// `superseded_by` pointer.
///
/// A parked row is recognised the way `slug::is_parked` recognised it: its
/// slug is `<base>_<its own hid>`. It is only un-parked when some *other*
/// row actually holds `<base>` — that other row is the winner the park was
/// derived against, and without it there is no supersession to record (a
/// lone parked row is genuine drift, and `datum check` is the right place
/// to meet it).
///
/// Parking also tombstoned the loser when the pair contended for one live
/// `(field, occur_time)`, stamping `deleted_at` with the *winner's*
/// `modify_time`. That exact equality is the signature of the park, so it
/// is the only tombstone lifted here; a tombstone at any other instant is a
/// deletion the user asked for and stays. Lifting one moves the row's
/// `content_hash` (deleted-ness is hashed, da-4vyw), so the hash is
/// recomputed for every row this touches.
fn unpark_rows(conn: &Connection) -> Result<()> {
    let rows: Vec<crate::entry::Entry> = {
        let mut stmt = conn.prepare("SELECT * FROM entries")?;
        let mapped = stmt.query_map([], crate::db::row_to_entry_public)?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let holders: std::collections::HashMap<&str, &crate::entry::Entry> =
        rows.iter().map(|e| (e.slug.as_str(), e)).collect();

    let mut update = conn.prepare(
        "UPDATE entries SET slug = ?2, superseded_by = ?3, deleted_at = ?4, content_hash = ?5 \
         WHERE hid = ?1",
    )?;
    for row in &rows {
        let suffix = format!("_{}", row.hid);
        let Some(base) = row.slug.as_str().strip_suffix(&suffix) else {
            continue;
        };
        if base.is_empty() {
            continue;
        }
        let Some(winner) = holders.get(base).filter(|w| w.hid != row.hid) else {
            continue;
        };
        let mut unparked = row.clone();
        unparked.slug = crate::ids::Slug(base.to_string());
        unparked.superseded_by = Some(winner.hid.clone());
        if unparked.deleted_at == Some(winner.modify_time) {
            unparked.deleted_at = None;
        }
        unparked.content_hash = unparked.expected_content_hash();
        update.execute(params![
            unparked.hid,
            unparked.slug,
            unparked.superseded_by,
            unparked.deleted_at.map(|t| t.to_rfc3339()),
            unparked.content_hash,
        ])?;
    }
    Ok(())
}

/// The `entries_field_occurtime` definition as `schema.sql` lays it down on
/// a fresh database, so the migrated and the fresh index are one text.
/// `migrations/tests.rs::migrated_and_fresh_field_occurtime_index_agree`
/// holds the two together.
const FIELD_OCCURTIME_INDEX_V36: &str = "CREATE UNIQUE INDEX IF NOT EXISTS entries_field_occurtime
    ON entries (field, occur_time)
    WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
      AND state IS NOT NULL";

/// da-2fkr: narrow `entries_field_occurtime` to rows that assert a state.
///
/// da-ga8b built the index to stop "two state-changes for the same field at
/// the same exact instant", but wrote a predicate that never mentions state,
/// so it also refused two *stateless* points at one instant — two
/// transactions dated to one day, which are two events and not one undefined
/// fact. Adding `state IS NOT NULL` makes the predicate say what the rule
/// always meant. A stateless pair that shares a slug as well is still
/// refused by `entries_slug`; an id template (`--id`, `spec --id-template`)
/// is the user's declaration that the two are different events.
///
/// No pre-check: narrowing a UNIQUE predicate can only ever admit rows a
/// wider one already allowed, so it cannot fail on existing data. The index
/// is dropped and recreated because `Db::open` replays `schema.sql` with
/// `CREATE … IF NOT EXISTS`, which would leave an old definition standing
/// forever (the same reason da-uxgv gives above).
pub(super) fn m35_to_36_field_occurtime_state_only(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    conn.execute_batch(&format!(
        "DROP INDEX IF EXISTS entries_field_occurtime;\n{FIELD_OCCURTIME_INDEX_V36};"
    ))?;
    Ok(())
}

/// da-ij3x: a plain lookup index on `entries(slug)`.
///
/// `entries_slug` became partial on `superseded_by IS NULL` in v35, and a
/// partial index only serves queries that repeat its predicate — so every
/// other `slug = ?` (`slug = ? OR hid = ?`, the tombstone probe, UPDATEs
/// keyed on slug) planned as a full scan, once per row on the insert path.
pub(super) fn m36_to_37_slug_lookup_index(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    conn.execute_batch("CREATE INDEX IF NOT EXISTS entries_slug_lookup ON entries (slug);")?;
    Ok(())
}

/// Lift 37→38: field specs become ENTRIES (da-lc26c). Every `field_specs`
/// row is written as the entry `spec:<field>` whose data is the spec's JSON
/// plus `kit_id` for a kit's spec, then the table is dropped — the entries
/// table is the one source of truth, so specs back up, sync and have a
/// history like any doc.
///
/// Deterministic across devices: the same spec gives the same slug, the
/// same data (the JSON is re-read into sorted keys) and the same fixed
/// occur time, so two devices migrating one spec make the same doc. Rows go
/// in `field_specs` rowid order, which is the spec-alias index's
/// first-written-wins tie-break.
///
/// **Refuses on a name collision**: entries of a field `spec` that predate
/// this are life data, and would be read as specs. Nothing is written; the
/// error names the field and how many entries it has.
pub(super) fn m37_to_38_specs_are_entries(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "field_specs")? || !table_exists(conn, "entries")? {
        return Ok(());
    }
    let clashing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entries WHERE field = ?1",
        [SPEC_FIELD],
        |row| row.get(0),
    )?;
    if clashing > 0 {
        return Err(DatumError::Other(format!(
            "cannot move field specs into entries: this database has {clashing} \
             entr{} of a field named `{SPEC_FIELD}`, and `{SPEC_FIELD}` is now reserved \
             for field specs. Rename that field with an older datum first; nothing \
             was changed.",
            if clashing == 1 { "y" } else { "ies" }
        )));
    }
    let mut stmt =
        conn.prepare("SELECT field, spec_json, kit_id FROM field_specs ORDER BY rowid")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (field, spec_json, kit_id) in rows {
        let mut data: serde_json::Value = serde_json::from_str(&spec_json)?;
        if let (Some(kit), Some(obj)) = (kit_id, data.as_object_mut()) {
            obj.insert("kit_id".into(), kit.into());
        }
        let row = new_spec_entry(&field, data, "migration-v38");
        crate::db::try_insert(conn, &row.slug, &row.hid, &row)?;
    }
    conn.execute_batch("DROP TABLE field_specs;")?;
    Ok(())
}

/// Lift 38→39: user and kit aliases become ENTRIES (da-b3752), as specs
/// did in v38. Every `aliases` row is written as the entry `alias:<name>`
/// whose data is the alias's JSON plus `kit_id` for a kit's alias, then the
/// table is dropped.
///
/// Deterministic across devices the same way: the same alias gives the
/// same slug, data and fixed occur time. **Refuses on a name collision**
/// with life data already logged under `alias`, writing nothing.
pub(super) fn m38_to_39_aliases_are_entries(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "aliases")? || !table_exists(conn, "entries")? {
        return Ok(());
    }
    let clashing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entries WHERE field = ?1",
        [ALIAS_FIELD],
        |row| row.get(0),
    )?;
    if clashing > 0 {
        return Err(DatumError::Other(format!(
            "cannot move aliases into entries: this database has {clashing} \
             entr{} of a field named `{ALIAS_FIELD}`, and `{ALIAS_FIELD}` is now reserved \
             for aliases. Rename that field with an older datum first; nothing \
             was changed.",
            if clashing == 1 { "y" } else { "ies" }
        )));
    }
    let mut stmt = conn.prepare("SELECT name, alias_json, kit_id FROM aliases ORDER BY name")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (name, alias_json, kit_id) in rows {
        let mut alias: crate::field_spec::Alias = serde_json::from_str(&alias_json)?;
        // The row's key is the name the alias answered to.
        alias.name = name;
        let mut data = serde_json::to_value(alias)?;
        if let (Some(kit), Some(obj)) = (kit_id, data.as_object_mut()) {
            obj.insert("kit_id".into(), kit.into());
        }
        let row = new_alias_entry(data, "migration-v39");
        crate::db::try_insert(conn, &row.slug, &row.hid, &row)?;
    }
    conn.execute_batch("DROP TABLE aliases;")?;
    Ok(())
}

/// Lift 39→40: views, kit commands and kit index definitions become
/// ENTRIES (da-fwwtn), as specs and aliases did: every `views` row the
/// entry `view:<name>`, every `kit_commands` row `command:<name>`, every
/// `kit_indices` row `index:<kit>/<name>`, each's data the kit TOML's
/// block plus `kit_id`; then the three tables are dropped. An index keeps
/// its physical SQLite index, which `Db::derive_kit_indices` now answers
/// for.
///
/// Deterministic across devices the same way: the same definition gives the
/// same slug, data and fixed occur time. **Refuses on a name collision**
/// with life data already logged under `view`, `command` or `index`, and on
/// an index whose DDL is not one a kit install writes, writing nothing.
pub(super) fn m39_to_40_definitions_are_entries(conn: &Connection) -> Result<()> {
    let tables = ["views", "kit_commands", "kit_indices"];
    if !table_exists(conn, "entries")?
        || !tables
            .iter()
            .any(|t| table_exists(conn, t).unwrap_or(false))
    {
        return Ok(());
    }
    let clashing: i64 = conn.query_row(
        // A deleted log is not in the way: nothing reads it as life data.
        "SELECT COUNT(*) FROM entries WHERE field IN (?1, ?2, ?3) AND deleted_at IS NULL",
        [VIEW_FIELD, COMMAND_FIELD, INDEX_FIELD],
        |row| row.get(0),
    )?;
    if clashing > 0 {
        return Err(DatumError::Other(format!(
            "cannot move views, kit commands and kit indices into entries: this \
             database has {clashing} entr{} of a field named `{VIEW_FIELD}`, \
             `{COMMAND_FIELD}` or `{INDEX_FIELD}`, and those are now reserved. Rename \
             that field with an older datum first; nothing was changed.",
            if clashing == 1 { "y" } else { "ies" }
        )));
    }
    let mut defs = Vec::new();
    if table_exists(conn, "views")? {
        let mut stmt = conn.prepare("SELECT name, sql_text, kit_id FROM views ORDER BY name")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get::<_, Option<String>>(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (name, sql, kit) in rows {
            defs.push((
                VIEW_FIELD,
                definition_data(&ViewEntry { name, sql }, kit.as_deref())?,
            ));
        }
    }
    if table_exists(conn, "kit_commands")? {
        fn json<T: serde::de::DeserializeOwned + Default>(text: Option<String>) -> T {
            text.and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default()
        }
        let mut stmt = conn.prepare(
            "SELECT name, kit_id, expands_to, view_name, help, columns_json, labels_json,
                    formats_json, sort_col, flags_json, color_rules_json, script_text
             FROM kit_commands ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let command = CommandEntry {
                    name: row.get(0)?,
                    expands_to: row.get(2)?,
                    view: row.get(3)?,
                    help: row.get(4)?,
                    columns: json(row.get(5)?),
                    labels: json(row.get(6)?),
                    formats: json(row.get(7)?),
                    sort: row.get(8)?,
                    flags: json(row.get(9)?),
                    color_rules: json(row.get(10)?),
                    script: row.get(11)?,
                };
                Ok((command, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (command, kit) in rows {
            defs.push((COMMAND_FIELD, definition_data(&command, kit.as_deref())?));
        }
    }
    if table_exists(conn, "kit_indices")? {
        let mut stmt = conn.prepare(
            "SELECT kit_id, index_name, sql_text FROM kit_indices ORDER BY kit_id, index_name",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (kit, name, sql_text) in rows {
            let Some(entry) = index_entry_from_ddl(&name, &kit, &sql_text) else {
                return Err(DatumError::Other(format!(
                    "cannot move kit '{kit}' index '{name}' into entries: its DDL is not \
                     one a kit install writes ({sql_text}). Uninstall that kit with an \
                     older datum first; nothing was changed."
                )));
            };
            defs.push((INDEX_FIELD, definition_data(&entry, Some(&kit))?));
        }
    }
    for (field, data) in defs {
        let row = new_definition_entry(field, data, "migration-v40");
        // A deleted log is out of the way, unless it holds this very slug —
        // as the slug's UNIQUE sees it, which a superseded row is not in.
        let held: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM entries WHERE slug = ?1 AND superseded_by IS NULL)",
            [row.slug.as_str()],
            |r| r.get(0),
        )?;
        if held {
            return Err(DatumError::Other(format!(
                "cannot move views, kit commands and kit indices into entries: an \
                 entry already holds `{}`, which the {field} needs. Move it to another \
                 slug with an older datum first; nothing was changed.",
                row.slug.as_str()
            )));
        }
        crate::db::try_insert(conn, &row.slug, &row.hid, &row)?;
    }
    conn.execute_batch(
        "DROP TABLE IF EXISTS views;
         DROP TABLE IF EXISTS kit_commands;
         DROP TABLE IF EXISTS kit_indices;",
    )?;
    Ok(())
}

/// The [`IndexEntry`] whose DDL under `kit_id` is `sql_text` — the inverse
/// of `IndexEntry::build_ddl`, which wrote every `kit_indices` row — or
/// `None` when rebuilding it would not give those very bytes back.
pub(super) fn index_entry_from_ddl(name: &str, kit_id: &str, sql_text: &str) -> Option<IndexEntry> {
    let (unique, rest) = match sql_text.strip_prefix("CREATE UNIQUE INDEX IF NOT EXISTS ") {
        Some(rest) => (true, rest),
        None => (false, sql_text.strip_prefix("CREATE INDEX IF NOT EXISTS ")?),
    };
    // The quoted name is `[a-zA-Z0-9_]`, so the first ` ON ` ends it.
    let (on, rest) = rest.split_once(" ON ")?.1.split_once('(')?;
    // The expression ends at the paren that closes this one; a quoted
    // literal or identifier — '…', "…", […] or `…` — may hold any
    // character.
    let (mut depth, mut quote, mut end) = (1, None, None);
    for (i, c) in rest.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"' | '`') => quote = Some(c),
            (None, '[') => quote = Some(']'),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let where_clause = match &rest[end + 1..] {
        "" => None,
        tail => Some(tail.strip_prefix(" WHERE ")?.to_string()),
    };
    let entry = IndexEntry {
        name: name.to_string(),
        on: on.to_string(),
        expression: rest[..end].to_string(),
        where_clause,
        unique,
    };
    (entry.build_ddl(kit_id).ok()? == sql_text).then_some(entry)
}

// ─── Down migrations ─────────────────────────────────────────────────
//
// Each forward migration above has a paired `down` here that reverses
// the schema change. Conventions:
//
// * Every down is wrapped in the same outer transaction the runner
//   opens, so a partial failure rolls the whole step back.
// * Pure schema changes (CREATE INDEX, CREATE TABLE) reverse trivially.
// * Column additions are reversed via `ALTER TABLE ... DROP COLUMN`
//   (SQLite ≥ 3.35) on tables that have no generated columns. For
//   `entries` we still use DROP COLUMN because the dropped columns
//   (`extra_metadata`, `user_asserted_last_state`) are not referenced
//   by the generated `verb` / `dur_seconds` projections, so the drop
//   is safe.
// * Forward steps that destructively rewrote a table or wrote new rows
//   (v2→v3, v8→v9, v11→v12) document the data-loss in the down's
//   docstring. The down still runs to make the schema reversible for
//   round-trip testing; production callers should back up before
//   walking back across these.

/// Reverse 40→39 — put the live view, command and index definitions back
/// in their tables and delete the entries with their history. Loses each
/// one's history and any deleted one, which the tables never held.
pub(super) fn down_40_to_39_definitions_are_entries(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS views (
            name      TEXT NOT NULL PRIMARY KEY,
            sql_text  TEXT NOT NULL,
            kit_id    TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_views_kit_id ON views(kit_id);
        CREATE TABLE IF NOT EXISTS kit_indices (
            kit_id      TEXT NOT NULL,
            index_name  TEXT NOT NULL,
            on_disk_name TEXT NOT NULL,
            sql_text    TEXT NOT NULL,
            PRIMARY KEY (kit_id, index_name)
        );
        CREATE TABLE IF NOT EXISTS kit_commands (
            name              TEXT NOT NULL PRIMARY KEY,
            kit_id            TEXT,
            expands_to        TEXT,
            view_name         TEXT,
            help              TEXT,
            columns_json      TEXT,
            labels_json       TEXT,
            formats_json      TEXT,
            sort_col          TEXT,
            flags_json        TEXT,
            color_rules_json  TEXT,
            script_text       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_kit_commands_kit_id ON kit_commands(kit_id);
        INSERT INTO views (name, sql_text, kit_id)
            SELECT json_extract(data, '$.name'), json_extract(data, '$.sql'),
                   json_extract(data, '$.kit_id')
              FROM entries
             WHERE field = 'view' AND deleted_at IS NULL AND superseded_by IS NULL;
        INSERT INTO kit_commands
            SELECT json_extract(data, '$.name'), json_extract(data, '$.kit_id'),
                   json_extract(data, '$.expands_to'), json_extract(data, '$.view'),
                   json_extract(data, '$.help'), json_extract(data, '$.columns'),
                   json_extract(data, '$.labels'), json_extract(data, '$.formats'),
                   json_extract(data, '$.sort'), json_extract(data, '$.flags'),
                   json_extract(data, '$.color_rules'), json_extract(data, '$.script')
              FROM entries
             WHERE field = 'command' AND deleted_at IS NULL AND superseded_by IS NULL;",
    )?;
    let mut stmt = conn.prepare(
        "SELECT data FROM entries
         WHERE field = 'index' AND deleted_at IS NULL AND superseded_by IS NULL",
    )?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for data in rows {
        let data: serde_json::Value = serde_json::from_str(&data)?;
        let kit = data["kit_id"].as_str().unwrap_or_default().to_string();
        let entry: IndexEntry = serde_json::from_value(data)?;
        conn.execute(
            "INSERT INTO kit_indices (kit_id, index_name, on_disk_name, sql_text)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                kit,
                entry.name,
                entry.sqlite_name(&kit),
                entry.build_ddl(&kit)?
            ],
        )?;
    }
    conn.execute_batch(
        "DELETE FROM entry_history WHERE entry_hid IN
            (SELECT hid FROM entries WHERE field IN ('view', 'command', 'index'));
         DELETE FROM entries WHERE field IN ('view', 'command', 'index');",
    )?;
    Ok(())
}

/// Reverse 39→38 — put the live alias entries back in an `aliases` table
/// and delete the entries with their history. Loses each alias's history
/// and any deleted alias, which the table never held.
pub(super) fn down_39_to_38_aliases_are_entries(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS aliases (
            name        TEXT NOT NULL PRIMARY KEY,
            alias_json  TEXT NOT NULL,
            kit_id      TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_aliases_kit_id ON aliases (kit_id);
        INSERT INTO aliases (name, alias_json, kit_id)
            SELECT json_extract(data, '$.name'),
                   json_remove(data, '$.kit_id'),
                   json_extract(data, '$.kit_id')
              FROM entries
             WHERE field = 'alias' AND deleted_at IS NULL AND superseded_by IS NULL
             ORDER BY rowid;
        DELETE FROM entry_history
         WHERE entry_hid IN (SELECT hid FROM entries WHERE field = 'alias');
        DELETE FROM entries WHERE field = 'alias';",
    )?;
    Ok(())
}

/// Reverse 38→37 — put the live spec entries back in a `field_specs`
/// table and delete the entries with their history. Loses each spec's
/// history and any deleted spec, which the table never held.
pub(super) fn down_38_to_37_specs_are_entries(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS field_specs (
            field       TEXT NOT NULL PRIMARY KEY,
            spec_json   TEXT NOT NULL,
            kit_id      TEXT,
            tags        TEXT NOT NULL DEFAULT '[]',
            render_json TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_field_specs_kit_id ON field_specs(kit_id);
        INSERT INTO field_specs (field, spec_json, kit_id, tags, render_json)
            SELECT json_extract(data, '$.field'),
                   json_remove(data, '$.kit_id'),
                   json_extract(data, '$.kit_id'),
                   COALESCE(data -> '$.tags', '[]'),
                   data -> '$.render'
              FROM entries
             WHERE field = 'spec' AND deleted_at IS NULL AND superseded_by IS NULL
             ORDER BY rowid;
        DELETE FROM entry_history
         WHERE entry_hid IN (SELECT hid FROM entries WHERE field = 'spec');
        DELETE FROM entries WHERE field = 'spec';",
    )?;
    Ok(())
}

/// Reverse 37→36 — drop the slug lookup index. Lossless.
pub(super) fn down_37_to_36_slug_lookup_index(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_slug_lookup;")?;
    Ok(())
}

/// Reverse 36→35 — restore the state-blind `(field, occur_time)` predicate.
///
/// **Pre-check**, the da-ga8b pattern: the wider predicate forbids rows the
/// narrow one admits, so a database holding two live stateless entries for
/// one field at one instant cannot walk back. Find them first and fail with
/// the offending hids named, rather than letting `CREATE INDEX` report a
/// bare `UNIQUE constraint failed` about rows the operator cannot then
/// identify. The scan mirrors the v35 predicate exactly (live, non-superseded,
/// real occur_time) and only groups that are entirely stateless can be new —
/// a state-bearing collision was already impossible at v36.
pub(super) fn down_36_to_35_field_occurtime_state_only(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    let collisions: Vec<(String, String, String)> = conn
        .prepare(
            "SELECT field, occur_time, GROUP_CONCAT(hid, ',') \
             FROM entries \
             WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL \
             GROUP BY field, occur_time \
             HAVING COUNT(*) > 1 \
             ORDER BY field, occur_time",
        )?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    if !collisions.is_empty() {
        let mut diag = String::from(
            "v36→v35 migration aborted: the pre-da-2fkr index forbids two live entries \
             for one field at one instant, stateless or not. Resolve the offending rows \
             (delete or retime one of each group) before re-running:",
        );
        for (field, occur_time, hids) in &collisions {
            diag.push_str(&format!(
                "\n  field={field} occur_time={occur_time} hids=[{hids}]"
            ));
        }
        return Err(crate::error::DatumError::Other(diag));
    }
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_field_occurtime;
         CREATE UNIQUE INDEX entries_field_occurtime
             ON entries (field, occur_time)
             WHERE occur_time IS NOT NULL
               AND deleted_at IS NULL
               AND superseded_by IS NULL;",
    )?;
    Ok(())
}

/// Reverse 35→34 — restore the total `entries_slug` UNIQUE index and drop
/// `superseded_by`.
///
/// **Lossy by construction.** The pointer is the only record that a pair
/// was in contention; the parked slugs it replaced are not reconstructed,
/// so a database that still holds a superseded row cannot walk back — the
/// two rows would both claim one slug and the restored total UNIQUE index
/// would refuse to build. That is the honest failure: the down is for
/// round-trip testing and for databases with no live conflict, and a
/// database with one should resolve it before walking back.
pub(super) fn down_35_to_34_superseded_by(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_superseded_by;
         DROP INDEX IF EXISTS entries_slug;
         CREATE UNIQUE INDEX entries_slug ON entries (slug);
         DROP INDEX IF EXISTS entries_field_occurtime;
         CREATE UNIQUE INDEX entries_field_occurtime
             ON entries (field, occur_time)
             WHERE occur_time IS NOT NULL AND deleted_at IS NULL;",
    )?;
    if has_column(conn, "entries", "superseded_by")? {
        conn.execute_batch("ALTER TABLE entries DROP COLUMN superseded_by;")?;
    }
    Ok(())
}

/// Reverse 34→33 — restore the pre-da-b8rs column names. Lossless.
pub(super) fn down_34_to_33_precision_to_uncertainty(conn: &Connection) -> Result<()> {
    if has_column(conn, "entries", "occur_time_uncertainty")? {
        conn.execute_batch(
            "ALTER TABLE entries RENAME COLUMN occur_time_uncertainty TO occur_time_precision;",
        )?;
    }
    if has_column(conn, "entries", "dur_uncertainty")? {
        conn.execute_batch("ALTER TABLE entries RENAME COLUMN dur_uncertainty TO dur_precision;")?;
    }
    Ok(())
}

/// Reverse 33→32 — recompute every hash under the pre-da-4vyw definition
/// (`field · occur_time · dur · state · data`, `\x00`-separated), which is
/// reproduced locally here because the live one has moved on.
pub(super) fn down_33_to_32_content_hash_v33(conn: &Connection) -> Result<()> {
    rehash_all_entries(conn, |entry| {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for (i, part) in [
            entry.field.clone(),
            entry
                .occur_time
                .as_ref()
                .map(|t| t.utc.to_rfc3339())
                .unwrap_or_default(),
            entry
                .dur
                .as_ref()
                .map(crate::dur::Dur::to_storage_string)
                .unwrap_or_default(),
            entry
                .state
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_default(),
            entry
                .data
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ]
        .iter()
        .enumerate()
        {
            if i > 0 {
                h.update(b"\x00");
            }
            h.update(part.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
}

/// Reverse 32→31 — restore the `NOT NULL DEFAULT ''` constraint via the
/// same writable_schema mechanism. Existing NULL rows would fail the
/// reapplied constraint, so we UPDATE them to the empty-string
/// sentinel first; the data-loss is minimal (NULL ↔ '' is the
/// transparent mapping the row mapper preserves anyway).
pub(super) fn down_32_to_31_raw_input_nullable(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE entries SET raw_input = '' WHERE raw_input IS NULL",
        [],
    )?;
    conn.execute(
        "UPDATE entry_history SET raw_input = '' WHERE raw_input IS NULL",
        [],
    )?;
    rewrite_raw_input_column(conn, "entries", "TEXT", "TEXT NOT NULL DEFAULT ''")?;
    rewrite_raw_input_column(conn, "entry_history", "TEXT", "TEXT NOT NULL")?;
    Ok(())
}

/// Reverse 31→30 — restore the pre-da-xuiw column names.
pub(super) fn down_31_to_30_sync_state_rename_seq_columns(conn: &Connection) -> Result<()> {
    if has_column(conn, "sync_state", "push_mark")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN push_mark TO push_seq;")?;
    }
    if has_column(conn, "sync_state", "device_seq")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN device_seq TO my_push_seq;")?;
    }
    Ok(())
}
