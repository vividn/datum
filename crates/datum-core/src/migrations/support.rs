//! Helpers shared by more than one migration era.
//!
//! Nothing here is a migration step; these are the introspection and
//! table-rebuild primitives the era modules call. They do not grow with
//! the schema version, which is why they live apart from the eras.

use rusqlite::{params, Connection};

use crate::error::Result;

// ─── Table rebuild helpers ───────────────────────────────────────────
//
// SQLite's `ALTER TABLE … DROP COLUMN` re-parses the stored CREATE
// statement and trips on the inline `--` comments embedded in
// schema.sql. Rather than rewriting schema.sql, the down migrations
// rebuild affected tables in-place using the standard 12-step pattern
// (CREATE new, copy rows, drop old, rename) so they don't depend on
// SQLite's stored-DDL re-parser.

/// Rebuild `entries` to a specific (unit, dur_precision) shape,
/// preserving all stored rows. Used by m22_to_23 (forward + down).
/// Indexes and the `following_state`/`dur` CHECK are recreated.
pub(super) fn rebuild_entries_v23(
    conn: &Connection,
    with_unit: bool,
    with_dur_precision: bool,
) -> Result<()> {
    let mut cols: Vec<&str> = vec![
        "hid TEXT NOT NULL PRIMARY KEY",
        "slug TEXT NOT NULL",
        "content_hash TEXT NOT NULL",
        "field TEXT NOT NULL",
        "state TEXT",
        "dur TEXT",
        "following_state TEXT",
        "verb TEXT NOT NULL DEFAULT 'point'",
        "data TEXT",
    ];
    if with_unit {
        cols.push("unit TEXT");
    }
    cols.push("occur_time TEXT NOT NULL");
    cols.push("occur_time_offset INTEGER");
    cols.push("occur_time_tz TEXT");
    cols.push("occur_time_precision TEXT");
    if with_dur_precision {
        cols.push("dur_precision TEXT");
    }
    cols.push("lat REAL");
    cols.push("lon REAL");
    cols.push("location TEXT");
    cols.push("create_time TEXT NOT NULL");
    cols.push("modify_time TEXT NOT NULL");
    cols.push("device_id TEXT NOT NULL");
    cols.push("deleted_at TEXT");
    cols.push("id_structure TEXT");
    cols.push("field_structure TEXT");
    cols.push("raw_input TEXT NOT NULL DEFAULT ''");
    cols.push("alias_chain TEXT");
    cols.push("parser_version INTEGER NOT NULL DEFAULT 1");
    cols.push("user_asserted_last_state TEXT");
    cols.push("extra_metadata TEXT");
    cols.push("CHECK (following_state IS NULL OR dur IS NOT NULL)");

    let create = format!("CREATE TABLE entries__new ({});", cols.join(", "));

    // Stored columns common to both shapes — drive the column list of
    // both the INSERT target and the SELECT source.
    let mut common: Vec<&str> = vec![
        "hid",
        "slug",
        "content_hash",
        "field",
        "state",
        "dur",
        "following_state",
        "verb",
        "data",
    ];
    let pre_has_unit = has_column(conn, "entries", "unit")?;
    let pre_has_dur_precision = has_column(conn, "entries", "dur_precision")?;
    let copy_unit = with_unit && pre_has_unit;
    let copy_dur_precision = with_dur_precision && pre_has_dur_precision;
    if copy_unit {
        common.push("unit");
    }
    common.push("occur_time");
    common.push("occur_time_offset");
    common.push("occur_time_tz");
    common.push("occur_time_precision");
    if copy_dur_precision {
        common.push("dur_precision");
    }
    common.extend([
        "lat",
        "lon",
        "location",
        "create_time",
        "modify_time",
        "device_id",
        "deleted_at",
        "id_structure",
        "field_structure",
        "raw_input",
        "alias_chain",
        "parser_version",
        "user_asserted_last_state",
        "extra_metadata",
    ]);

    // Target column list mirrors the source order. When the target carries
    // a column the source lacks (e.g. forward step that adds dur_precision
    // to an old v=22 row), we fill it with NULL via a SELECT literal so the
    // positional INSERT lines up.
    let mut target_cols = common.clone();
    let mut select_exprs: Vec<String> = common.iter().map(|c| (*c).to_string()).collect();
    if with_unit && !copy_unit {
        target_cols.push("unit");
        select_exprs.push("NULL".to_string());
    }
    if with_dur_precision && !copy_dur_precision {
        target_cols.push("dur_precision");
        select_exprs.push("NULL".to_string());
    }

    let copy = format!(
        "INSERT INTO entries__new ({}) SELECT {} FROM entries;",
        target_cols.join(", "),
        select_exprs.join(", "),
    );

    let mut sql = String::new();
    sql.push_str("DROP INDEX IF EXISTS entries_slug;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_field_occur;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_live_field_occur;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_content_hash;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_verb;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_occur_time;\n");
    sql.push_str(&create);
    sql.push_str(&copy);
    sql.push_str("DROP TABLE entries;\n");
    sql.push_str("ALTER TABLE entries__new RENAME TO entries;\n");
    sql.push_str("CREATE UNIQUE INDEX IF NOT EXISTS entries_slug ON entries (slug);\n");
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_field_occur \
            ON entries (field, occur_time DESC);\n",
    );
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_live_field_occur \
            ON entries (field, occur_time) \
            WHERE deleted_at IS NULL;\n",
    );
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_content_hash \
            ON entries (content_hash);\n",
    );
    sql.push_str("CREATE INDEX IF NOT EXISTS entries_verb ON entries (verb);\n");
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_occur_time \
            ON entries (occur_time, slug) \
            WHERE deleted_at IS NULL;\n",
    );
    conn.execute_batch(&sql)?;
    Ok(())
}

