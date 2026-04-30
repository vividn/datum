//! Schema version tracking and forward migrations.
//!
//! Every DB carries a `_meta.schema_version` row that this binary compares
//! against [`CURRENT_SCHEMA_VERSION`]. Forward-compatible migrations
//! (ADD COLUMN, CREATE INDEX IF NOT EXISTS, CREATE TABLE IF NOT EXISTS)
//! run silently on open; anything destructive (column removal, PK change)
//! must go through `datum setup --force-recreate` or a proper migration
//! entry here.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{DatumError, Result};

/// The schema version this binary writes.
///
/// Bump this whenever `schema.sql` changes AND append a [`Migration`] that
/// lifts the previous version to the new one. The invariant is that a DB
/// at any version in `[1, CURRENT_SCHEMA_VERSION]` can be migrated to the
/// current shape by walking [`MIGRATIONS`] in order.
pub const CURRENT_SCHEMA_VERSION: u32 = 19;

/// A single forward migration between two adjacent schema versions.
///
/// Migrations run inside a transaction opened by the caller. `apply` must
/// be idempotent against a partially-applied state where possible — in
/// practice this means using `CREATE … IF NOT EXISTS` / `ALTER … ADD
/// COLUMN` patterns so interrupted runs can be retried safely.
pub struct Migration {
    pub from: u32,
    pub to: u32,
    pub summary: &'static str,
    pub apply: fn(&Connection) -> Result<()>,
}

/// Ordered forward migrations. Append-only — never reorder, rename, or
/// delete entries here.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        from: 1,
        to: 2,
        summary: "sync_state: replace (device_id,last_seq,updated_at) with (db_id,push_seq,pull_seq,last_push_at,last_pull_at)",
        apply: m1_to_2_sync_state,
    },
    Migration {
        from: 2,
        to: 3,
        summary: "data model v2: entries table replaced — entry_shape, intent, precision fields, timestamp rename, hid-PK, generated verb/dur_seconds, raw_input/alias_chain/parser_version",
        apply: m2_to_3_entries_v2,
    },
    Migration {
        from: 3,
        to: 4,
        summary: "entry_history: append-only modification trail (entry_hid, version, snapshot, change_kind)",
        apply: m3_to_4_entry_history,
    },
    Migration {
        from: 4,
        to: 5,
        summary: "sync_conflicts: non-blocking overlap / double-open / orphan-end queue (local_validator + sync_pull)",
        apply: m4_to_5_sync_conflicts,
    },
    Migration {
        from: 5,
        to: 6,
        summary: "entries_live_field_start: partial index for SQL-backed views (deleted_at IS NULL AND start_time IS NOT NULL)",
        apply: m5_to_6_live_field_index,
    },
    Migration {
        from: 6,
        to: 7,
        summary: "sync_state: add my_push_seq column (per-device AAD seq counter for C4c push)",
        apply: m6_to_7_sync_state_my_push_seq,
    },
    Migration {
        from: 7,
        to: 8,
        summary: "entries: add user_asserted_last_state column (S6 — explicit -L assertion value)",
        apply: m7_to_8_entries_user_asserted_last_state,
    },
    Migration {
        from: 8,
        to: 9,
        summary: "track/ignore unification (da-2sr): convert field_specs.ignored=true rows into switch IGNORED entries; spec retains schema only",
        apply: m8_to_9_track_ignore_unification,
    },
    Migration {
        from: 9,
        to: 10,
        summary: "entries_hybrid_time: expression index on COALESCE(start_time, modify_time) for bare-time tail/head (da-7iu)",
        apply: m9_to_10_hybrid_time_index,
    },
    Migration {
        from: 10,
        to: 11,
        summary: "last_docs: singleton pointer to the hids of the most-recently-accessed batch (da-e65)",
        apply: m10_to_11_last_docs,
    },
    Migration {
        from: 11,
        to: 12,
        summary: "entries: add extra_metadata column for system provenance; relocate datum-migrate `_import` marker out of data (da-tsw)",
        apply: m11_to_12_extra_metadata,
    },
    Migration {
        from: 12,
        to: 13,
        summary: "field_specs: add kit_id column for kit-namespaced field specs; index on kit_id (da-d4t.1)",
        apply: m12_to_13_field_specs_kit_id,
    },
    Migration {
        from: 13,
        to: 14,
        summary: "views: named SQL views table (kit-scoped); powers `datum view <name>` (da-d4t.4)",
        apply: m13_to_14_views_table,
    },
    Migration {
        from: 14,
        to: 15,
        summary: "kit_indices: track CREATE INDEX statements installed by kits so uninstall can DROP INDEX cleanly (da-d4t.5)",
        apply: m14_to_15_kit_indices,
    },
    Migration {
        from: 15,
        to: 16,
        summary: "aliases: add kit_id column for kit-namespaced [[alias]] rows; index on kit_id (da-d4t.3)",
        apply: m15_to_16_aliases_kit_id,
    },
    Migration {
        from: 16,
        to: 17,
        summary: "kit_commands: kit-defined Level 1/2 commands (alias expansion + view+presentation) (da-d4t.6)",
        apply: m16_to_17_kit_commands_table,
    },
    Migration {
        from: 17,
        to: 18,
        summary: "field_specs: add tags TEXT column (JSON array, default '[]') for kit/view tag filtering (da-a7g)",
        apply: m17_to_18_field_specs_tags,
    },
    Migration {
        from: 18,
        to: 19,
        summary: "field_specs: add render_json TEXT column (NULL = visible everywhere) for per-surface render visibility (da-2zd)",
        apply: m18_to_19_field_specs_render_json,
    },
];

