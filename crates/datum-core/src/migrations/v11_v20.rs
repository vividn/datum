//! Migration bodies for schema versions 10 → 20 — the kit era.
//!
//! Where field specs grew a kit, a tag list and a render blob, aliases and
//! views became first-class tables, and the v19 → v20 pivot moved entries
//! from `occur/start/stop` triples onto the `(start_time, end_time)` time
//! shape the current model reads.

use rusqlite::{params, Connection};

use super::support::*;
use crate::error::Result;

fn recompute_content_hash(field: &str, start: &str, end: &str, value: &str, extra: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(field.as_bytes());
    h.update(b"\x00");
    h.update(start.as_bytes());
    h.update(b"\x00");
    h.update(end.as_bytes());
    h.update(b"\x00");
    h.update(value.as_bytes());
    h.update(b"\x00");
    h.update(extra.as_bytes());
    format!("{:x}", h.finalize())
}

pub(super) fn m10_to_11_last_docs(conn: &Connection) -> Result<()> {
    // da-e65: backs the `_LAST` / `_LAST_WITH_PROTECTION` quickId tokens.
    // Single row keyed on a constant id; hids stored as a JSON array.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS last_docs (
             id    INTEGER NOT NULL PRIMARY KEY,
             hids  TEXT NOT NULL,
             time  TEXT NOT NULL
         );",
    )?;
    Ok(())
}

pub(super) fn m11_to_12_extra_metadata(conn: &Connection) -> Result<()> {
    // da-tsw: separate system-managed provenance from user-supplied `data`.
    //
    // Step 1 — additive column. Idempotent via pragma_table_info introspection
    // so a half-applied migration can be retried.
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('entries') WHERE name = 'extra_metadata'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        conn.execute_batch("ALTER TABLE entries ADD COLUMN extra_metadata TEXT;")?;
    }

    // Step 2 — relocate `_import` markers written by previous datum-migrate
    // runs from `data` to `extra_metadata`. Recompute `content_hash` to match
    // the post-relocation `data` so re-imports of the same source dedup
    // against the migrated rows. Sets `data` to NULL when the strip leaves
    // an empty object so the column matches the freshly-imported shape.
    let mut stmt = conn.prepare(
        "SELECT hid, field, start_time, end_time, value, data
         FROM entries
         WHERE data IS NOT NULL AND data LIKE '%\"_import\"%'",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(crate::error::DatumError::Db)?;
    drop(stmt);

    for (hid, field, start_str, end_str, value_str, data_str) in rows {
        let mut data_val: serde_json::Value = serde_json::from_str(&data_str).map_err(|e| {
            crate::error::DatumError::Other(format!("bad data JSON for hid {hid}: {e}"))
        })?;
        let import_val = match data_val.as_object_mut() {
            Some(map) => map.remove("_import"),
            None => None,
        };
        if import_val.is_none() {
            // LIKE pattern matched a non-_import literal — skip without
            // touching the row.
            continue;
        }
        let metadata_val = serde_json::Value::Object({
            let mut m = serde_json::Map::new();
            m.insert("_import".to_string(), import_val.unwrap());
            m
        });

        // Empty post-strip → store NULL so the column matches a fresh import.
        let new_data: Option<String> = match &data_val {
            serde_json::Value::Object(map) if map.is_empty() => None,
            _ => Some(data_val.to_string()),
        };

        // Recompute content_hash from the post-relocation columns. Mirrors
        // `Entry::expected_content_hash` so re-imports collide.
        let new_hash = recompute_content_hash(
            &field,
            start_str.as_deref().unwrap_or(""),
            end_str.as_deref().unwrap_or(""),
            value_str.as_deref().unwrap_or(""),
            new_data.as_deref().unwrap_or(""),
        );

        conn.execute(
            "UPDATE entries
             SET data = ?2, extra_metadata = ?3, content_hash = ?4
             WHERE hid = ?1",
            params![hid, new_data, metadata_val.to_string(), new_hash],
        )?;
    }

    Ok(())
}

/// da-d4t.1: kits scope `field_specs` rows. `kit_id` is NULL for
/// user-declared specs (set via `datum spec set` or auto-tracked) and
/// non-NULL for rows installed by `datum kit install`. Uninstall deletes
/// only rows with the matching `kit_id`. Additive + idempotent.
pub(super) fn m12_to_13_field_specs_kit_id(conn: &Connection) -> Result<()> {
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('field_specs') WHERE name = 'kit_id'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        conn.execute_batch("ALTER TABLE field_specs ADD COLUMN kit_id TEXT;")?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_field_specs_kit_id ON field_specs(kit_id);",
    )?;
    Ok(())
}