/// Rebuild `entries` toggling only the `occur_time` nullability, keeping
/// all other columns, the `following_state` CHECK, and all nine indexes.
/// When `occur_nullable` is false (down), NULL `occur_time` rows are
/// backfilled with `create_time` so the restored `NOT NULL` holds.
pub(super) fn rebuild_entries_occur_time(conn: &Connection, occur_nullable: bool) -> Result<()> {
    let occur_col = if occur_nullable {
        "occur_time TEXT"
    } else {
        "occur_time TEXT NOT NULL"
    };
    let occur_select = if occur_nullable {
        "occur_time"
    } else {
        "COALESCE(occur_time, create_time)"
    };

    let cols = [
        "hid TEXT NOT NULL PRIMARY KEY",
        "slug TEXT NOT NULL",
        "content_hash TEXT NOT NULL",
        "field TEXT NOT NULL",
        "state TEXT",
        "dur TEXT",
        "following_state TEXT",
        "verb TEXT NOT NULL DEFAULT 'point'",
        "data TEXT",
        occur_col,
        "occur_time_offset INTEGER",
        "occur_time_tz TEXT",
        "occur_time_precision TEXT",
        "dur_precision TEXT",
        "lat REAL",
        "lon REAL",
        "location TEXT",
        "create_time TEXT NOT NULL",
        "modify_time TEXT NOT NULL",
        "device_id TEXT NOT NULL",
        "deleted_at TEXT",
        "id_structure TEXT",
        "field_structure TEXT",
        "raw_input TEXT NOT NULL DEFAULT ''",
        "alias_chain TEXT",
        "parser_version INTEGER NOT NULL DEFAULT 1",
        "user_asserted_last_state TEXT",
        "extra_metadata TEXT",
        "CHECK (following_state IS NULL OR dur IS NOT NULL)",
    ];

    // Column list mirrors `cols` (minus the trailing CHECK) for the
    // positional INSERT; the SELECT swaps `occur_time` for its (possibly
    // COALESCE'd) source expression.
    let target_cols = "hid, slug, content_hash, field, state, dur, following_state, verb, data, \
         occur_time, occur_time_offset, occur_time_tz, occur_time_precision, dur_precision, \
         lat, lon, location, create_time, modify_time, device_id, deleted_at, \
         id_structure, field_structure, raw_input, alias_chain, parser_version, \
         user_asserted_last_state, extra_metadata";
    let select_exprs = format!(
        "hid, slug, content_hash, field, state, dur, following_state, verb, data, \
         {occur_select}, occur_time_offset, occur_time_tz, occur_time_precision, dur_precision, \
         lat, lon, location, create_time, modify_time, device_id, deleted_at, \
         id_structure, field_structure, raw_input, alias_chain, parser_version, \
         user_asserted_last_state, extra_metadata"
    );

    let mut sql = String::new();
    sql.push_str("DROP INDEX IF EXISTS entries_slug;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_field_occurtime;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_field_occur;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_live_field_occur;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_content_hash;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_verb;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_occur_time;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_id_structure;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_field_structure;\n");
    sql.push_str(&format!(
        "CREATE TABLE entries__new ({});\n",
        cols.join(", ")
    ));
    sql.push_str(&format!(
        "INSERT INTO entries__new ({target_cols}) SELECT {select_exprs} FROM entries;\n"
    ));
    sql.push_str("DROP TABLE entries;\n");
    sql.push_str("ALTER TABLE entries__new RENAME TO entries;\n");
    sql.push_str("CREATE UNIQUE INDEX IF NOT EXISTS entries_slug ON entries (slug);\n");
    sql.push_str(
        "CREATE UNIQUE INDEX IF NOT EXISTS entries_field_occurtime \
            ON entries (field, occur_time) \
            WHERE occur_time IS NOT NULL AND deleted_at IS NULL;\n",
    );
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_field_occur \
            ON entries (field, occur_time DESC);\n",
    );
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_live_field_occur \
            ON entries (field, occur_time) \
            WHERE deleted_at IS NULL;\n",
    );
    sql.push_str("CREATE INDEX IF NOT EXISTS entries_content_hash ON entries (content_hash);\n");
    sql.push_str("CREATE INDEX IF NOT EXISTS entries_verb ON entries (verb);\n");
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_occur_time \
            ON entries (occur_time, slug) \
            WHERE deleted_at IS NULL;\n",
    );
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_id_structure \
            ON entries (hid) WHERE id_structure IS NOT NULL;\n",
    );
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_field_structure \
            ON entries (hid) WHERE field_structure IS NOT NULL;\n",
    );
    conn.execute_batch(&sql)?;
    Ok(())
}