/// da-2zd: per-surface render visibility. `render_json` mirrors
/// `FieldSpec.render` out of `spec_json` so visual surfaces (dayview,
/// nowview, future renderers) can filter without a JSON parse. NULL means
/// the implicit default — visible everywhere. Additive + idempotent.
fn m18_to_19_field_specs_render_json(conn: &Connection) -> Result<()> {
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

/// da-a7g: tag-based field grouping. `tags` mirrors `FieldSpec.tags` out of
/// `spec_json` into a column so SQL filters (`tags LIKE '%"chore"%'`) and
/// the `datum spec list --tag` query can run without JSON parsing. Additive
/// + idempotent.
fn m17_to_18_field_specs_tags(conn: &Connection) -> Result<()> {
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

/// da-d4t.3: kits scope `aliases` rows. `kit_id` is NULL for user-declared
/// aliases (set via `datum alias set` or seeded into the schema) and the
/// kit's name for rows installed by `datum kit install` from a `[[alias]]`
/// block. Uninstall deletes only rows with the matching `kit_id`. Additive +
/// idempotent.
fn m15_to_16_aliases_kit_id(conn: &Connection) -> Result<()> {
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
fn m16_to_17_kit_commands_table(conn: &Connection) -> Result<()> {
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

/// da-d4t.4: kit-installable named SELECT views. Rows are tagged with
/// `kit_id` (NULL for user-declared rows, the kit's name for installed
/// rows); uninstall drops only rows with the matching `kit_id`. Additive +
/// idempotent — `CREATE TABLE IF NOT EXISTS` lets a half-applied run retry.
fn m13_to_14_views_table(conn: &Connection) -> Result<()> {
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
fn m14_to_15_kit_indices(conn: &Connection) -> Result<()> {
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

/// da-d4t.1: kits scope `field_specs` rows. `kit_id` is NULL for
/// user-declared specs (set via `datum spec set` or auto-tracked) and
/// non-NULL for rows installed by `datum kit install`. Uninstall deletes
/// only rows with the matching `kit_id`. Additive + idempotent.
fn m12_to_13_field_specs_kit_id(conn: &Connection) -> Result<()> {
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

fn m11_to_12_extra_metadata(conn: &Connection) -> Result<()> {
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

fn m10_to_11_last_docs(conn: &Connection) -> Result<()> {
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

fn m9_to_10_hybrid_time_index(conn: &Connection) -> Result<()> {
    // da-7iu: bare-time `tail`/`head` (no `--field`) orders by the default
    // hybrid metric `COALESCE(start_time, modify_time)`. Without this index
    // the planner does a full SCAN + temp B-tree sort. Additive, idempotent.
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS entries_hybrid_time
            ON entries (COALESCE(start_time, modify_time), slug)
            WHERE deleted_at IS NULL;",
    )?;
    Ok(())
}

fn m1_to_2_sync_state(conn: &Connection) -> Result<()> {
    // The v1 sync_state shape was a placeholder that was never wired up to
    // any write path, so dropping it is safe. Recreate with the C4a shape.
    conn.execute_batch(
        "DROP TABLE IF EXISTS sync_state;
         CREATE TABLE sync_state (
             db_id         TEXT NOT NULL,
             push_seq      INTEGER NOT NULL DEFAULT 0,
             pull_seq      INTEGER NOT NULL DEFAULT 0,
             last_push_at  INTEGER,
             last_pull_at  INTEGER,
             PRIMARY KEY (db_id)
         );",
    )?;
    Ok(())
}

fn m2_to_3_entries_v2(conn: &Connection) -> Result<()> {
    // No production data yet — the v2 → v3 jump is a structural rewrite of
    // the entries table. Drop and let schema.sql recreate with the new
    // shape (hid PK, entry_shape, precision, intent, generated verb, etc.).
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_entries_hid;
         DROP INDEX IF EXISTS idx_entries_field;
         DROP INDEX IF EXISTS idx_entries_time;
         DROP INDEX IF EXISTS idx_entries_content_hash;
         DROP TABLE IF EXISTS entries;",
    )?;
    Ok(())
}

fn m8_to_9_track_ignore_unification(conn: &Connection) -> Result<()> {
    // da-2sr: tracking state moves out of the spec and into the entry log.
    // For every existing spec with `ignored = true`, write a switch entry
    // with value = "IGNORED" at create_time = NOW so the field is reported
    // as ignored under the new derivation. The legacy `tracked` / `ignored`
    // keys remain inside `spec_json` (serde tolerates them as unknown
    // fields on read); the binary just stops reading them.
    let mut stmt = conn.prepare("SELECT spec_json FROM field_specs")?;
    let specs: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(crate::error::DatumError::Db)?;
    drop(stmt);

    let now_dt = crate::time::DatumTime::now();
    for spec_json in specs {
        let v: serde_json::Value = serde_json::from_str(&spec_json)
            .map_err(|e| crate::error::DatumError::Other(format!("bad spec_json: {e}")))?;
        let ignored = v.get("ignored").and_then(|x| x.as_bool()).unwrap_or(false);
        if !ignored {
            continue;
        }
        let field = match v.get("field").and_then(|x| x.as_str()) {
            Some(f) if !f.is_empty() => f.to_string(),
            _ => continue,
        };
        let entry = crate::entry::Entry::new(
            field,
            crate::entry::Verb::Switch,
            Some(serde_json::Value::String("IGNORED".to_string())),
            None,
            None,
            Some(now_dt.clone()),
            None,
            "migration-v9",
        );
        crate::db::try_insert(conn, &entry.slug, &entry.hid, &entry)?;
    }
    Ok(())
}

fn m7_to_8_entries_user_asserted_last_state(conn: &Connection) -> Result<()> {
    // S6: record explicit --last-state=<v> assertions on switch entries.
    // Nullable — most entries leave it NULL; mismatches between the assertion
    // and the computed last-state are filed as `last_state_mismatch` rows in
    // sync_conflicts. The column is metadata only and does not participate in
    // content_hash so the same switch logged with or without -L dedups the same.
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('entries') WHERE name = 'user_asserted_last_state'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        conn.execute_batch("ALTER TABLE entries ADD COLUMN user_asserted_last_state TEXT;")?;
    }
    Ok(())
}

fn m6_to_7_sync_state_my_push_seq(conn: &Connection) -> Result<()> {
    // Purely additive: the new column defaults to 0 so any DB that's
    // already pushed (there aren't any pre-C4c) behaves as if it's never
    // pushed. `ALTER TABLE ... ADD COLUMN` is natively idempotent via an
    // introspection check so replaying this step is safe.
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sync_state') WHERE name = 'my_push_seq'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        conn.execute_batch(
            "ALTER TABLE sync_state ADD COLUMN my_push_seq INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    Ok(())
}

fn m5_to_6_live_field_index(conn: &Connection) -> Result<()> {
    // R3: the SQL-backed views (timing, duration_sum, state_change)
    // consistently filter `deleted_at IS NULL AND start_time IS NOT NULL`
    // and push a field equality down into SQL. A partial index lets the
    // planner skip tombstoned/untimed rows entirely.
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS entries_live_field_start
            ON entries (field, start_time)
            WHERE deleted_at IS NULL AND start_time IS NOT NULL;",
    )?;
    Ok(())
}

fn m4_to_5_sync_conflicts(conn: &Connection) -> Result<()> {
    // Purely additive: schema.sql re-run by Db::migrate after migrations
    // will bring the table up on fresh DBs. For existing v4 DBs we create
    // it explicitly here so the table exists as soon as the version bumps.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sync_conflicts (
            id              TEXT NOT NULL PRIMARY KEY,
            hid             TEXT NOT NULL,
            entry_slug      TEXT NOT NULL,
            local_entry     TEXT NOT NULL,
            remote_entry    TEXT,
            detected_at     TEXT NOT NULL,
            resolution      TEXT,
            resolved_at     TEXT,
            detected_by     TEXT NOT NULL,
            conflict_kind   TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS sync_conflicts_hid ON sync_conflicts(hid);
        CREATE INDEX IF NOT EXISTS sync_conflicts_slug ON sync_conflicts(entry_slug);
        CREATE INDEX IF NOT EXISTS sync_conflicts_pending
            ON sync_conflicts(detected_at) WHERE resolution IS NULL;",
    )?;
    Ok(())
}

fn m3_to_4_entry_history(conn: &Connection) -> Result<()> {
    // Purely additive: create the entry_history table and indexes. Any rows
    // that existed pre-D4 will lack a version=1 "create" history row, so
    // undo/history commands degrade gracefully for those (treated as
    // having no recorded history).
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entry_history (
            entry_hid         TEXT NOT NULL,
            version           INTEGER NOT NULL,
            snapshot          TEXT NOT NULL,
            modify_time       TEXT NOT NULL,
            raw_input         TEXT NOT NULL,
            alias_chain       TEXT,
            device_id         TEXT NOT NULL,
            change_kind       TEXT NOT NULL,
            PRIMARY KEY (entry_hid, version)
         );
         CREATE INDEX IF NOT EXISTS entry_history_entry
            ON entry_history(entry_hid, version DESC);
         CREATE INDEX IF NOT EXISTS entry_history_change_kind
            ON entry_history(change_kind);",
    )?;
    Ok(())
}

/// Create the `_meta` key/value table if it does not exist.
pub fn ensure_meta_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _meta (
            key    TEXT PRIMARY KEY,
            value  TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// Read the stored schema version, if any.
pub fn read_schema_version(conn: &Connection) -> Result<Option<u32>> {
    ensure_meta_table(conn)?;
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    match value {
        Some(s) => s
            .parse::<u32>()
            .map(Some)
            .map_err(|e| DatumError::Other(format!("bad schema_version in _meta: {e}"))),
        None => Ok(None),
    }
}

/// Write the schema version.
pub fn set_schema_version(conn: &Connection, version: u32) -> Result<()> {
    ensure_meta_table(conn)?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![version.to_string()],
    )?;
    Ok(())
}

/// Detect the schema shape of a DB that predates `_meta`. Returns the
/// version we should stamp it at. For now, any slug-as-PK shape is v1;
/// anything older is rejected upstream by `Db::open`.
pub fn infer_legacy_version(_conn: &Connection) -> Result<u32> {
    // The legacy check in `Db::migrate()` has already rejected pre-refactor
    // ULID-based tables by the time we get here. Anything else is v1.
    Ok(1)
}

/// Run forward migrations from `current` up to `target`, using the given
/// registry. All pending steps run inside a single transaction so a partial
/// failure rolls back to `current` instead of leaving a half-migrated DB.
/// Returns a list of summary strings for each migration applied.
pub fn apply_migrations(
    conn: &Connection,
    migrations: &[Migration],
    current: u32,
    target: u32,
) -> Result<Vec<String>> {
    if current == target {
        return Ok(vec![]);
    }
    if current > target {
        return Err(DatumError::IncompatibleSchema(format!(
            "database schema version {current} is newer than this binary supports \
             (expected ≤ {target}); upgrade datum"
        )));
    }
    let tx = conn.unchecked_transaction()?;
    let mut applied = Vec::new();
    let mut version = current;
    while version < target {
        let next = migrations
            .iter()
            .find(|m| m.from == version)
            .ok_or_else(|| {
                DatumError::IncompatibleSchema(format!(
                    "no migration path from schema v{version} to v{target}"
                ))
            })?;
        (next.apply)(&tx)?;
        version = next.to;
        set_schema_version(&tx, version)?;
        applied.push(format!("v{}→v{}: {}", next.from, next.to, next.summary));
    }
    tx.commit()?;
    Ok(applied)
}

/// Status row reported by [`migration_status`].
#[derive(Debug, Clone)]
pub struct MigrationStatusRow {
    pub from: u32,
    pub to: u32,
    pub summary: &'static str,
    pub applied: bool,
}

/// Snapshot of migration state for reporting. Returns the current stored
/// schema version, the binary's target version, and a row per registered
/// migration tagged as applied/pending.
pub fn migration_status(
    conn: &Connection,
    migrations: &[Migration],
) -> Result<(u32, u32, Vec<MigrationStatusRow>)> {
    let current = read_schema_version(conn)?.unwrap_or(CURRENT_SCHEMA_VERSION);
    let target = CURRENT_SCHEMA_VERSION;
    let rows = migrations
        .iter()
        .map(|m| MigrationStatusRow {
            from: m.from,
            to: m.to,
            summary: m.summary,
            applied: m.to <= current,
        })
        .collect();
    Ok((current, target, rows))
}

/// Migrate an open connection to [`CURRENT_SCHEMA_VERSION`] using the
/// module-level [`MIGRATIONS`] registry. Returns the summary lines.
pub fn migrate_to_current(conn: &Connection) -> Result<Vec<String>> {
    let current = match read_schema_version(conn)? {
        Some(v) => v,
        None => {
            let inferred = infer_legacy_version(conn)?;
            set_schema_version(conn, inferred)?;
            inferred
        }
    };
    apply_migrations(conn, MIGRATIONS, current, CURRENT_SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn fresh_db_stamps_current_schema_version() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(
            read_schema_version(db.conn()).unwrap(),
            Some(CURRENT_SCHEMA_VERSION)
        );
    }

    #[test]
    fn opening_db_with_newer_schema_version_errors() {
        let db = Db::open_in_memory().unwrap();
        // Force _meta.schema_version above what this binary supports.
        set_schema_version(db.conn(), CURRENT_SCHEMA_VERSION + 5).unwrap();
        drop(db);

        // Re-running migrate_to_current directly (Db::open_in_memory would
        // lose state) emulates what Db::migrate does on a file-backed DB.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        set_schema_version(&conn, CURRENT_SCHEMA_VERSION + 1).unwrap();
        let err = migrate_to_current(&conn).unwrap_err();
        assert!(
            matches!(err, DatumError::IncompatibleSchema(_)),
            "expected IncompatibleSchema, got {err:?}"
        );
    }

    #[test]
    fn apply_migrations_runs_registered_steps_in_order() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        set_schema_version(&conn, 1).unwrap();

        fn m1_to_2(conn: &Connection) -> Result<()> {
            conn.execute_batch("CREATE TABLE IF NOT EXISTS synth_v2 (x INTEGER);")?;
            Ok(())
        }
        fn m2_to_3(conn: &Connection) -> Result<()> {
            conn.execute_batch("CREATE TABLE IF NOT EXISTS synth_v3 (y INTEGER);")?;
            Ok(())
        }
        let migrations = &[
            Migration {
                from: 1,
                to: 2,
                summary: "v2 table",
                apply: m1_to_2,
            },
            Migration {
                from: 2,
                to: 3,
                summary: "v3 table",
                apply: m2_to_3,
            },
        ];
        let applied = apply_migrations(&conn, migrations, 1, 3).unwrap();
        assert_eq!(applied.len(), 2);
        assert_eq!(read_schema_version(&conn).unwrap(), Some(3));
        // Both migrated tables exist.
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('synth_v2','synth_v3')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn apply_migrations_errors_when_no_path_exists() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        set_schema_version(&conn, 1).unwrap();
        let err = apply_migrations(&conn, &[], 1, 2).unwrap_err();
        assert!(
            matches!(err, DatumError::IncompatibleSchema(_)),
            "expected IncompatibleSchema for missing migration path, got {err:?}"
        );
    }

    #[test]
    fn migrate_advances_schema_version() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        set_schema_version(&conn, 1).unwrap();

        fn m1_to_2(c: &Connection) -> Result<()> {
            c.execute_batch("CREATE TABLE IF NOT EXISTS t2 (x INTEGER);")?;
            Ok(())
        }
        fn m2_to_3(c: &Connection) -> Result<()> {
            c.execute_batch("CREATE TABLE IF NOT EXISTS t3 (x INTEGER);")?;
            Ok(())
        }
        let migrations = &[
            Migration {
                from: 1,
                to: 2,
                summary: "add t2",
                apply: m1_to_2,
            },
            Migration {
                from: 2,
                to: 3,
                summary: "add t3",
                apply: m2_to_3,
            },
        ];

        apply_migrations(&conn, migrations, 1, 3).unwrap();
        assert_eq!(read_schema_version(&conn).unwrap(), Some(3));
    }

    #[test]
    fn migrate_runs_pending_only() {
        // DB already at v2; only v2→v3 and v3→v4 should run. The v1→v2 step
        // must NOT be re-executed.
        fn m1_to_2(c: &Connection) -> Result<()> {
            c.execute_batch("CREATE TABLE IF NOT EXISTS ran_1_2 (x INTEGER);")?;
            Ok(())
        }
        fn m2_to_3(c: &Connection) -> Result<()> {
            c.execute_batch("CREATE TABLE IF NOT EXISTS ran_2_3 (x INTEGER);")?;
            Ok(())
        }
        fn m3_to_4(c: &Connection) -> Result<()> {
            c.execute_batch("CREATE TABLE IF NOT EXISTS ran_3_4 (x INTEGER);")?;
            Ok(())
        }
        let migrations = &[
            Migration {
                from: 1,
                to: 2,
                summary: "m12",
                apply: m1_to_2,
            },
            Migration {
                from: 2,
                to: 3,
                summary: "m23",
                apply: m2_to_3,
            },
            Migration {
                from: 3,
                to: 4,
                summary: "m34",
                apply: m3_to_4,
            },
        ];

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        set_schema_version(&conn, 2).unwrap();

        let applied = apply_migrations(&conn, migrations, 2, 4).unwrap();
        assert_eq!(applied.len(), 2, "only two steps should run");
        assert_eq!(read_schema_version(&conn).unwrap(), Some(4));

        fn table_exists(c: &Connection, name: &str) -> bool {
            c.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![name],
                |row| row.get::<_, i64>(0).map(|n| n > 0),
            )
            .unwrap_or(false)
        }
        assert!(
            !table_exists(&conn, "ran_1_2"),
            "v1→v2 must NOT have re-run"
        );
        assert!(table_exists(&conn, "ran_2_3"), "v2→v3 should have run");
        assert!(table_exists(&conn, "ran_3_4"), "v3→v4 should have run");
    }

    #[test]
    fn migrate_is_idempotent_on_already_current_db() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        set_schema_version(&conn, CURRENT_SCHEMA_VERSION).unwrap();

        let first = migrate_to_current(&conn).unwrap();
        assert!(first.is_empty(), "no migrations should run on a current DB");
        assert_eq!(
            read_schema_version(&conn).unwrap(),
            Some(CURRENT_SCHEMA_VERSION)
        );

        let second = migrate_to_current(&conn).unwrap();
        assert!(second.is_empty(), "second run remains a no-op");
        assert_eq!(
            read_schema_version(&conn).unwrap(),
            Some(CURRENT_SCHEMA_VERSION)
        );
    }

    #[test]
    fn force_recreate_resets_schema_version_to_current() {
        let db = Db::open_in_memory().unwrap();
        // Seed _meta.version to something wrong, then force-recreate.
        set_schema_version(db.conn(), 99).unwrap();
        db.force_recreate_schema().unwrap();
        assert_eq!(
            read_schema_version(db.conn()).unwrap(),
            Some(CURRENT_SCHEMA_VERSION)
        );
    }

    /// da-2sr: v8→v9 migration converts `field_specs` rows with
    /// `ignored = true` into a switch IGNORED entry, and leaves
    /// `ignored = false` rows alone.
    #[test]
    fn m8_to_9_writes_ignored_switch_for_each_ignored_spec() {
        let db = Db::open_in_memory().unwrap();

        // Roll back to v8 so the migration step is what we exercise.
        set_schema_version(db.conn(), 8).unwrap();
        // Seed two specs at the v8 shape (the JSON keeps `tracked`/`ignored`).
        db.conn().execute(
            "INSERT INTO field_specs (field, spec_json) VALUES (?1, ?2)",
            rusqlite::params![
                "youtube",
                r#"{"field":"youtube","default_verb":"occur","subfields":{},"tracked":true,"ignored":true}"#,
            ],
        ).unwrap();
        db.conn().execute(
            "INSERT INTO field_specs (field, spec_json) VALUES (?1, ?2)",
            rusqlite::params![
                "mood",
                r#"{"field":"mood","default_verb":"switch","subfields":{},"tracked":true,"ignored":false}"#,
            ],
        ).unwrap();

        // Run the v8→v9 step.
        migrate_to_current(db.conn()).unwrap();
        assert_eq!(
            read_schema_version(db.conn()).unwrap(),
            Some(CURRENT_SCHEMA_VERSION)
        );

        // youtube has the IGNORED sentinel; mood does not.
        let now = chrono::Utc::now();
        let yt_state = db.state_at("youtube", now).unwrap();
        assert_eq!(
            yt_state,
            Some(serde_json::Value::String("IGNORED".to_string())),
            "ignored=true spec must be converted to switch IGNORED",
        );
        let mood_state = db.state_at("mood", now).unwrap();
        assert_eq!(mood_state, None, "ignored=false spec must NOT be migrated");
    }

    /// da-tsw: v11→v12 relocates `_import` markers out of user-payload `data`
    /// into the new `extra_metadata` column, recomputing `content_hash` so
    /// re-imports of the same source dedup against the migrated row.
    #[test]
    fn m11_to_12_relocates_import_marker_and_recomputes_hash() {
        // Open a fresh DB at the current schema, then roll the column back
        // and seed a row in the legacy shape (`_import` inside `data`).
        let db = Db::open_in_memory().unwrap();

        // Seed a row whose `data` mixes user payload (`dur`) with the
        // system marker (`_import`) — exactly what pre-da-tsw migrate
        // import wrote.
        let legacy_data = serde_json::json!({
            "dur": "PT30M",
            "_import": {
                "source": "datum-v2-couchdb",
                "backup_time": "2025-02-02T12:26:32.488Z",
                "original_id": "exercise:2025-01-08T08:00:00.000Z"
            }
        })
        .to_string();
        let legacy_data_only_marker = serde_json::json!({
            "_import": {
                "source": "datum-v2-couchdb",
                "backup_time": "2025-02-02T12:26:32.488Z",
                "original_id": "brushteeth:2025-01-08T23:53:08.259Z"
            }
        })
        .to_string();

        // Wipe extra_metadata so the row looks pre-v12 — the column
        // already exists at the current schema; we just leave it NULL.
        db.conn()
            .execute(
                r#"INSERT INTO entries (
                hid, slug, content_hash, field, entry_shape, intent,
                value, data, unit,
                start_time, start_time_offset, start_time_tz,
                start_time_precision, start_time_precision_unit,
                end_time, end_time_offset, end_time_tz,
                end_time_precision, end_time_precision_unit,
                lat, lon, location,
                create_time, modify_time, device_id,
                deleted_at, id_structure, field_structure,
                raw_input, alias_chain, parser_version,
                user_asserted_last_state, extra_metadata
            ) VALUES (
                'h1blockrowfix', 'exercise:2025-01-08T08:00:00.000Z',
                'oldhash-block', 'exercise', 'block', NULL,
                NULL, ?1, NULL,
                '2025-01-08T08:00:00+00:00', 0, 'UTC', 0, 'exact',
                '2025-01-08T08:30:00+00:00', 0, 'UTC', 0, 'exact',
                NULL, NULL, NULL,
                '2025-01-08T08:00:00.500+00:00', '2025-01-08T08:00:00.500+00:00', 'test',
                NULL, NULL, NULL,
                '', NULL, 0,
                NULL, NULL
            )"#,
                rusqlite::params![legacy_data],
            )
            .unwrap();
        db.conn()
            .execute(
                r#"INSERT INTO entries (
                hid, slug, content_hash, field, entry_shape, intent,
                value, data, unit,
                start_time, start_time_offset, start_time_tz,
                start_time_precision, start_time_precision_unit,
                end_time, end_time_offset, end_time_tz,
                end_time_precision, end_time_precision_unit,
                lat, lon, location,
                create_time, modify_time, device_id,
                deleted_at, id_structure, field_structure,
                raw_input, alias_chain, parser_version,
                user_asserted_last_state, extra_metadata
            ) VALUES (
                'h2pointrowok1', 'brushteeth:2025-01-08T23:53:08.259Z',
                'oldhash-point', 'brushteeth', 'point', NULL,
                NULL, ?1, NULL,
                '2025-01-08T23:53:08.259+00:00', 0, 'UTC', 0, 'exact',
                NULL, NULL, NULL, 0, 'exact',
                NULL, NULL, NULL,
                '2025-01-08T23:53:08.569+00:00', '2025-01-08T23:53:08.569+00:00', 'test',
                NULL, NULL, NULL,
                '', NULL, 0,
                NULL, NULL
            )"#,
                rusqlite::params![legacy_data_only_marker],
            )
            .unwrap();

        // Roll back the version stamp to v11 so migrate_to_current
        // re-runs the v11→v12 step against the seeded rows.
        set_schema_version(db.conn(), 11).unwrap();
        migrate_to_current(db.conn()).unwrap();
        assert_eq!(
            read_schema_version(db.conn()).unwrap(),
            Some(CURRENT_SCHEMA_VERSION)
        );

        // Block row: dur survives in data; _import moved to extra_metadata.
        let (data_str, meta_str): (Option<String>, Option<String>) = db
            .conn()
            .query_row(
                "SELECT data, extra_metadata FROM entries WHERE hid = 'h1blockrowfix'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let data: serde_json::Value = serde_json::from_str(data_str.as_deref().unwrap()).unwrap();
        let meta: serde_json::Value = serde_json::from_str(meta_str.as_deref().unwrap()).unwrap();
        assert_eq!(data["dur"], serde_json::json!("PT30M"));
        assert!(
            data.get("_import").is_none(),
            "_import must be stripped from data",
        );
        assert_eq!(
            meta["_import"]["source"],
            serde_json::json!("datum-v2-couchdb"),
        );

        // Point row: data was only the marker → relocated then nulled.
        let (point_data_str, point_meta_str): (Option<String>, Option<String>) = db
            .conn()
            .query_row(
                "SELECT data, extra_metadata FROM entries WHERE hid = 'h2pointrowok1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(
            point_data_str.is_none(),
            "marker-only data must NULL after relocation, got {point_data_str:?}",
        );
        let point_meta: serde_json::Value =
            serde_json::from_str(point_meta_str.as_deref().unwrap()).unwrap();
        assert_eq!(
            point_meta["_import"]["original_id"],
            serde_json::json!("brushteeth:2025-01-08T23:53:08.259Z"),
        );

        // content_hash recomputed (no longer matches the seeded `oldhash-*`).
        let new_hash: String = db
            .conn()
            .query_row(
                "SELECT content_hash FROM entries WHERE hid = 'h1blockrowfix'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(
            new_hash, "oldhash-block",
            "content_hash must be recomputed after data changes",
        );
    }

    /// da-a7g: v17→v18 adds the `tags` column with default `'[]'` so existing
    /// rows stay readable as having no tags.
    #[test]
    fn m17_to_18_adds_tags_column_with_default_empty_array() {
        // Build a v17-shape connection from scratch so the test exercises
        // the migration end-to-end. We avoid `ALTER TABLE DROP COLUMN` here
        // because it leaves a generated-column-style residue on some SQLite
        // builds when other generated columns reference table-wide state.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE field_specs (
                field      TEXT NOT NULL PRIMARY KEY,
                spec_json  TEXT NOT NULL,
                kit_id     TEXT
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO field_specs (field, spec_json) VALUES (?1, ?2)",
            rusqlite::params![
                "legacy",
                r#"{"field":"legacy","default_verb":"occur","subfields":{}}"#,
            ],
        )
        .unwrap();
        set_schema_version(&conn, 17).unwrap();

        // The full migration runner walks the registry; only v17→v18 is
        // pending in this synthetic conn, so this exercises the new step.
        let applied = migrate_to_current(&conn).unwrap();
        assert!(applied.iter().any(|s| s.contains("v17→v18")));
        assert_eq!(
            read_schema_version(&conn).unwrap(),
            Some(CURRENT_SCHEMA_VERSION),
        );

        let tags: String = conn
            .query_row(
                "SELECT tags FROM field_specs WHERE field = 'legacy'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tags, "[]", "default tags JSON must be '[]' for legacy rows");

        // Re-running migrate_to_current is a no-op and doesn't error on the
        // already-present column.
        let again = migrate_to_current(&conn).unwrap();
        assert!(again.is_empty());
    }

    /// da-2zd: v18→v19 adds the nullable `render_json` column. Existing
    /// rows stay readable as having no render override (NULL = visible
    /// everywhere).
    #[test]
    fn m18_to_19_adds_render_json_column_nullable() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        // Build a v18-shape field_specs from scratch (no render_json column).
        conn.execute_batch(
            "CREATE TABLE field_specs (
                field      TEXT NOT NULL PRIMARY KEY,
                spec_json  TEXT NOT NULL,
                kit_id     TEXT,
                tags       TEXT NOT NULL DEFAULT '[]'
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO field_specs (field, spec_json) VALUES (?1, ?2)",
            rusqlite::params![
                "legacy",
                r#"{"field":"legacy","default_verb":"occur","subfields":{}}"#,
            ],
        )
        .unwrap();
        set_schema_version(&conn, 18).unwrap();

        let applied = migrate_to_current(&conn).unwrap();
        assert!(applied.iter().any(|s| s.contains("v18→v19")));
        assert_eq!(
            read_schema_version(&conn).unwrap(),
            Some(CURRENT_SCHEMA_VERSION),
        );

        // Legacy row's render_json must default to NULL — visible everywhere.
        let render_json: Option<String> = conn
            .query_row(
                "SELECT render_json FROM field_specs WHERE field = 'legacy'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(render_json.is_none(), "expected NULL, got {render_json:?}");

        // Idempotent — re-running is a no-op.
        let again = migrate_to_current(&conn).unwrap();
        assert!(again.is_empty());
    }
}