/// da-d4t.4: kit-installable named SELECT views. Rows are tagged with
/// `kit_id` (NULL for user-declared rows, the kit's name for installed
/// rows); uninstall drops only rows with the matching `kit_id`. Additive +
/// idempotent — `CREATE TABLE IF NOT EXISTS` lets a half-applied run retry.
pub(super) fn m13_to_14_views_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS views (
            name      TEXT NOT NULL PRIMARY KEY,
            sql_text  TEXT NOT NULL,
            kit_id    TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_views_kit_id ON views(kit_id);",
    )?;
    Ok(())
}

/// da-d4t.5: kits can declare `[[index]]` entries; each is materialized
/// as a real SQL index against `entries` and recorded in `kit_indices`
/// so uninstall can `DROP INDEX` symmetrically. Additive + idempotent.
pub(super) fn m14_to_15_kit_indices(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kit_indices (
             kit_id       TEXT NOT NULL,
             index_name   TEXT NOT NULL,
             on_disk_name TEXT NOT NULL,
             sql_text     TEXT NOT NULL,
             PRIMARY KEY (kit_id, index_name)
         );",
    )?;
    Ok(())
}

/// da-d4t.3: kits scope `aliases` rows. `kit_id` is NULL for user-declared
/// aliases (set via `datum alias set` or seeded into the schema) and the
/// kit's name for rows installed by `datum kit install` from a `[[alias]]`
/// block. Uninstall deletes only rows with the matching `kit_id`. Additive +
/// idempotent.
pub(super) fn m15_to_16_aliases_kit_id(conn: &Connection) -> Result<()> {
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('aliases') WHERE name = 'kit_id'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        conn.execute_batch("ALTER TABLE aliases ADD COLUMN kit_id TEXT;")?;
    }
    conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_aliases_kit_id ON aliases(kit_id);")?;
    Ok(())
}

/// da-d4t.6: kit-installable Level 1 (alias-style) and Level 2
/// (view-rendered) commands. Each row is tagged with `kit_id` (NULL
/// reserved for future user-declared parity); uninstall drops only the
/// rows it owns. Additive + idempotent.
pub(super) fn m16_to_17_kit_commands_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kit_commands (
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
         CREATE INDEX IF NOT EXISTS idx_kit_commands_kit_id ON kit_commands(kit_id);",
    )?;
    Ok(())
}

/// da-a7g: tag-based field grouping. `tags` mirrors `FieldSpec.tags` out of
/// `spec_json` into a column so SQL filters (`tags LIKE '%"chore"%'`) and
/// the `datum spec list --tag` query can run without JSON parsing. Additive
/// + idempotent.
pub(super) fn m17_to_18_field_specs_tags(conn: &Connection) -> Result<()> {
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('field_specs') WHERE name = 'tags'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        conn.execute_batch("ALTER TABLE field_specs ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';")?;
    }
    Ok(())
}

/// da-2zd: per-surface render visibility. `render_json` mirrors
/// `FieldSpec.render` out of `spec_json` so visual surfaces (dayview,
/// nowview, future renderers) can filter without a JSON parse. NULL means
/// the implicit default — visible everywhere. Additive + idempotent.
pub(super) fn m18_to_19_field_specs_render_json(conn: &Connection) -> Result<()> {
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('field_specs') WHERE name = 'render_json'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        conn.execute_batch("ALTER TABLE field_specs ADD COLUMN render_json TEXT;")?;
    }
    Ok(())
}