/// Rebuild `field_specs` to a specific (kit_id, tags, render_json)
/// shape, preserving all rows. The PK and `idx_field_specs_kit_id`
/// index are recreated as appropriate for the target shape.
pub(super) fn rebuild_field_specs(
    conn: &Connection,
    with_kit_id: bool,
    with_tags: bool,
    with_render_json: bool,
) -> Result<()> {
    let mut cols = vec!["field TEXT NOT NULL PRIMARY KEY", "spec_json TEXT NOT NULL"];
    if with_kit_id {
        cols.push("kit_id TEXT");
    }
    if with_tags {
        cols.push("tags TEXT NOT NULL DEFAULT '[]'");
    }
    if with_render_json {
        cols.push("render_json TEXT");
    }
    let create = format!("CREATE TABLE field_specs__new ({});", cols.join(", "));

    let mut select_cols = vec!["field", "spec_json"];
    if with_kit_id {
        select_cols.push("kit_id");
    }
    if with_tags {
        select_cols.push("tags");
    }
    if with_render_json {
        select_cols.push("render_json");
    }
    let copy = format!(
        "INSERT INTO field_specs__new ({c}) SELECT {c} FROM field_specs;",
        c = select_cols.join(", ")
    );

    let mut sql = String::new();
    sql.push_str("DROP INDEX IF EXISTS idx_field_specs_kit_id;\n");
    sql.push_str(&create);
    sql.push_str(&copy);
    sql.push_str("DROP TABLE field_specs;\n");
    sql.push_str("ALTER TABLE field_specs__new RENAME TO field_specs;\n");
    if with_kit_id {
        sql.push_str("CREATE INDEX IF NOT EXISTS idx_field_specs_kit_id ON field_specs(kit_id);\n");
    }
    conn.execute_batch(&sql)?;
    Ok(())
}