/// da-5ns.1: time-shape pivot. Reverts daydumb's time model from
/// start_time/end_time/entry_shape/intent shape back to occur_time +
/// state + dur (ISO duration string) + following_state, plus a stored
/// `verb` column derived from the (state, dur, following_state) triple.
///
/// **Pre-v1 hard-cut.** No production data exists; this rebuilds the
/// entries table from scratch with the new shape rather than attempting
/// any data migration. Rows present at the time of the up are discarded.
/// Sub-beads 2/3 land verb-derivation logic and the insert/update path
/// that fills the new columns.
///
/// Schema delta (entries table):
/// - DROP: start_time, start_time_offset, start_time_tz,
///   start_time_precision, start_time_precision_unit,
///   end_time, end_time_offset, end_time_tz,
///   end_time_precision, end_time_precision_unit,
///   entry_shape, intent, value, dur_seconds (generated),
///   and the existing GENERATED `verb` column.
/// - ADD: occur_time TEXT NOT NULL, occur_time_offset INTEGER,
///   occur_time_tz TEXT, occur_time_precision TEXT
///   (single ISO-8601 duration string),
///   state TEXT (any JSON value; null == undefined),
///   dur TEXT (ISO-8601 duration string; NULL = no duration,
///   "PT0S" = zero),
///   following_state TEXT,
///   verb TEXT NOT NULL DEFAULT 'point' (stored, indexable).
/// - CHECK: following_state IS NULL OR dur IS NOT NULL.
/// - Indexes that referenced start_time are reseated on occur_time.
pub(super) fn m19_to_20_time_shape_pivot(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_slug;
         DROP INDEX IF EXISTS entries_field_start;
         DROP INDEX IF EXISTS entries_live_field_start;
         DROP INDEX IF EXISTS entries_content_hash;
         DROP INDEX IF EXISTS entries_verb;
         DROP INDEX IF EXISTS entries_hybrid_time;
         DROP TABLE IF EXISTS entries;
         CREATE TABLE entries (
             hid             TEXT NOT NULL PRIMARY KEY,
             slug            TEXT NOT NULL,
             content_hash    TEXT NOT NULL,
             field           TEXT NOT NULL,
             state           TEXT,
             dur             TEXT,
             following_state TEXT,
             verb            TEXT NOT NULL DEFAULT 'point',
             data            TEXT,
             unit            TEXT,
             occur_time           TEXT NOT NULL,
             occur_time_offset    INTEGER,
             occur_time_tz        TEXT,
             occur_time_precision TEXT,
             lat             REAL,
             lon             REAL,
             location        TEXT,
             create_time     TEXT NOT NULL,
             modify_time     TEXT NOT NULL,
             device_id       TEXT NOT NULL,
             deleted_at      TEXT,
             id_structure    TEXT,
             field_structure TEXT,
             raw_input       TEXT NOT NULL DEFAULT '',
             alias_chain     TEXT,
             parser_version  INTEGER NOT NULL DEFAULT 1,
             user_asserted_last_state TEXT,
             extra_metadata  TEXT,
             CHECK (following_state IS NULL OR dur IS NOT NULL)
         );
         CREATE UNIQUE INDEX IF NOT EXISTS entries_slug ON entries (slug);
         CREATE INDEX IF NOT EXISTS entries_field_occur
             ON entries (field, occur_time DESC);
         CREATE INDEX IF NOT EXISTS entries_live_field_occur
             ON entries (field, occur_time)
             WHERE deleted_at IS NULL;
         CREATE INDEX IF NOT EXISTS entries_content_hash
             ON entries (content_hash);
         CREATE INDEX IF NOT EXISTS entries_verb ON entries (verb);
         CREATE INDEX IF NOT EXISTS entries_occur_time
             ON entries (occur_time, slug)
             WHERE deleted_at IS NULL;",
    )?;
    Ok(())
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

/// Reverse v19→v20: drop the v20 entries table and recreate the v19
/// shape (start_time/end_time/entry_shape/intent/value plus the
/// generated `verb` and `dur_seconds` columns). All v19-era indexes
/// that had been reseated onto `occur_time` are re-created on
/// `start_time`.
///
/// **DATA LOSS.** The v19→v20 forward jump rebuilds `entries` from
/// scratch (pre-v1 hard-cut, no production data). Walking back across
/// this step likewise drops every row currently in the table — the
/// (occur_time, state, dur, following_state, verb) shape cannot be
/// faithfully translated back to (start_time, end_time, entry_shape,
/// intent, value) without per-row context the down does not have.
/// The v19 stub recreated here exists so the chain-test harness can
/// verify schema reversibility; production callers must back up before
/// walking down across this step.
pub(super) fn down_20_to_19_time_shape_pivot(conn: &Connection) -> Result<()> {
    // Drop the v=20 indexes and table. `rebuild_entries_v3` would copy
    // rows from a v=19-shape `entries`, but at this point the table is
    // in v=20 shape — column-by-column copy would fail. The pre-v1
    // hard-cut means dropping the rows is the documented price of
    // walking back across this step, so we just recreate the v=19 stub
    // empty.
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_slug;
         DROP INDEX IF EXISTS entries_field_occur;
         DROP INDEX IF EXISTS entries_live_field_occur;
         DROP INDEX IF EXISTS entries_content_hash;
         DROP INDEX IF EXISTS entries_verb;
         DROP INDEX IF EXISTS entries_occur_time;
         DROP TABLE IF EXISTS entries;
         CREATE TABLE entries (
             hid             TEXT NOT NULL PRIMARY KEY,
             slug            TEXT NOT NULL,
             content_hash    TEXT NOT NULL,
             field           TEXT NOT NULL,
             entry_shape     TEXT NOT NULL,
             intent          TEXT,
             value           TEXT,
             data            TEXT,
             unit            TEXT,
             start_time      TEXT,
             start_time_offset INTEGER,
             start_time_tz   TEXT,
             start_time_precision INTEGER NOT NULL DEFAULT 0,
             start_time_precision_unit TEXT NOT NULL DEFAULT 'exact',
             end_time        TEXT,
             end_time_offset INTEGER,
             end_time_tz     TEXT,
             end_time_precision INTEGER NOT NULL DEFAULT 0,
             end_time_precision_unit TEXT NOT NULL DEFAULT 'exact',
             lat             REAL,
             lon             REAL,
             location        TEXT,
             create_time     TEXT NOT NULL,
             modify_time     TEXT NOT NULL,
             device_id       TEXT NOT NULL,
             deleted_at      TEXT,
             id_structure    TEXT,
             field_structure TEXT,
             raw_input       TEXT NOT NULL DEFAULT '',
             alias_chain     TEXT,
             parser_version  INTEGER NOT NULL DEFAULT 1,
             user_asserted_last_state TEXT,
             extra_metadata  TEXT,
             verb TEXT GENERATED ALWAYS AS (
                 CASE
                     WHEN intent IS NOT NULL THEN intent
                     WHEN entry_shape = 'ongoing' THEN 'start'
                     WHEN entry_shape = 'block' THEN 'block'
                     WHEN entry_shape = 'point' AND value IS NULL THEN 'occur'
                     WHEN entry_shape = 'point' AND json_valid(value)
                          AND json_type(value) IN ('true','false') THEN 'switch'
                     WHEN entry_shape = 'point' AND value IS NOT NULL THEN 'switch'
                     ELSE 'add'
                 END
             ) VIRTUAL,
             dur_seconds INTEGER GENERATED ALWAYS AS (
                 CASE WHEN start_time IS NOT NULL AND end_time IS NOT NULL
                     THEN CAST((julianday(end_time) - julianday(start_time)) * 86400 AS INTEGER)
                     ELSE NULL
                 END
             ) VIRTUAL
         );
         CREATE UNIQUE INDEX IF NOT EXISTS entries_slug ON entries (slug);
         CREATE INDEX IF NOT EXISTS entries_field_start
             ON entries (field, start_time DESC);
         CREATE INDEX IF NOT EXISTS entries_live_field_start
             ON entries (field, start_time)
             WHERE deleted_at IS NULL AND start_time IS NOT NULL;
         CREATE INDEX IF NOT EXISTS entries_content_hash
             ON entries (content_hash);
         CREATE INDEX IF NOT EXISTS entries_verb ON entries (verb);
         CREATE INDEX IF NOT EXISTS entries_hybrid_time
             ON entries (COALESCE(start_time, modify_time), slug)
             WHERE deleted_at IS NULL;",
    )?;
    Ok(())
}

/// Reverse v18→v19: drop `field_specs.render_json` via table rebuild.
/// Render-visibility overrides written after the up are discarded;
/// spec-side `render` values inside `spec_json` are untouched.
pub(super) fn down_19_to_18_field_specs_render_json(conn: &Connection) -> Result<()> {
    rebuild_field_specs(
        conn, /* with_kit_id = */ true, /* with_tags = */ true,
        /* with_render_json = */ false,
    )
}

/// Reverse v17→v18: drop `field_specs.tags` via table rebuild. Any
/// tag values written after the up are discarded. Spec-side tags inside
/// `spec_json` are untouched (they were never moved out by the up).
pub(super) fn down_18_to_17_field_specs_tags(conn: &Connection) -> Result<()> {
    rebuild_field_specs(
        conn, /* with_kit_id = */ true, /* with_tags = */ false,
        /* with_render_json = */ false,
    )
}

/// Reverse v16→v17: drop the kit_commands table and its index. Any
/// installed kit commands are lost; reinstall the kit to recreate them.
pub(super) fn down_17_to_16_kit_commands_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_kit_commands_kit_id;
         DROP TABLE IF EXISTS kit_commands;",
    )?;
    Ok(())
}