/// Rebuild `aliases` to a specific (derived_from_field) shape, preserving
/// all rows. The `kit_id` column is always present (added by m15→16, well
/// before m20→21), so we don't condition on it.
pub(super) fn rebuild_aliases(conn: &Connection, with_derived_from_field: bool) -> Result<()> {
    let mut cols = vec!["name TEXT NOT NULL PRIMARY KEY", "alias_json TEXT NOT NULL"];
    if with_derived_from_field {
        cols.push("derived_from_field TEXT");
    }
    cols.push("kit_id TEXT");
    let create = format!("CREATE TABLE aliases__new ({});", cols.join(", "));

    let mut select_cols = vec!["name", "alias_json"];
    if with_derived_from_field {
        // Forward (drop column) never takes this branch. The down branch
        // re-adds the column populated with NULL — no rows preserve the
        // tag because the forward step already deleted derived rows.
        select_cols.push("NULL AS derived_from_field");
    }
    select_cols.push("kit_id");

    let copy = format!(
        "INSERT INTO aliases__new (name, alias_json{}, kit_id) \
            SELECT {} FROM aliases;",
        if with_derived_from_field {
            ", derived_from_field"
        } else {
            ""
        },
        select_cols.join(", ")
    );

    let mut sql = String::new();
    sql.push_str("DROP INDEX IF EXISTS idx_aliases_derived_from_field;\n");
    sql.push_str("DROP INDEX IF EXISTS idx_aliases_kit_id;\n");
    sql.push_str(&create);
    sql.push_str(&copy);
    sql.push_str("DROP TABLE aliases;\n");
    sql.push_str("ALTER TABLE aliases__new RENAME TO aliases;\n");
    sql.push_str("CREATE INDEX IF NOT EXISTS idx_aliases_kit_id ON aliases (kit_id);\n");
    if with_derived_from_field {
        sql.push_str(
            "CREATE INDEX IF NOT EXISTS idx_aliases_derived_from_field \
                ON aliases (derived_from_field);\n",
        );
    }
    conn.execute_batch(&sql)?;
    Ok(())
}