/// Reverse v15→v16: drop `aliases.kit_id` and its index via table
/// rebuild. Kit-installed alias rows lose their kit linkage but remain
/// in the table.
pub(super) fn down_16_to_15_aliases_kit_id(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_aliases_kit_id;
         CREATE TABLE aliases__v15 (
             name                TEXT NOT NULL PRIMARY KEY,
             alias_json          TEXT NOT NULL,
             derived_from_field  TEXT
         );
         INSERT INTO aliases__v15 (name, alias_json, derived_from_field)
            SELECT name, alias_json, derived_from_field FROM aliases;
         DROP TABLE aliases;
         ALTER TABLE aliases__v15 RENAME TO aliases;
         CREATE INDEX IF NOT EXISTS idx_aliases_derived_from_field
            ON aliases (derived_from_field);",
    )?;
    Ok(())
}

/// Reverse v14→v15: drop the kit_indices bookkeeping table. Any
/// real CREATE INDEX statements installed by kits remain on `entries`
/// but their (kit_id, index_name) ownership record is gone, so a
/// subsequent uninstall at v=14 cannot find them. Reinstall the kit
/// to repopulate the bookkeeping.
pub(super) fn down_15_to_14_kit_indices(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS kit_indices;")?;
    Ok(())
}

/// Reverse v13→v14: drop the views table and its index. Any installed
/// view definitions are lost; reinstall the kit to recreate them.
pub(super) fn down_14_to_13_views_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_views_kit_id;
         DROP TABLE IF EXISTS views;",
    )?;
    Ok(())
}

/// Reverse v12→v13: drop `field_specs.kit_id` and its index via table
/// rebuild. Any kit-installed rows lose their kit linkage; uninstall
/// by kit_id is no longer possible while at v=12.
pub(super) fn down_13_to_12_field_specs_kit_id(conn: &Connection) -> Result<()> {
    rebuild_field_specs(
        conn, /* with_kit_id = */ false, /* with_tags = */ false,
        /* with_render_json = */ false,
    )
}

/// Reverse v11→v12: drop `entries.extra_metadata` and merge any
/// `_import` markers back into `data`.
///
/// **PARTIAL DATA LOSS.** `content_hash` cannot be cleanly recomputed
/// to its pre-migration value (the v=11 hash is gone). After the down,
/// rows whose `_import` was relocated have `data` restored but
/// `content_hash` reflects the v=12 inputs. This is acceptable because
/// `content_hash` is internal: it is recomputed on rewrite, and dedup
/// against an external import re-derives it from the row payload.
pub(super) fn down_12_to_11_extra_metadata(conn: &Connection) -> Result<()> {
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('entries') WHERE name = 'extra_metadata'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        return Ok(());
    }

    // Re-merge any extra_metadata._import back into data so a later
    // forward walk reproduces the v=11 → v=12 transformation.
    let mut stmt = conn.prepare(
        "SELECT hid, data, extra_metadata
         FROM entries
         WHERE extra_metadata IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(crate::error::DatumError::Db)?;
    drop(stmt);

    for (hid, data_str, meta_str) in rows {
        let meta_val: serde_json::Value = serde_json::from_str(&meta_str).map_err(|e| {
            crate::error::DatumError::Other(format!("bad extra_metadata for hid {hid}: {e}"))
        })?;
        let import_val = meta_val.as_object().and_then(|m| m.get("_import")).cloned();
        let Some(import_val) = import_val else {
            continue;
        };
        let mut data_obj = match data_str.as_deref() {
            Some(s) => match serde_json::from_str::<serde_json::Value>(s) {
                Ok(serde_json::Value::Object(m)) => m,
                Ok(_) | Err(_) => serde_json::Map::new(),
            },
            None => serde_json::Map::new(),
        };
        data_obj.insert("_import".to_string(), import_val);
        let new_data = serde_json::Value::Object(data_obj).to_string();
        conn.execute(
            "UPDATE entries SET data = ?2 WHERE hid = ?1",
            params![hid, new_data],
        )?;
    }

    rebuild_entries_v3(
        conn, /* with_user_asserted = */ true, /* with_extra_metadata = */ false,
    )
}

/// Reverse v10→v11: drop the last_docs table.
pub(super) fn down_11_to_10_last_docs(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS last_docs;")?;
    Ok(())
}