/// Rebuild `entries` to the v3 shape with optional v8 (`user_asserted_last_state`)
/// and v12 (`extra_metadata`) columns. Generated columns (`verb`, `dur_seconds`)
/// and the v3-era indexes are recreated. Late-binding indexes
/// (`entries_live_field_start`, `entries_hybrid_time`) are NOT recreated
/// here — the chain runner only invokes this from down steps that have
/// already torn those indexes down via their own dedicated downs.
pub(super) fn rebuild_entries_v3(
    conn: &Connection,
    with_user_asserted: bool,
    with_extra_metadata: bool,
) -> Result<()> {
    // Build the column list for the new table. Order must match the
    // SELECT below so positional INSERT lines up.
    let mut cols: Vec<&str> = vec![
        "hid TEXT NOT NULL PRIMARY KEY",
        "slug TEXT NOT NULL",
        "content_hash TEXT NOT NULL",
        "field TEXT NOT NULL",
        "entry_shape TEXT NOT NULL",
        "intent TEXT",
        "value TEXT",
        "data TEXT",
        "unit TEXT",
        "start_time TEXT",
        "start_time_offset INTEGER",
        "start_time_tz TEXT",
        "start_time_precision INTEGER NOT NULL DEFAULT 0",
        "start_time_precision_unit TEXT NOT NULL DEFAULT 'exact'",
        "end_time TEXT",
        "end_time_offset INTEGER",
        "end_time_tz TEXT",
        "end_time_precision INTEGER NOT NULL DEFAULT 0",
        "end_time_precision_unit TEXT NOT NULL DEFAULT 'exact'",
        "lat REAL",
        "lon REAL",
        "location TEXT",
        "create_time TEXT NOT NULL",
        "modify_time TEXT NOT NULL",
        "device_id TEXT NOT NULL",
        "deleted_at TEXT",
        "id_structure TEXT",
        "field_structure TEXT",
        "raw_input TEXT NOT NULL DEFAULT ''",
        "alias_chain TEXT",
        "parser_version INTEGER NOT NULL DEFAULT 1",
    ];
    if with_user_asserted {
        cols.push("user_asserted_last_state TEXT");
    }
    if with_extra_metadata {
        cols.push("extra_metadata TEXT");
    }
    // Generated columns are positional and must come last.
    let generated = "verb TEXT GENERATED ALWAYS AS (
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
            ) VIRTUAL";

    // The columns we copy = stored columns only (no generated).
    let stored: Vec<&str> = vec![
        "hid",
        "slug",
        "content_hash",
        "field",
        "entry_shape",
        "intent",
        "value",
        "data",
        "unit",
        "start_time",
        "start_time_offset",
        "start_time_tz",
        "start_time_precision",
        "start_time_precision_unit",
        "end_time",
        "end_time_offset",
        "end_time_tz",
        "end_time_precision",
        "end_time_precision_unit",
        "lat",
        "lon",
        "location",
        "create_time",
        "modify_time",
        "device_id",
        "deleted_at",
        "id_structure",
        "field_structure",
        "raw_input",
        "alias_chain",
        "parser_version",
    ];
    let mut copy_cols: Vec<&str> = stored.clone();
    if with_user_asserted {
        copy_cols.push("user_asserted_last_state");
    }
    if with_extra_metadata {
        copy_cols.push("extra_metadata");
    }
    let copy_list = copy_cols.join(", ");

    let create = format!(
        "CREATE TABLE entries__new ({}, {});",
        cols.join(", "),
        generated
    );
    let copy = format!(
        "INSERT INTO entries__new ({c}) SELECT {c} FROM entries;",
        c = copy_list
    );

    let mut sql = String::new();
    // Drop indexes that reference `entries` so the rename can succeed
    // without renaming dependents. The chain runner has typically
    // already torn down v6+ / v10+ indexes via their dedicated downs,
    // but DROP INDEX IF EXISTS keeps this idempotent either way.
    sql.push_str("DROP INDEX IF EXISTS entries_slug;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_field_start;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_content_hash;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_verb;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_live_field_start;\n");
    sql.push_str("DROP INDEX IF EXISTS entries_hybrid_time;\n");
    sql.push_str(&create);
    sql.push_str(&copy);
    sql.push_str("DROP TABLE entries;\n");
    sql.push_str("ALTER TABLE entries__new RENAME TO entries;\n");
    // Recreate the v3-era indexes that always exist on entries.
    sql.push_str("CREATE UNIQUE INDEX IF NOT EXISTS entries_slug ON entries (slug);\n");
    sql.push_str(
        "CREATE INDEX IF NOT EXISTS entries_field_start ON entries (field, start_time DESC);\n",
    );
    sql.push_str("CREATE INDEX IF NOT EXISTS entries_content_hash ON entries (content_hash);\n");
    sql.push_str("CREATE INDEX IF NOT EXISTS entries_verb ON entries (verb);\n");
    conn.execute_batch(&sql)?;
    Ok(())
}

pub(super) fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(n > 0)
}
