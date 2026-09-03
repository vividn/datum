//! Schema version tracking and forward migrations.
//!
//! Every DB carries a `_meta.schema_version` row that this binary compares
//! against [`CURRENT_SCHEMA_VERSION`]. Forward-compatible migrations
//! (ADD COLUMN, CREATE INDEX IF NOT EXISTS, CREATE TABLE IF NOT EXISTS)
//! run silently on open; anything destructive (column removal, PK change)
//! must go through `datum db purge` or a proper migration entry here.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{DatumError, Result};

/// The schema version this binary writes.
///
/// Bump this whenever `schema.sql` changes AND append a [`Migration`] that
/// lifts the previous version to the new one. The invariant is that a DB
/// at any version in `[1, CURRENT_SCHEMA_VERSION]` can be migrated to the
/// current shape by walking [`MIGRATIONS`] in order.
pub const CURRENT_SCHEMA_VERSION: u32 = 34;

/// A single forward migration between two adjacent schema versions.
///
/// Migrations run inside a transaction opened by the caller. `apply` must
/// be idempotent against a partially-applied state where possible — in
/// practice this means using `CREATE … IF NOT EXISTS` / `ALTER … ADD
/// COLUMN` patterns so interrupted runs can be retried safely.
///
/// `down` reverses `apply` to restore the v=`from` schema shape. It runs
/// inside a transaction opened by the caller. Where the forward migration
/// is data-destructive (e.g. v2→v3 replaces the entries table), the
/// docstring on `down` MUST document the data-loss; the down may either
/// proceed with documented loss or return [`DatumError::NotReversible`]
/// when it would silently destroy production data.
pub struct Migration {
    pub from: u32,
    pub to: u32,
    pub summary: &'static str,
    pub apply: fn(&Connection) -> Result<()>,
    pub down: fn(&Connection) -> Result<()>,
}

/// Ordered forward migrations. Append-only — never reorder, rename, or
/// delete entries here.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        from: 1,
        to: 2,
        summary: "sync_state: replace (device_id,last_seq,updated_at) with (db_id,push_seq,pull_seq,last_push_at,last_pull_at)",
        apply: m1_to_2_sync_state,
        down: down_2_to_1_sync_state,
    },
    Migration {
        from: 2,
        to: 3,
        summary: "data model v2: entries table replaced — entry_shape, intent, precision fields, timestamp rename, hid-PK, generated verb/dur_seconds, raw_input/alias_chain/parser_version",
        apply: m2_to_3_entries_v2,
        down: down_3_to_2_entries_v2,
    },
    Migration {
        from: 3,
        to: 4,
        summary: "entry_history: append-only modification trail (entry_hid, version, snapshot, change_kind)",
        apply: m3_to_4_entry_history,
        down: down_4_to_3_entry_history,
    },
    Migration {
        from: 4,
        to: 5,
        summary: "sync_conflicts: non-blocking overlap / double-open / orphan-end queue (local_validator + sync_pull)",
        apply: m4_to_5_sync_conflicts,
        down: down_5_to_4_sync_conflicts,
    },
    Migration {
        from: 5,
        to: 6,
        summary: "entries_live_field_start: partial index for SQL-backed views (deleted_at IS NULL AND start_time IS NOT NULL)",
        apply: m5_to_6_live_field_index,
        down: down_6_to_5_live_field_index,
    },
    Migration {
        from: 6,
        to: 7,
        summary: "sync_state: add my_push_seq column (per-device AAD seq counter for C4c push)",
        apply: m6_to_7_sync_state_my_push_seq,
        down: down_7_to_6_sync_state_my_push_seq,
    },
    Migration {
        from: 7,
        to: 8,
        summary: "entries: add user_asserted_last_state column (S6 — explicit -L assertion value)",
        apply: m7_to_8_entries_user_asserted_last_state,
        down: down_8_to_7_entries_user_asserted_last_state,
    },
    Migration {
        from: 8,
        to: 9,
        summary: "track/ignore unification (da-2sr): convert field_specs.ignored=true rows into switch IGNORED entries; spec retains schema only",
        apply: m8_to_9_track_ignore_unification,
        down: down_9_to_8_track_ignore_unification,
    },
    Migration {
        from: 9,
        to: 10,
        summary: "entries_hybrid_time: expression index on COALESCE(start_time, modify_time) for bare-time tail/head (da-7iu)",
        apply: m9_to_10_hybrid_time_index,
        down: down_10_to_9_hybrid_time_index,
    },
    Migration {
        from: 10,
        to: 11,
        summary: "last_docs: singleton pointer to the hids of the most-recently-accessed batch (da-e65)",
        apply: m10_to_11_last_docs,
        down: down_11_to_10_last_docs,
    },
    Migration {
        from: 11,
        to: 12,
        summary: "entries: add extra_metadata column for system provenance; relocate datum-migrate `_import` marker out of data (da-tsw)",
        apply: m11_to_12_extra_metadata,
        down: down_12_to_11_extra_metadata,
    },
    Migration {
        from: 12,
        to: 13,
        summary: "field_specs: add kit_id column for kit-namespaced field specs; index on kit_id (da-d4t.1)",
        apply: m12_to_13_field_specs_kit_id,
        down: down_13_to_12_field_specs_kit_id,
    },
    Migration {
        from: 13,
        to: 14,
        summary: "views: named SQL views table (kit-scoped); powers `datum view <name>` (da-d4t.4)",
        apply: m13_to_14_views_table,
        down: down_14_to_13_views_table,
    },
    Migration {
        from: 14,
        to: 15,
        summary: "kit_indices: track CREATE INDEX statements installed by kits so uninstall can DROP INDEX cleanly (da-d4t.5)",
        apply: m14_to_15_kit_indices,
        down: down_15_to_14_kit_indices,
    },
    Migration {
        from: 15,
        to: 16,
        summary: "aliases: add kit_id column for kit-namespaced [[alias]] rows; index on kit_id (da-d4t.3)",
        apply: m15_to_16_aliases_kit_id,
        down: down_16_to_15_aliases_kit_id,
    },
    Migration {
        from: 16,
        to: 17,
        summary: "kit_commands: kit-defined Level 1/2 commands (alias expansion + view+presentation) (da-d4t.6)",
        apply: m16_to_17_kit_commands_table,
        down: down_17_to_16_kit_commands_table,
    },
    Migration {
        from: 17,
        to: 18,
        summary: "field_specs: add tags TEXT column (JSON array, default '[]') for kit/view tag filtering (da-a7g)",
        apply: m17_to_18_field_specs_tags,
        down: down_18_to_17_field_specs_tags,
    },
    Migration {
        from: 18,
        to: 19,
        summary: "field_specs: add render_json TEXT column (NULL = visible everywhere) for per-surface render visibility (da-2zd)",
        apply: m18_to_19_field_specs_render_json,
        down: down_19_to_18_field_specs_render_json,
    },
    Migration {
        from: 19,
        to: 20,
        summary: "entries: time-shape pivot — drop start_time*/end_time*/entry_shape/intent/value; add occur_time*/state/dur/following_state; verb is now stored, not generated (da-5ns.1)",
        apply: m19_to_20_time_shape_pivot,
        down: down_20_to_19_time_shape_pivot,
    },
    Migration {
        from: 20,
        to: 21,
        summary: "aliases: drop materialized derived alias rows + the derived_from_field column. Derived aliases now live in spec_json and are computed lazily by SpecAliasIndex (da-8y2q.5)",
        apply: m20_to_21_drop_derived_from_field,
        down: down_21_to_20_drop_derived_from_field,
    },
    Migration {
        from: 21,
        to: 22,
        summary: "entries: rewrite legacy verb strings ('occur'/'add'→'point'; 'start'/'stop'→'switch' with state canonicalization). field_specs: legacy default_verb names already match Action's serde so no JSON rewrite needed (da-8y2q.2)",
        apply: m21_to_22_verb_canonicalize,
        down: down_22_to_21_verb_canonicalize,
    },
    Migration {
        from: 22,
        to: 23,
        summary: "entries: drop vestigial `unit` column (da-qnn9); add nullable `dur_precision` column (da-df27) — ISO-8601 duration string capturing the granularity at which `dur` is meaningful",
        apply: m22_to_23_unit_drop_dur_precision_add,
        down: down_23_to_22_unit_drop_dur_precision_add,
    },
    Migration {
        from: 23,
        to: 24,
        summary: "entries: partial indexes on id_structure / field_structure (NOT NULL) — drift-detection support for `dm check slugs` (da-b423)",
        apply: m23_to_24_id_field_structure_indexes,
        down: down_24_to_23_id_field_structure_indexes,
    },
    Migration {
        from: 24,
        to: 25,
        summary: "entries: partial UNIQUE on (field, occur_time) WHERE occur_time IS NOT NULL AND deleted_at IS NULL — block same-instant state-changes (da-ga8b)",
        apply: m24_to_25_field_occurtime_unique,
        down: down_25_to_24_field_occurtime_unique,
    },
    Migration {
        from: 25,
        to: 26,
        summary: "rename sync_conflicts → integrity_warnings (table, indexes, conflict_kind column → kind); these are local data-integrity findings, not cross-device sync conflicts (da-u2do)",
        apply: m25_to_26_rename_integrity_warnings,
        down: down_26_to_25_rename_integrity_warnings,
    },
    Migration {
        from: 26,
        to: 27,
        summary: "sync_state: add cluster_id + cluster_generation columns — per-DB cluster registration for the multi-DB sync control plane (da-eu7o)",
        apply: m26_to_27_sync_state_cluster_columns,
        down: down_27_to_26_sync_state_cluster_columns,
    },
    Migration {
        from: 27,
        to: 28,
        summary: "integrity_warnings: drop stored same_state_repeat rows — `repeated_state` is now computed live from entries (da-acft)",
        apply: m27_to_28_drop_stored_same_state_repeat,
        down: down_28_to_27_drop_stored_same_state_repeat,
    },
    Migration {
        from: 28,
        to: 29,
        summary: "entries: drop NOT NULL on occur_time — a bare `add` (no -t) is timeless and stores NULL; the absence of a time is what distinguishes `add` from `occur` (da-lxyw)",
        apply: m28_to_29_occur_time_nullable,
        down: down_29_to_28_occur_time_nullable,
    },
    Migration {
        from: 29,
        to: 30,
        summary: "entries: add indexed modify_time_ms (epoch-ms of modify_time) so sync push filters in SQL instead of scanning every row (da-x3da)",
        apply: m29_to_30_modify_time_ms_index,
        down: down_30_to_29_modify_time_ms_index,
    },
    Migration {
        from: 30,
        to: 31,
        summary: "sync_state: rename push_seq -> push_mark, my_push_seq -> device_seq for clarity (da-xuiw)",
        apply: m30_to_31_sync_state_rename_seq_columns,
        down: down_31_to_30_sync_state_rename_seq_columns,
    },
    Migration {
        from: 31,
        to: 32,
        summary: "entries: drop NOT NULL DEFAULT '' from raw_input; existing empty strings normalize to NULL at the row-mapper boundary (polecat §5 #11)",
        apply: m31_to_32_raw_input_nullable,
        down: down_32_to_31_raw_input_nullable,
    },
    Migration {
        from: 32,
        to: 33,
        summary: "entries: recompute every content_hash — the hash now covers `following_state` and deleted-ness, so two semantically different entries no longer hash alike (da-4vyw)",
        apply: m32_to_33_content_hash_v33,
        down: down_33_to_32_content_hash_v33,
    },
    Migration {
        from: 33,
        to: 34,
        summary: "entries: rename occur_time_precision -> occur_time_uncertainty and dur_precision -> dur_uncertainty — one name for one concept, now carried on DatumTime/Dur rather than in loose sibling fields (da-b8rs)",
        apply: m33_to_34_precision_to_uncertainty,
        down: down_34_to_33_precision_to_uncertainty,
    },
];

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
fn m33_to_34_precision_to_uncertainty(conn: &Connection) -> Result<()> {
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

/// Reverse 34→33 — restore the pre-da-b8rs column names. Lossless.
fn down_34_to_33_precision_to_uncertainty(conn: &Connection) -> Result<()> {
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

/// SQL expression deriving epoch-ms from the RFC3339 `modify_time` string.
/// Kept in one place so the generated column (migration + schema.sql) and any
/// future reader agree on the metric. `unixepoch(.., 'subsec')` returns
/// fractional seconds; ×1000 + truncating CAST yields milliseconds. The push
/// pipeline uses this metric for BOTH the watermark filter and the watermark
/// it advances to, so the (sub-millisecond) float deviation from chrono's
/// `timestamp_millis` is self-consistent and cannot skip a row.
const MODIFY_TIME_MS_EXPR: &str = "CAST(unixepoch(modify_time, 'subsec') * 1000 AS INTEGER)";

/// da-x3da: add a `modify_time_ms` virtual generated column (epoch-ms of the
/// RFC3339 `modify_time`) plus an index, so `select_entries_since` can push
/// its watermark filter into SQL (`WHERE modify_time_ms > ?`) and stop
/// scanning the whole `entries` table on every push.
///
/// VIRTUAL (not STORED) because `ALTER TABLE ADD COLUMN` only permits virtual
/// generated columns; the index materializes the computed values so the range
/// filter still uses a b-tree seek. Additive and idempotent.
fn m29_to_30_modify_time_ms_index(conn: &Connection) -> Result<()> {
    // `pragma_table_info` omits virtual generated columns — probe with
    // `pragma_table_xinfo`, which lists them.
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_xinfo('entries') WHERE name = 'modify_time_ms'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        conn.execute_batch(&format!(
            "ALTER TABLE entries ADD COLUMN modify_time_ms INTEGER \
                GENERATED ALWAYS AS ({MODIFY_TIME_MS_EXPR}) VIRTUAL;"
        ))?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS entries_modify_time_ms ON entries (modify_time_ms);",
    )?;
    Ok(())
}

/// Reverse 30→29 — drop the index and the generated column. The index must
/// go first: SQLite refuses to drop a column an index still references.
fn down_30_to_29_modify_time_ms_index(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_modify_time_ms;")?;
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_xinfo('entries') WHERE name = 'modify_time_ms'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if has_col {
        conn.execute_batch("ALTER TABLE entries DROP COLUMN modify_time_ms;")?;
    }
    Ok(())
}

/// da-xuiw: rename `sync_state`'s two muddy "push seq" columns for clarity.
/// `push_seq` (the `modified_at` packing watermark) -> `push_mark`, and
/// `my_push_seq` (the per-device AAD push counter) -> `device_seq`. Pure
/// rename, no data change; `pull_seq` already reads cleanly and is left as
/// is. Guarded per-column so a re-run — or a fresh DB already on the new
/// names — is a no-op.
fn m30_to_31_sync_state_rename_seq_columns(conn: &Connection) -> Result<()> {
    if has_column(conn, "sync_state", "push_seq")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN push_seq TO push_mark;")?;
    }
    if has_column(conn, "sync_state", "my_push_seq")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN my_push_seq TO device_seq;")?;
    }
    Ok(())
}

/// Reverse 31→30 — restore the pre-da-xuiw column names.
fn down_31_to_30_sync_state_rename_seq_columns(conn: &Connection) -> Result<()> {
    if has_column(conn, "sync_state", "push_mark")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN push_mark TO push_seq;")?;
    }
    if has_column(conn, "sync_state", "device_seq")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN device_seq TO my_push_seq;")?;
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
fn m31_to_32_raw_input_nullable(conn: &Connection) -> Result<()> {
    rewrite_raw_input_column(conn, "entries", "TEXT NOT NULL DEFAULT ''", "TEXT")?;
    rewrite_raw_input_column(conn, "entry_history", "TEXT NOT NULL", "TEXT")?;
    Ok(())
}

/// Rewrite the stored `CREATE TABLE` SQL for `table_name`, swapping the
/// `raw_input` column definition. Matches the trailing comma so the
/// replacement is anchored: `raw_input <from>,` → `raw_input <to>,`.
///
/// Idempotent: no-op when the target form is already present.
/// Handles both the padded (schema.sql) and the collapsed-whitespace
/// (test fixture) variants.
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
    // Anchor with the trailing comma so `raw_input TEXT NOT NULL ...`
    // doesn't also match the substring `raw_input TEXT`.
    let padded_from = format!("raw_input       {from},");
    let padded_to = format!("raw_input       {to},");
    let tight_from = format!("raw_input {from},");
    let tight_to = format!("raw_input {to},");
    if !current_sql.contains(&padded_from) && !current_sql.contains(&tight_from) {
        return Ok(());
    }
    let new_sql = current_sql
        .replace(&padded_from, &padded_to)
        .replace(&tight_from, &tight_to);
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

/// Reverse 32→31 — restore the `NOT NULL DEFAULT ''` constraint via the
/// same writable_schema mechanism. Existing NULL rows would fail the
/// reapplied constraint, so we UPDATE them to the empty-string
/// sentinel first; the data-loss is minimal (NULL ↔ '' is the
/// transparent mapping the row mapper preserves anyway).
fn down_32_to_31_raw_input_nullable(conn: &Connection) -> Result<()> {
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
fn m32_to_33_content_hash_v33(conn: &Connection) -> Result<()> {
    rehash_all_entries(conn, |entry| entry.expected_content_hash())
}

/// Reverse 33→32 — recompute every hash under the pre-da-4vyw definition
/// (`field · occur_time · dur · state · data`, `\x00`-separated), which is
/// reproduced locally here because the live one has moved on.
fn down_33_to_32_content_hash_v33(conn: &Connection) -> Result<()> {
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
fn m19_to_20_time_shape_pivot(conn: &Connection) -> Result<()> {
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

/// da-8y2q.5: drop the materialized-derived-alias scheme. The
/// `aliases.derived_from_field` column tagged rows that mirrored a
/// FieldSpec's bare-field / terminal-end / per-state aliases, but the
/// design wrote ~10 alias rows per spec save while reads always hit the
/// same `aliases` table. Cross-spec state-name collisions (two specs
/// both declaring state `done`) silently raced on save order.
///
/// Post-v21 the derived aliases live exclusively in
/// `field_specs.spec_json` and are surfaced on read via
/// [`crate::db::Db::spec_alias_index`]. The forward step deletes any
/// derived-alias row (those whose `derived_from_field IS NOT NULL`)
/// before rebuilding the table without the column.
///
/// **Pre-v1 hard-cut.** No production data exists; the rebuild matches
/// the pattern used by other table-shape migrations in this file.
fn m20_to_21_drop_derived_from_field(conn: &Connection) -> Result<()> {
    // Synthetic test setups (and ancient pre-a0 DBs missing the table)
    // can reach this step without an `aliases` table — the post-migration
    // schema.sql pass will (re)create it in the v=21 shape, so a no-op is
    // safe.
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name = 'aliases'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !table_exists {
        return Ok(());
    }
    let has_col: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('aliases') \
             WHERE name = 'derived_from_field'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_col {
        return Ok(());
    }
    conn.execute_batch("DELETE FROM aliases WHERE derived_from_field IS NOT NULL;")?;
    rebuild_aliases(conn, /* with_derived_from_field = */ false)
}

/// Reverse v20→v21: re-add the `derived_from_field` column to
/// `aliases` via table rebuild. Surviving rows have `derived_from_field
/// = NULL` — the v21 forward step deleted derived-alias rows, so the
/// information needed to re-populate the column does not exist on disk.
/// Pre-v1 hard-cut: this down step exists for the chain-test harness
/// only.
fn down_21_to_20_drop_derived_from_field(conn: &Connection) -> Result<()> {
    rebuild_aliases(conn, /* with_derived_from_field = */ true)
}

/// da-8y2q.2: rewrite legacy verb strings on stored rows.
///
/// The v=20 schema's `verb` column carries one of the canonical five
/// storage verbs (Point/Switch/Block/BlockSwitch/PointSwitch), but the
/// pre-da-8y2q `Verb` enum also accepted the legacy intent values
/// (`occur`/`start`/`stop`/`add`) and earlier write paths could leak
/// those into the column. After the legacy variants were dropped, any
/// row still carrying one of those strings would fail to deserialize.
///
/// Rewrite map (preserving the storage truth table — see
/// `crates/datum-core/src/verb.rs`):
/// - `occur`/`add` → `point` (no state change, point shape).
/// - `start` with NULL state → `switch` + `state='true'` (state=true,
///   dur=NULL implies Switch by the truth table).
/// - `start` with a non-NULL state → `switch` (state preserved).
/// - `stop` with NULL state → `switch` + `state='false'`.
/// - `stop` with a non-NULL state → `switch`.
///
/// `field_specs.spec_json` is unchanged: `Action`'s serde names match
/// the legacy `default_verb` strings (`occur`/`start`/`switch`/`add`),
/// so on-disk specs round-trip without rewriting.
///
/// **Render-layer side effect (Mayor decision, da-8y2q.2 escalation):**
/// the legacy `Verb::Add` storage variant drove the `¢`/NON_OCCUR glyph
/// in `render::action::format_state_group`. After the collapse into
/// `Verb::Point`, time-stamped legacy `add` rows render as dots (matching
/// the rest of the Point family); only timeless adds (no `occur_time`)
/// still render as NON_OCCUR. This is an intentional pre-v1 visual
/// regression accepted as the cost of dropping intent-as-storage; see
/// the bead's escalation thread for the full rationale.
fn m21_to_22_verb_canonicalize(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "UPDATE entries SET verb = 'point'
            WHERE verb IN ('occur', 'add');
         UPDATE entries SET verb = 'switch', state = 'true'
            WHERE verb = 'start' AND state IS NULL;
         UPDATE entries SET verb = 'switch'
            WHERE verb = 'start';
         UPDATE entries SET verb = 'switch', state = 'false'
            WHERE verb = 'stop' AND state IS NULL;
         UPDATE entries SET verb = 'switch'
            WHERE verb = 'stop';",
    )?;
    Ok(())
}

/// Reverse of [`m21_to_22_verb_canonicalize`].
///
/// **Lossy.** The forward migration collapses two intent verbs into one
/// storage verb (`switch`), so the reverse cannot recover the original
/// `start`/`stop` distinction without re-deriving from `state`. We do a
/// best-effort reconstruction from the canonicalized state value, but
/// any `switch` row whose state is something other than the literal
/// JSON `true`/`false` is left as `switch` (not legacy). Pre-v1 hard
/// cut allows this asymmetry.
fn down_22_to_21_verb_canonicalize(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "UPDATE entries SET verb = 'start', state = NULL
            WHERE verb = 'switch' AND state = 'true' AND dur IS NULL AND following_state IS NULL;
         UPDATE entries SET verb = 'stop', state = NULL
            WHERE verb = 'switch' AND state = 'false' AND dur IS NULL AND following_state IS NULL;
         UPDATE entries SET verb = 'occur'
            WHERE verb = 'point';",
    )?;
    Ok(())
}

/// da-qnn9 + da-df27: drop the vestigial `unit` column and add the new
/// nullable `dur_precision` column to `entries`. Bundled into one
/// migration because both are schema column changes on the same table
/// and the project's pre-v1 hard-cut policy permits a single step.
///
/// SQLite's `ALTER TABLE … DROP COLUMN` re-parses the stored CREATE
/// statement and trips on the inline `--` comments embedded in
/// schema.sql, so the migration uses the standard 12-step rebuild
/// pattern (create new table without `unit` and with `dur_precision`,
/// copy stored columns over, drop old, rename). Rows are preserved.
///
/// Idempotent: if the table already lacks `unit` and already has
/// `dur_precision`, the migration is a no-op.
fn m22_to_23_unit_drop_dur_precision_add(conn: &Connection) -> Result<()> {
    let has_unit = has_column(conn, "entries", "unit")?;
    let has_dur_precision = has_column(conn, "entries", "dur_precision")?;
    if !has_unit && has_dur_precision {
        return Ok(());
    }
    rebuild_entries_v23(
        conn, /* with_unit = */ false, /* with_dur_precision = */ true,
    )
}

/// Reverse v22→v23: re-add the `unit` column and drop `dur_precision`
/// via table rebuild. Surviving rows have `unit = NULL` — the forward
/// step does not preserve the dropped column's data because the column
/// is vestigial. Pre-v1 hard-cut; this down step exists for the
/// chain-test harness.
fn down_23_to_22_unit_drop_dur_precision_add(conn: &Connection) -> Result<()> {
    rebuild_entries_v23(
        conn, /* with_unit = */ true, /* with_dur_precision = */ false,
    )
}

/// da-b423: add two partial indexes to `entries` to back drift-detection
/// scans by `dm check slugs`:
///
/// - `entries_id_structure`    — `(hid) WHERE id_structure IS NOT NULL`
/// - `entries_field_structure` — `(hid) WHERE field_structure IS NOT NULL`
///
/// Selectivity is asymmetric: almost every row carries `id_structure`
/// (default `%occur_time%` / `%create_time%` template), while
/// `field_structure` is set only when the user writes a templated field
/// like `word__%srcLang%`. Two separate partial indexes let the planner
/// pick a tiny one for `field_structure` scans without paying for the
/// full-table version.
///
/// Additive + idempotent — `CREATE INDEX IF NOT EXISTS` lets a
/// half-applied or already-applied DB run safely.
fn m23_to_24_id_field_structure_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS entries_id_structure \
            ON entries (hid) WHERE id_structure IS NOT NULL;
         CREATE INDEX IF NOT EXISTS entries_field_structure \
            ON entries (hid) WHERE field_structure IS NOT NULL;",
    )?;
    Ok(())
}

/// Reverse of [`m23_to_24_id_field_structure_indexes`]. Index-only
/// changes; no data is touched.
fn down_24_to_23_id_field_structure_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_id_structure;
         DROP INDEX IF EXISTS entries_field_structure;",
    )?;
    Ok(())
}

/// da-ga8b: add a partial UNIQUE index on `(field, occur_time)` over live
/// rows so two state-changes for the same field at the same exact instant
/// are blocked even when custom id templates produce distinct slugs.
///
/// **Pre-check.** Scans existing live rows for `(field, occur_time)`
/// collisions BEFORE creating the index. If any exist, the migration
/// fails with a diagnostic listing the offending hids so the operator can
/// resolve the dirty data (manually delete or retime one of each pair)
/// and re-run. The pre-check filters tombstones so soft-deleted rows
/// don't block the upgrade.
fn m24_to_25_field_occurtime_unique(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT field, occur_time, GROUP_CONCAT(hid, ',') \
         FROM entries \
         WHERE occur_time IS NOT NULL AND deleted_at IS NULL \
         GROUP BY field, occur_time \
         HAVING COUNT(*) > 1 \
         ORDER BY field, occur_time",
    )?;
    let collisions: Vec<(String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    if !collisions.is_empty() {
        let mut diag = String::from(
            "v24→v25 migration aborted: (field, occur_time) collisions exist in live entries. \
             Resolve the offending rows (delete or retime one of each group) before re-running:",
        );
        for (field, occur_time, hids) in &collisions {
            diag.push_str(&format!(
                "\n  field={field} occur_time={occur_time} hids=[{hids}]"
            ));
        }
        return Err(DatumError::Other(diag));
    }

    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS entries_field_occurtime \
            ON entries (field, occur_time) \
            WHERE occur_time IS NOT NULL AND deleted_at IS NULL;",
    )?;
    Ok(())
}

/// Reverse of [`m24_to_25_field_occurtime_unique`]. Index-only; no data
/// is touched.
fn down_25_to_24_field_occurtime_unique(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_field_occurtime;")?;
    Ok(())
}

/// da-u2do: rename `sync_conflicts` → `integrity_warnings`. These rows
/// describe local data-integrity findings (double-open, orphan-end,
/// overlap, same-state-repeat, last-state-mismatch) — they are NOT
/// cross-device sync conflicts. The legacy name conflated the two
/// concepts; the rename gives integrity warnings their own surface and
/// frees the `sync_conflicts` name for a future true-sync-conflict table.
///
/// Side effects:
/// - rename the table
/// - rename `conflict_kind` column → `kind`
/// - drop old indexes, create new indexes with the new prefix
fn m25_to_26_rename_integrity_warnings(conn: &Connection) -> Result<()> {
    // Some upgrade-path tests stamp the schema forward without creating the
    // earlier `sync_conflicts` table. Treat its absence as "nothing to
    // rename" rather than aborting — the v5 step is what owns table
    // existence, not this rename.
    let has_legacy: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'sync_conflicts'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_legacy {
        return Ok(());
    }
    conn.execute_batch(
        "DROP INDEX IF EXISTS sync_conflicts_hid;
         DROP INDEX IF EXISTS sync_conflicts_slug;
         DROP INDEX IF EXISTS sync_conflicts_pending;
         ALTER TABLE sync_conflicts RENAME TO integrity_warnings;
         ALTER TABLE integrity_warnings RENAME COLUMN conflict_kind TO kind;
         CREATE INDEX IF NOT EXISTS integrity_warnings_hid ON integrity_warnings(hid);
         CREATE INDEX IF NOT EXISTS integrity_warnings_slug ON integrity_warnings(entry_slug);
         CREATE INDEX IF NOT EXISTS integrity_warnings_pending
             ON integrity_warnings(detected_at) WHERE resolution IS NULL;",
    )?;
    Ok(())
}

/// Reverse of [`m25_to_26_rename_integrity_warnings`]. Renames back to
/// the legacy `sync_conflicts` shape so the migration is round-trippable.
fn down_26_to_25_rename_integrity_warnings(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS integrity_warnings_hid;
         DROP INDEX IF EXISTS integrity_warnings_slug;
         DROP INDEX IF EXISTS integrity_warnings_pending;
         ALTER TABLE integrity_warnings RENAME COLUMN kind TO conflict_kind;
         ALTER TABLE integrity_warnings RENAME TO sync_conflicts;
         CREATE INDEX IF NOT EXISTS sync_conflicts_hid ON sync_conflicts(hid);
         CREATE INDEX IF NOT EXISTS sync_conflicts_slug ON sync_conflicts(entry_slug);
         CREATE INDEX IF NOT EXISTS sync_conflicts_pending
             ON sync_conflicts(detected_at) WHERE resolution IS NULL;",
    )?;
    Ok(())
}

/// da-eu7o: add per-DB `cluster_id` (nullable until first sync echoes
/// back the relay's assignment) and `cluster_generation` (default 0)
/// columns to `sync_state`. Both `ALTER TABLE ... ADD COLUMN` steps are
/// natively idempotent via an introspection check so replaying this
/// migration is safe.
fn m26_to_27_sync_state_cluster_columns(conn: &Connection) -> Result<()> {
    // Some upgrade-path tests stamp the schema forward without creating the
    // earlier `sync_state` table. Treat its absence as "nothing to extend" —
    // the v1→v2 step is what owns sync_state existence, not this column add.
    let has_sync_state: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'sync_state'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_sync_state {
        return Ok(());
    }
    let has_cluster_id: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sync_state') WHERE name = 'cluster_id'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_cluster_id {
        conn.execute_batch("ALTER TABLE sync_state ADD COLUMN cluster_id TEXT;")?;
    }
    let has_cluster_gen: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('sync_state') WHERE name = 'cluster_generation'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_cluster_gen {
        conn.execute_batch(
            "ALTER TABLE sync_state ADD COLUMN cluster_generation INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    Ok(())
}

/// da-acft: clear stored `same_state_repeat` rows from
/// `integrity_warnings`. Going forward `detect_for_entry` no longer
/// files this kind — `repeated_state` is computed live from the
/// `entries` table by `integrity::list_repeated_states`. Any rows
/// still in the table are stale by construction (the live query will
/// re-derive the same facts from current data, or stop reporting
/// them if the user has since backfilled / deleted).
///
/// Idempotent: a `DELETE` on an empty (or already-cleared) table is
/// a no-op. Treats a missing `integrity_warnings` table as already
/// cleaned — some upgrade-path tests stamp the schema forward without
/// creating the rename target.
fn m27_to_28_drop_stored_same_state_repeat(conn: &Connection) -> Result<()> {
    let has_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'integrity_warnings'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !has_table {
        return Ok(());
    }
    conn.execute(
        "DELETE FROM integrity_warnings WHERE kind = 'same_state_repeat'",
        [],
    )?;
    Ok(())
}

/// Reverse v27→v28: there is nothing to put back. The stored rows we
/// deleted were stale by construction (the live query at v28 would
/// have ignored them anyway); we have no preserved snapshot to
/// re-insert. The down step is a structural no-op so a downgrade
/// chain can pass through without erroring.
fn down_28_to_27_drop_stored_same_state_repeat(_conn: &Connection) -> Result<()> {
    Ok(())
}

/// Reverse v26→v27: drop the cluster_id / cluster_generation columns
/// via a 12-step table rebuild (SQLite's stored-DDL re-parser tripped
/// on inline `--` comments in our schema.sql when we tried `ALTER TABLE
/// ... DROP COLUMN`, mirroring v6→v7's workaround). Any locally-stored
/// cluster_id mapping is discarded — at pre-v1 there are no production
/// clusters that would suffer.
fn down_27_to_26_sync_state_cluster_columns(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE sync_state__v26 (
             db_id         TEXT NOT NULL,
             push_seq      INTEGER NOT NULL DEFAULT 0,
             pull_seq      INTEGER NOT NULL DEFAULT 0,
             my_push_seq   INTEGER NOT NULL DEFAULT 0,
             last_push_at  INTEGER,
             last_pull_at  INTEGER,
             PRIMARY KEY (db_id)
         );
         INSERT INTO sync_state__v26 (db_id, push_seq, pull_seq, my_push_seq, last_push_at, last_pull_at)
            SELECT db_id, push_seq, pull_seq, my_push_seq, last_push_at, last_pull_at FROM sync_state;
         DROP TABLE sync_state;
         ALTER TABLE sync_state__v26 RENAME TO sync_state;",
    )?;
    Ok(())
}

/// Rebuild `entries` to a specific (unit, dur_precision) shape,
/// preserving all stored rows. Used by m22_to_23 (forward + down).
/// Indexes and the `following_state`/`dur` CHECK are recreated.
fn rebuild_entries_v23(conn: &Connection, with_unit: bool, with_dur_precision: bool) -> Result<()> {
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

/// da-lxyw: forward 28→29 — drop the `NOT NULL` on `entries.occur_time`
/// so a bare `add` (no `-t`) can be stored timeless (`occur_time` NULL).
/// SQLite has no `ALTER COLUMN`, so rebuild the table preserving every
/// row, then re-derive `verb` is unnecessary — the column shape and data
/// are unchanged apart from the relaxed constraint.
fn m28_to_29_occur_time_nullable(conn: &Connection) -> Result<()> {
    rebuild_entries_occur_time(conn, true)
}

/// Reverse 29→28 — restore `NOT NULL` on `occur_time`. Timeless rows
/// (NULL `occur_time`) created under v29 are backfilled with
/// `create_time` during the rebuild so they satisfy the constraint —
/// matching the pre-da-lxyw stub behaviour that defaulted to create_time.
fn down_29_to_28_occur_time_nullable(conn: &Connection) -> Result<()> {
    rebuild_entries_occur_time(conn, false)
}

/// Rebuild `entries` toggling only the `occur_time` nullability, keeping
/// all other columns, the `following_state` CHECK, and all nine indexes.
/// When `occur_nullable` is false (down), NULL `occur_time` rows are
/// backfilled with `create_time` so the restored `NOT NULL` holds.
fn rebuild_entries_occur_time(conn: &Connection, occur_nullable: bool) -> Result<()> {
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
    // the entries table. Drop the v2 shape and recreate with the v3
    // shape (hid PK, entry_shape, precision, intent, generated verb, etc.).
    //
    // Self-contained recreation matters because (a) callers shouldn't have
    // to re-run schema.sql to land at a coherent v3, and (b) later
    // migrations (m5→6, m7→8, m9→10, m11→12) ALTER/INDEX `entries` and
    // would break otherwise. The shape here matches schema.sql today MINUS
    // the columns added by later migrations (`user_asserted_last_state` is
    // added at v7→8; `extra_metadata` is added at v11→12) — those flow
    // back in via their own ALTER TABLE steps.
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_entries_hid;
         DROP INDEX IF EXISTS idx_entries_field;
         DROP INDEX IF EXISTS idx_entries_time;
         DROP INDEX IF EXISTS idx_entries_content_hash;
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
         CREATE INDEX IF NOT EXISTS entries_content_hash
             ON entries (content_hash);
         CREATE INDEX IF NOT EXISTS entries_verb ON entries (verb);",
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
    //
    // Idempotency (da-9u9.1): the new shape-aware `infer_legacy_version`
    // can resume migrations from v8 on a DB that was previously stamped
    // here via the old buggy schema.sql seed. Re-running unconditionally
    // would write duplicate IGNORED entries at a fresh `now`, since the
    // content_hash binds to start_time. Skip fields that already carry an
    // IGNORED switch entry — the prior run is canonical.
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
        let already: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries
                 WHERE field = ?1 AND value = '\"IGNORED\"' AND deleted_at IS NULL",
                params![&field],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if already > 0 {
            continue;
        }
        let entry = crate::entry::Entry::new(
            field,
            crate::entry::Verb::Switch,
            Some(serde_json::Value::String("IGNORED".to_string())),
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

/// Reverse v1→v2: restore the v1 sync_state placeholder shape.
/// Drops the C4a-shape table and recreates the (device_id, last_seq,
/// updated_at) placeholder. The v2 table was a no-op for any DB that
/// hadn't synced, so this loses no real data on rollback.
fn down_2_to_1_sync_state(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS sync_state;
         CREATE TABLE sync_state (
             device_id   TEXT NOT NULL PRIMARY KEY,
             last_seq    INTEGER NOT NULL DEFAULT 0,
             updated_at  TEXT
         );",
    )?;
    Ok(())
}

/// Reverse v2→v3: drop the v3 entries table and recreate a minimal
/// v2-shape stub.
///
/// **DATA LOSS.** The v2→v3 forward jump was itself a structural rewrite
/// that dropped the old entries table. Walking back across this step
/// destroys every row in the current entries table. The v2 stub
/// recreated here mirrors the public columns the v2 schema exposed
/// (slug PK, JSON value, plain timestamps) but is not a faithful
/// restoration — it exists so the runner can land at v=2 and the
/// chain-test harness can verify schema reversibility. Production
/// callers must back up before walking down across this step.
fn down_3_to_2_entries_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_slug;
         DROP INDEX IF EXISTS entries_field_start;
         DROP INDEX IF EXISTS entries_live_field_start;
         DROP INDEX IF EXISTS entries_content_hash;
         DROP INDEX IF EXISTS entries_verb;
         DROP INDEX IF EXISTS entries_hybrid_time;
         DROP TABLE IF EXISTS entries;
         CREATE TABLE entries (
             slug            TEXT NOT NULL PRIMARY KEY,
             content_hash    TEXT NOT NULL,
             field           TEXT NOT NULL,
             value           TEXT,
             data            TEXT,
             time_start      TEXT,
             time_end        TEXT,
             timezone        TEXT,
             create_time     TEXT NOT NULL,
             modify_time     TEXT NOT NULL,
             device_id       TEXT NOT NULL,
             deleted_at      TEXT
         );",
    )?;
    Ok(())
}

/// Reverse v3→v4: drop the entry_history table and its indexes.
/// Forward step was purely additive, so the down is a clean schema
/// reversal — but any history rows recorded between the up and down
/// are lost.
fn down_4_to_3_entry_history(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entry_history_entry;
         DROP INDEX IF EXISTS entry_history_change_kind;
         DROP TABLE IF EXISTS entry_history;",
    )?;
    Ok(())
}

/// Reverse v4→v5: drop the sync_conflicts table and its indexes.
/// Any pending conflict rows are discarded.
fn down_5_to_4_sync_conflicts(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS sync_conflicts_hid;
         DROP INDEX IF EXISTS sync_conflicts_slug;
         DROP INDEX IF EXISTS sync_conflicts_pending;
         DROP TABLE IF EXISTS sync_conflicts;",
    )?;
    Ok(())
}

/// Reverse v5→v6: drop the partial live-field-start index. The base
/// `entries_field_start` index is unaffected.
fn down_6_to_5_live_field_index(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_live_field_start;")?;
    Ok(())
}

/// Reverse v6→v7: drop the `my_push_seq` column from sync_state via a
/// table-rebuild. SQLite's `ALTER TABLE … DROP COLUMN` re-parses the
/// stored CREATE statement and stumbles on inline `--` comments in our
/// schema.sql, so we use the 12-step rebuild pattern instead.
/// Any per-device push-seq counters are lost — but at v6 there were
/// no production pushes that depended on them.
fn down_7_to_6_sync_state_my_push_seq(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE sync_state__v6 (
             db_id         TEXT NOT NULL,
             push_seq      INTEGER NOT NULL DEFAULT 0,
             pull_seq      INTEGER NOT NULL DEFAULT 0,
             last_push_at  INTEGER,
             last_pull_at  INTEGER,
             PRIMARY KEY (db_id)
         );
         INSERT INTO sync_state__v6 (db_id, push_seq, pull_seq, last_push_at, last_pull_at)
            SELECT db_id, push_seq, pull_seq, last_push_at, last_pull_at FROM sync_state;
         DROP TABLE sync_state;
         ALTER TABLE sync_state__v6 RENAME TO sync_state;",
    )?;
    Ok(())
}

/// Reverse v7→v8: drop `entries.user_asserted_last_state` via a table
/// rebuild. The column is metadata-only and is not referenced by any
/// generated column. We use rebuild rather than `ALTER TABLE … DROP
/// COLUMN` so we don't depend on SQLite's stored-DDL re-parser. Any -L
/// assertion values recorded between up and down are discarded.
fn down_8_to_7_entries_user_asserted_last_state(conn: &Connection) -> Result<()> {
    rebuild_entries_v3(
        conn, /* with_user_asserted = */ false, /* with_extra_metadata = */ false,
    )
}

/// Reverse v8→v9: delete the synthetic IGNORED switch entries written
/// by the forward step.
///
/// **DATA LOSS / HEURISTIC.** The forward migration tagged its writes
/// with `device_id = 'migration-v9'`, so the down deletes exactly those
/// entries. If the user has manually written legitimate switch entries
/// with that sentinel device_id (extremely unlikely — it is only ever
/// emitted by this migration), they will be removed. The legacy
/// `tracked` / `ignored` fields inside `field_specs.spec_json` were
/// not touched by the forward step and remain readable, so the spec
/// side is reversible without effort.
fn down_9_to_8_track_ignore_unification(conn: &Connection) -> Result<()> {
    // Best-effort: only delete if the entries table still exists at this
    // point in the down walk. The runner walks v19→v1 so entries is
    // present here.
    let entries_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='entries'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if entries_exists {
        conn.execute("DELETE FROM entries WHERE device_id = 'migration-v9'", [])?;
    }
    Ok(())
}

/// Reverse v9→v10: drop the hybrid-time expression index.
fn down_10_to_9_hybrid_time_index(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_hybrid_time;")?;
    Ok(())
}

/// Reverse v10→v11: drop the last_docs table.
fn down_11_to_10_last_docs(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS last_docs;")?;
    Ok(())
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
fn down_12_to_11_extra_metadata(conn: &Connection) -> Result<()> {
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

/// Reverse v12→v13: drop `field_specs.kit_id` and its index via table
/// rebuild. Any kit-installed rows lose their kit linkage; uninstall
/// by kit_id is no longer possible while at v=12.
fn down_13_to_12_field_specs_kit_id(conn: &Connection) -> Result<()> {
    rebuild_field_specs(
        conn, /* with_kit_id = */ false, /* with_tags = */ false,
        /* with_render_json = */ false,
    )
}

/// Reverse v13→v14: drop the views table and its index. Any installed
/// view definitions are lost; reinstall the kit to recreate them.
fn down_14_to_13_views_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_views_kit_id;
         DROP TABLE IF EXISTS views;",
    )?;
    Ok(())
}

/// Reverse v14→v15: drop the kit_indices bookkeeping table. Any
/// real CREATE INDEX statements installed by kits remain on `entries`
/// but their (kit_id, index_name) ownership record is gone, so a
/// subsequent uninstall at v=14 cannot find them. Reinstall the kit
/// to repopulate the bookkeeping.
fn down_15_to_14_kit_indices(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS kit_indices;")?;
    Ok(())
}

/// Reverse v15→v16: drop `aliases.kit_id` and its index via table
/// rebuild. Kit-installed alias rows lose their kit linkage but remain
/// in the table.
fn down_16_to_15_aliases_kit_id(conn: &Connection) -> Result<()> {
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

/// Reverse v16→v17: drop the kit_commands table and its index. Any
/// installed kit commands are lost; reinstall the kit to recreate them.
fn down_17_to_16_kit_commands_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_kit_commands_kit_id;
         DROP TABLE IF EXISTS kit_commands;",
    )?;
    Ok(())
}

/// Reverse v17→v18: drop `field_specs.tags` via table rebuild. Any
/// tag values written after the up are discarded. Spec-side tags inside
/// `spec_json` are untouched (they were never moved out by the up).
fn down_18_to_17_field_specs_tags(conn: &Connection) -> Result<()> {
    rebuild_field_specs(
        conn, /* with_kit_id = */ true, /* with_tags = */ false,
        /* with_render_json = */ false,
    )
}

/// Reverse v18→v19: drop `field_specs.render_json` via table rebuild.
/// Render-visibility overrides written after the up are discarded;
/// spec-side `render` values inside `spec_json` are untouched.
fn down_19_to_18_field_specs_render_json(conn: &Connection) -> Result<()> {
    rebuild_field_specs(
        conn, /* with_kit_id = */ true, /* with_tags = */ true,
        /* with_render_json = */ false,
    )
}

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
fn down_20_to_19_time_shape_pivot(conn: &Connection) -> Result<()> {
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

// ─── Table rebuild helpers ───────────────────────────────────────────
//
// SQLite's `ALTER TABLE … DROP COLUMN` re-parses the stored CREATE
// statement and trips on the inline `--` comments embedded in
// schema.sql. Rather than rewriting schema.sql, the down migrations
// rebuild affected tables in-place using the standard 12-step pattern
// (CREATE new, copy rows, drop old, rename) so they don't depend on
// SQLite's stored-DDL re-parser.

/// Rebuild `field_specs` to a specific (kit_id, tags, render_json)
/// shape, preserving all rows. The PK and `idx_field_specs_kit_id`
/// index are recreated as appropriate for the target shape.
fn rebuild_field_specs(
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
fn rebuild_aliases(conn: &Connection, with_derived_from_field: bool) -> Result<()> {
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
fn rebuild_entries_v3(
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
/// version we should stamp it at, by introspecting actual table/column
/// shapes — never blindly 1.
///
/// **Why this matters (2026-04-30 incident).** Before this function was
/// shape-aware, it returned 1 unconditionally and `schema.sql` carried a
/// literal `INSERT OR IGNORE INTO _meta VALUES ('schema_version', '18')`
/// seed. Together those two paths produced silent half-states: a legacy
/// DB without `_meta` had its version stamped at 18 by schema.sql, then
/// `migrate_to_current` saw no work to do, and migrations v1..v17 — including
/// the `m12_to_13` step that adds `field_specs.kit_id` — never ran. The
/// next `dm` invocation hit "no such column: kit_id". Fix: schema.sql no
/// longer seeds `_meta`; this function does the stamping after walking the
/// real shape.
///
/// **Floor at v3.** Pre-v3 entries (ULID `id` PK) are rejected upstream in
/// `Db::migrate`. A fresh DB (no datum tables yet) short-circuits to
/// [`CURRENT_SCHEMA_VERSION`] before any column probe can return low, so
/// the destructive migrations v1→v2 (DROP/CREATE `sync_state`) and v2→v3
/// (DROP `entries`) are unreachable via infer — both would wipe live rows
/// if they ran against a real-data DB. Real-data DBs always have at least
/// one datum table present, fall through past the short-circuit, and the
/// column probes return ≥ 6 (or [`CURRENT_SCHEMA_VERSION`] when fully
/// current).
///
/// **How we detect.** First a fresh-DB short-circuit: if no non-`_meta`
/// datum table exists, the DB was just created and schema.sql will lay
/// down the current shape — stamp current and skip the registry walk.
/// Otherwise walk the column-add migrations in order. Adding a column to
/// a pre-existing table is the one operation `CREATE TABLE IF NOT EXISTS`
/// cannot mask: if a legacy DB is at v12 shape, `field_specs.kit_id` is
/// genuinely absent, and we can prove it. The first missing column fixes
/// the resume point at the version *immediately before* the migration
/// that adds it.
pub fn infer_legacy_version(conn: &Connection) -> Result<u32> {
    // Fresh-DB short-circuit. `Db::migrate` no longer runs schema.sql
    // before this function, so a brand-new file has zero datum tables.
    // Stamp at CURRENT_SCHEMA_VERSION so schema.sql can lay down the
    // latest shape directly — and so the destructive v1→v2 / v2→v3
    // migrations (which assume legacy `sync_state` / `entries` tables to
    // drop) are unreachable. `_meta` is excluded: `read_schema_version`
    // creates it as a side-effect via `ensure_meta_table`, so its presence
    // is not evidence of pre-existing user data.
    let datum_tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != '_meta'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if datum_tables == 0 {
        return Ok(CURRENT_SCHEMA_VERSION);
    }

    // m6_to_7: sync_state.my_push_seq.
    if !has_column(conn, "sync_state", "my_push_seq")? {
        return Ok(6);
    }
    // m7_to_8: entries.user_asserted_last_state.
    if !has_column(conn, "entries", "user_asserted_last_state")? {
        return Ok(7);
    }
    // m11_to_12: entries.extra_metadata.
    if !has_column(conn, "entries", "extra_metadata")? {
        return Ok(11);
    }
    // m12_to_13: field_specs.kit_id — the 2026-04-30 regression site.
    if !has_column(conn, "field_specs", "kit_id")? {
        return Ok(12);
    }
    // m15_to_16: aliases.kit_id.
    if !has_column(conn, "aliases", "kit_id")? {
        return Ok(15);
    }
    // m17_to_18: field_specs.tags.
    if !has_column(conn, "field_specs", "tags")? {
        return Ok(17);
    }
    // m18_to_19: field_specs.render_json.
    if !has_column(conn, "field_specs", "render_json")? {
        return Ok(18);
    }
    // m19_to_20: time-shape pivot — entries.start_time was dropped in
    // favour of entries.occur_time. A v=19 DB still carries `start_time`
    // post-schema.sql (the `CREATE TABLE IF NOT EXISTS` no-ops on the
    // existing v19 entries shape), so its presence pins the DB at v=19
    // and the runner walks it forward through the rebuild.
    if has_column(conn, "entries", "start_time")? {
        return Ok(19);
    }
    // m20_to_21: aliases.derived_from_field was dropped at v=21. Its
    // presence pins the DB at v=20.
    if has_column(conn, "aliases", "derived_from_field")? {
        return Ok(20);
    }
    // m22_to_23: entries.unit was dropped (and entries.dur_precision
    // added) at v=23. A legacy v=22 DB still carries `unit` post-
    // schema.sql because `CREATE TABLE IF NOT EXISTS` no-ops on the
    // existing v=22 entries shape; its presence pins the DB at v=22 so
    // the runner walks the rebuild forward. Pre-v22 verb canonicalization
    // (m21→m22) is an in-place UPDATE with no structural footprint, so
    // we do not probe for it here.
    if has_column(conn, "entries", "unit")? {
        return Ok(22);
    }
    // All column-add features present → DB is already at current shape.
    // Idempotent re-runs are CREATE/INDEX-IF-NOT-EXISTS and will no-op.
    Ok(CURRENT_SCHEMA_VERSION)
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(n > 0)
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

/// Walk the registry backward from `current` down to `target`, calling
/// each step's `down` in version-descending order. All steps run inside
/// a single transaction so a partial failure rolls back to `current`.
/// Returns one summary line per applied down step.
pub fn apply_migrations_down(
    conn: &Connection,
    migrations: &[Migration],
    current: u32,
    target: u32,
) -> Result<Vec<String>> {
    if current == target {
        return Ok(vec![]);
    }
    if current < target {
        return Err(DatumError::IncompatibleSchema(format!(
            "down-walk requires current ({current}) >= target ({target})"
        )));
    }
    let tx = conn.unchecked_transaction()?;
    let mut applied = Vec::new();
    let mut version = current;
    while version > target {
        let step = migrations.iter().find(|m| m.to == version).ok_or_else(|| {
            DatumError::IncompatibleSchema(format!(
                "no down-migration path from schema v{version} toward v{target}"
            ))
        })?;
        (step.down)(&tx)?;
        version = step.from;
        set_schema_version(&tx, version)?;
        applied.push(format!(
            "v{}→v{}: down — {}",
            step.to, step.from, step.summary
        ));
    }
    tx.commit()?;
    Ok(applied)
}

/// Walk the module-level [`MIGRATIONS`] registry backward to `target`.
/// Convenience wrapper around [`apply_migrations_down`].
pub fn migrate_down_to(conn: &Connection, target: u32) -> Result<Vec<String>> {
    let current = read_schema_version(conn)?.unwrap_or(CURRENT_SCHEMA_VERSION);
    apply_migrations_down(conn, MIGRATIONS, current, target)
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

/// The schema version a DB is at, resolved without writing to it.
///
/// [`migrate_to_current`] resolves the same value but stamps `_meta` as a
/// side-effect. The pre-migration backup has to know `from` *before* any
/// write lands, so this variant infers without stamping.
pub fn peek_schema_version(conn: &Connection) -> Result<u32> {
    let has_meta: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_meta'",
        [],
        |row| row.get::<_, i64>(0),
    )? > 0;
    if has_meta {
        let stored: Option<String> = conn
            .query_row(
                "SELECT value FROM _meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(s) = stored {
            return s
                .parse::<u32>()
                .map_err(|e| DatumError::Other(format!("bad schema_version in _meta: {e}")));
        }
    }
    infer_legacy_version(conn)
}

/// What a single [`crate::db::Db::open`] did to the schema, so the CLI can
/// say so. `datum-core` is deliberately silent — the migration happens
/// here, the telling happens at the edge.
#[derive(Debug, Clone)]
pub struct MigrationReport {
    pub from_version: u32,
    pub to_version: u32,
    /// The pre-migration snapshot. `None` only for in-memory databases,
    /// which have no file to snapshot.
    pub backup_path: Option<std::path::PathBuf>,
    /// One summary line per migration step applied.
    pub applied: Vec<String>,
    /// Older snapshots removed by retention after this migration.
    pub pruned: Vec<std::path::PathBuf>,
}

/// How many pre-migration snapshots to keep alongside a database. Older
/// ones are pruned after a successful migration — never before, so a
/// failed migration can never cost you a backup.
pub const MAX_PRE_MIGRATION_BACKUPS: usize = 5;

/// Snapshot the database file before migrating it. Returns the path written.
///
/// `VACUUM INTO`, not a filesystem copy: connections run in WAL mode, so
/// the `.db` file on its own is not the whole database, and a copy taken
/// without the `-wal` sidecar can be stale. `VACUUM INTO` writes one
/// self-contained file holding exactly the committed contents, and — the
/// point of the exercise — it goes through SQLite, not through datum's
/// serialisation code, which is the code the migration is changing.
///
/// The caller must treat a failure here as fatal: a migration that runs
/// after a failed backup is worse than no backup policy at all, because
/// the note printed afterwards would claim a snapshot that isn't there.
pub fn backup_before_migration(
    conn: &Connection,
    db_path: &std::path::Path,
    from: u32,
    to: u32,
) -> Result<std::path::PathBuf> {
    let dest = free_backup_path(db_path, from, to);
    // `VACUUM INTO` refuses to overwrite, so an existing file is an error
    // rather than silent data loss — exactly what we want.
    conn.execute("VACUUM INTO ?1", params![dest.to_string_lossy()])
        .map_err(|e| {
            DatumError::Other(format!(
                "refusing to migrate schema v{from}→v{to}: could not write the \
                 pre-migration backup to {}: {e}",
                dest.display()
            ))
        })?;
    Ok(dest)
}

/// `<db file name>.pre-v<from>-to-v<to>-<UTC timestamp>.bak`, next to the
/// database, with a `-<n>` disambiguator if that name is somehow taken.
fn free_backup_path(db_path: &std::path::Path, from: u32, to: u32) -> std::path::PathBuf {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let base = format!(
        "{}.pre-v{from}-to-v{to}-{stamp}",
        db_path.file_name().unwrap_or_default().to_string_lossy()
    );
    let dir = db_path.parent().unwrap_or(std::path::Path::new("."));
    let mut candidate = dir.join(format!("{base}.bak"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base}-{n}.bak"));
        n += 1;
    }
    candidate
}

/// Drop all but the newest [`MAX_PRE_MIGRATION_BACKUPS`] snapshots taken
/// for this database. Best-effort: anything we cannot read or remove is
/// left alone, since failing to prune is harmless and refusing to open the
/// database over it would not be.
pub fn prune_pre_migration_backups(db_path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let (Some(dir), Some(name)) = (db_path.parent(), db_path.file_name()) else {
        return Vec::new();
    };
    let prefix = format!("{}.pre-v", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with(&prefix) && n.ends_with(".bak")
        })
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.path()))
        })
        .collect();
    if found.len() <= MAX_PRE_MIGRATION_BACKUPS {
        return Vec::new();
    }
    // Newest first, then discard everything past the keep window.
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    found
        .split_off(MAX_PRE_MIGRATION_BACKUPS)
        .into_iter()
        .filter(|(_, path)| std::fs::remove_file(path).is_ok())
        .map(|(_, path)| path)
        .collect()
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

    /// da-bq31: fresh `Db::open_in_memory` must materialize exactly the
    /// shape declared in `schema.sql` — no extra indexes from migration
    /// rebuilds, no missing tables. Locks down the new migrate() flow
    /// (schema.sql runs once, at the end) so any drift between the
    /// migration registry and schema.sql is caught at test time.
    #[test]
    fn fresh_db_matches_schema_sql_shape() {
        let db = Db::open_in_memory().unwrap();

        let mut tables: Vec<String> = db
            .conn()
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type='table' AND name NOT LIKE 'sqlite_%' \
                 ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        tables.sort();
        let expected_tables = vec![
            "_meta",
            "aliases",
            "entries",
            "entry_history",
            "field_specs",
            "integrity_warnings",
            "kit_commands",
            "kit_indices",
            "last_docs",
            "sync_state",
            "views",
        ];
        assert_eq!(tables, expected_tables);

        let mut indexes: Vec<String> = db
            .conn()
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type='index' AND name NOT LIKE 'sqlite_%' \
                 ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        indexes.sort();
        let expected_indexes = vec![
            "entries_content_hash",
            "entries_field_occur",
            "entries_field_occurtime",
            "entries_field_structure",
            "entries_id_structure",
            "entries_live_field_occur",
            "entries_modify_time_ms",
            "entries_occur_time",
            "entries_slug",
            "entries_verb",
            "entry_history_change_kind",
            "entry_history_entry",
            "idx_aliases_kit_id",
            "idx_field_specs_kit_id",
            "idx_kit_commands_kit_id",
            "idx_views_kit_id",
            "integrity_warnings_hid",
            "integrity_warnings_pending",
            "integrity_warnings_slug",
        ];
        assert_eq!(indexes, expected_indexes);
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
        fn noop_down(_: &Connection) -> Result<()> {
            Ok(())
        }
        let migrations = &[
            Migration {
                from: 1,
                to: 2,
                summary: "v2 table",
                apply: m1_to_2,
                down: noop_down,
            },
            Migration {
                from: 2,
                to: 3,
                summary: "v3 table",
                apply: m2_to_3,
                down: noop_down,
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
        fn noop_down(_: &Connection) -> Result<()> {
            Ok(())
        }
        let migrations = &[
            Migration {
                from: 1,
                to: 2,
                summary: "add t2",
                apply: m1_to_2,
                down: noop_down,
            },
            Migration {
                from: 2,
                to: 3,
                summary: "add t3",
                apply: m2_to_3,
                down: noop_down,
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
        fn noop_down(_: &Connection) -> Result<()> {
            Ok(())
        }
        let migrations = &[
            Migration {
                from: 1,
                to: 2,
                summary: "m12",
                apply: m1_to_2,
                down: noop_down,
            },
            Migration {
                from: 2,
                to: 3,
                summary: "m23",
                apply: m2_to_3,
                down: noop_down,
            },
            Migration {
                from: 3,
                to: 4,
                summary: "m34",
                apply: m3_to_4,
                down: noop_down,
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

    /// da-ga8b: v24→v25 creates the partial UNIQUE on `(field, occur_time)`
    /// over a live, collision-free DB. The new index must exist and be
    /// flagged UNIQUE in `sqlite_master`. Re-running migrate_to_current is
    /// a no-op (the CREATE is `IF NOT EXISTS`).
    #[test]
    fn m24_to_25_creates_partial_unique_on_clean_db() {
        let db = Db::open_in_memory().unwrap();
        // Fresh DB is already at CURRENT_SCHEMA_VERSION; the index exists.
        let n: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='index' AND name='entries_field_occurtime'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "fresh DB must carry the field/occur_time index");

        // Re-running is a no-op.
        let again = migrate_to_current(db.conn()).unwrap();
        assert!(again.is_empty());
    }

    /// da-ga8b: v24→v25 fails cleanly when live rows already collide on
    /// `(field, occur_time)`. The error message lists each offending
    /// group with field, occur_time, and the hids.
    #[test]
    fn m24_to_25_aborts_with_diagnostic_on_dirty_db() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        // Minimal v24-shape entries table: only the columns the pre-check
        // and CREATE INDEX care about. We bypass the schema's full DDL
        // (and the slug UNIQUE constraint) to plant the dirty rows.
        conn.execute_batch(
            "CREATE TABLE entries (
                hid          TEXT PRIMARY KEY,
                slug         TEXT NOT NULL,
                field        TEXT NOT NULL,
                occur_time   TEXT,
                deleted_at   TEXT
             );",
        )
        .unwrap();
        // Two distinct hids/slugs sharing the same (field, occur_time).
        conn.execute(
            "INSERT INTO entries (hid, slug, field, occur_time, deleted_at) VALUES \
             ('hidA000000001', 'project:2026-05-13T09:00Z', 'project', '2026-05-13T09:00:00Z', NULL), \
             ('hidB000000002', 'custom-id-B',               'project', '2026-05-13T09:00:00Z', NULL)",
            [],
        )
        .unwrap();
        set_schema_version(&conn, 24).unwrap();

        let err = migrate_to_current(&conn).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("v24→v25"),
            "diagnostic must name the failing step: {msg}"
        );
        assert!(
            msg.contains("project") && msg.contains("2026-05-13T09:00:00Z"),
            "diagnostic must include field + occur_time: {msg}"
        );
        assert!(
            msg.contains("hidA000000001") && msg.contains("hidB000000002"),
            "diagnostic must list the offending hids: {msg}"
        );

        // Tx rolled back: schema_version still at 24 and the index is absent.
        assert_eq!(read_schema_version(&conn).unwrap(), Some(24));
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='index' AND name='entries_field_occurtime'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0, "failed migration must not leave the index behind");
    }

    /// da-ga8b: the pre-check filters tombstoned rows, so soft-delete +
    /// re-insert at the same (field, occur_time) does not block the
    /// migration.
    #[test]
    fn m24_to_25_ignores_tombstones() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE entries (
                hid          TEXT PRIMARY KEY,
                slug         TEXT NOT NULL,
                field        TEXT NOT NULL,
                occur_time   TEXT,
                deleted_at   TEXT
             );",
        )
        .unwrap();
        // Live row + tombstoned row at the same (field, occur_time) is fine.
        conn.execute(
            "INSERT INTO entries (hid, slug, field, occur_time, deleted_at) VALUES \
             ('hidA000000001', 'a', 'mood', '2026-05-13T09:00:00Z', NULL), \
             ('hidB000000002', 'b', 'mood', '2026-05-13T09:00:00Z', '2026-05-13T10:00:00Z')",
            [],
        )
        .unwrap();
        set_schema_version(&conn, 24).unwrap();

        // Scope to the v24→v25 step under test. The minimal fixture omits
        // most entries columns (only the pre-check's field/occur_time/
        // deleted_at matter here); the v28→v29 rebuild (da-lxyw) needs the
        // full column set, so a `migrate_to_current` would trip on the
        // stub. The clean-DB and roundtrip suites cover the full chain.
        let applied = apply_migrations(&conn, MIGRATIONS, 24, 25).unwrap();
        assert!(applied.iter().any(|s| s.contains("v24→v25")));
        assert_eq!(read_schema_version(&conn).unwrap(), Some(25));
    }

    /// da-9u9.1: `infer_legacy_version` returns the correct resume point
    /// for each column-add boundary. The first missing column fixes the
    /// resume point at the version *immediately before* the migration
    /// that adds it.
    #[test]
    fn infer_legacy_version_resumes_at_first_missing_column() {
        // Helper: build a synthetic shape and assert the inferred resume.
        fn assert_resume(setup: &str, expected: u32, label: &str) {
            let conn = rusqlite::Connection::open_in_memory().unwrap();
            conn.execute_batch(setup).unwrap();
            let v = infer_legacy_version(&conn).unwrap();
            assert_eq!(v, expected, "{label}: expected v{expected}, got v{v}");
        }

        // sync_state without my_push_seq → resume at 6.
        assert_resume(
            "CREATE TABLE sync_state (db_id TEXT PRIMARY KEY);",
            6,
            "missing my_push_seq",
        );

        // entries without user_asserted_last_state → resume at 7.
        assert_resume(
            "CREATE TABLE sync_state (db_id TEXT PRIMARY KEY, my_push_seq INTEGER);
             CREATE TABLE entries (hid TEXT PRIMARY KEY);",
            7,
            "missing user_asserted_last_state",
        );

        // field_specs without kit_id → resume at 12 (the 2026-04-30 case).
        assert_resume(
            "CREATE TABLE sync_state (db_id TEXT PRIMARY KEY, my_push_seq INTEGER);
             CREATE TABLE entries (hid TEXT PRIMARY KEY,
                                   user_asserted_last_state TEXT,
                                   extra_metadata TEXT);
             CREATE TABLE field_specs (field TEXT PRIMARY KEY, spec_json TEXT);",
            12,
            "missing field_specs.kit_id",
        );

        // All column-add features present → at current shape.
        assert_resume(
            "CREATE TABLE sync_state (db_id TEXT PRIMARY KEY, my_push_seq INTEGER);
             CREATE TABLE entries (hid TEXT PRIMARY KEY,
                                   user_asserted_last_state TEXT,
                                   extra_metadata TEXT);
             CREATE TABLE field_specs (field TEXT PRIMARY KEY, spec_json TEXT,
                                       kit_id TEXT, tags TEXT, render_json TEXT);
             CREATE TABLE aliases (name TEXT PRIMARY KEY, kit_id TEXT);",
            CURRENT_SCHEMA_VERSION,
            "all column features present",
        );
    }

    /// da-9u9.1: `Db::open_unmigrated` returns a usable connection that
    /// has NOT had migrations applied. It's the recovery path's foothold:
    /// `db purge` opens a possibly-corrupt DB without auto-migrate, then
    /// drops & recreates the schema.
    #[test]
    fn open_unmigrated_skips_migration_pipeline() {
        // Build a temp file path — we need real path-backed open() since
        // open_unmigrated takes &str.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.db");
        let path_str = path.to_string_lossy().to_string();

        // Pre-corrupt the DB: write a `_meta.schema_version` past
        // CURRENT_SCHEMA_VERSION so a normal `Db::open` would error.
        {
            let conn = rusqlite::Connection::open(&path_str).unwrap();
            ensure_meta_table(&conn).unwrap();
            set_schema_version(&conn, CURRENT_SCHEMA_VERSION + 99).unwrap();
        }

        // `Db::open` errors on the bad version.
        match Db::open(&path_str) {
            Err(DatumError::IncompatibleSchema(_)) => {}
            Err(other) => panic!("expected IncompatibleSchema, got {other:?}"),
            Ok(_) => panic!("Db::open must surface the corrupt version"),
        }

        // `Db::open_unmigrated` succeeds — it does not consult _meta.
        let db = Db::open_unmigrated(&path_str).unwrap();
        // And from here, purge brings the DB back to current.
        db.force_recreate_schema().unwrap();
        assert_eq!(
            read_schema_version(db.conn()).unwrap(),
            Some(CURRENT_SCHEMA_VERSION),
        );
    }

    /// Seed a DB with the row shapes whose hashes the v33 definition moves:
    /// a plain point, a block, a block-switch (`following_state` set), and a
    /// tombstone. Returns the db.
    fn db_with_hashable_rows() -> Db {
        use crate::entry::Verb;
        use crate::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let at = |s: &str| {
            DatumTime::from_utc_and_tz(
                chrono::DateTime::parse_from_rfc3339(s)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                "UTC",
            )
            .unwrap()
        };

        let point = crate::entry::Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(at("2026-01-02T03:04:05Z")),
            None,
            "dev",
        );
        db.insert_entry(&point).unwrap();

        let block = crate::entry::Entry::new(
            "project",
            Verb::Block,
            Some(serde_json::json!("writing")),
            None,
            Some(at("2026-01-02T09:00:00Z")),
            Some(crate::dur::Dur::of(chrono::Duration::minutes(30))),
            "dev",
        );
        db.insert_entry(&block).unwrap();

        let mut block_switch = crate::entry::Entry::new(
            "project",
            Verb::Block,
            Some(serde_json::json!("writing")),
            None,
            Some(at("2026-01-02T11:00:00Z")),
            Some(crate::dur::Dur::of(chrono::Duration::minutes(30))),
            "dev",
        );
        block_switch.following_state = Some(serde_json::json!("resting"));
        db.insert_entry(&block_switch).unwrap();

        let doomed = crate::entry::Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(at("2026-01-02T15:00:00Z")),
            None,
            "dev",
        );
        db.insert_entry(&doomed).unwrap();
        db.soft_delete_entry(doomed.slug.as_str()).unwrap();

        db
    }

    /// Read every row through the live `Entry` mapper while the schema is
    /// still pre-v34, where the two uncertainty columns carry their
    /// `*_precision` names. Same aliasing `rehash_all_entries` does, and
    /// for the same reason: the mapper speaks today's column names, a
    /// mid-walk schema does not (da-b8rs).
    fn entries_pre_v34(db: &Db) -> Vec<crate::entry::Entry> {
        let conn = db.conn();
        let mut stmt = conn
            .prepare(
                "SELECT *, occur_time_precision AS occur_time_uncertainty, \
                 dur_precision AS dur_uncertainty FROM entries",
            )
            .unwrap();
        let rows = stmt.query_map([], crate::db::row_to_entry_public).unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    /// da-4vyw acceptance: a v32 database opens, migrates to v33, and every
    /// row's stored `content_hash` equals its `expected_content_hash`
    /// afterwards — the same predicate `datum check` uses, so a migrated DB
    /// reports no hash drift.
    #[test]
    fn m32_to_33_rewrites_every_content_hash() {
        let db = db_with_hashable_rows();

        // Walk back to v32 — `down_33_to_32` restores the pre-da-4vyw
        // five-input hashes, which is exactly the on-disk state of a
        // database written by the previous release.
        migrate_down_to(db.conn(), 32).unwrap();
        assert_eq!(read_schema_version(db.conn()).unwrap(), Some(32));
        let stale: Vec<String> = entries_pre_v34(&db)
            .iter()
            .map(|e| e.content_hash.clone())
            .collect();
        assert_eq!(stale.len(), 4);
        for entry in entries_pre_v34(&db) {
            assert_ne!(
                entry.content_hash,
                entry.expected_content_hash(),
                "the v32 hash must actually differ, or this proves nothing"
            );
        }

        let applied = migrate_to_current(db.conn()).unwrap();
        assert!(applied.iter().any(|s| s.contains("v32→v33")), "{applied:?}");

        let migrated = db.list_including_deleted().unwrap();
        assert_eq!(migrated.len(), 4);
        for entry in &migrated {
            assert_eq!(
                entry.content_hash,
                entry.expected_content_hash(),
                "row {} still carries a drifted hash after v33",
                entry.slug
            );
        }
        // Tombstones are included in the walk, not just live rows.
        assert!(migrated.iter().any(|e| e.deleted_at.is_some()));

        // Idempotent — re-running is a no-op.
        assert!(migrate_to_current(db.conn()).unwrap().is_empty());
    }

    /// Soft-delete moves the row's `content_hash` — a tombstone must not
    /// keep reading as identical content to the live row it replaced — and
    /// the stored value must stay in step with `expected_content_hash` so
    /// `datum check` sees no drift.
    #[test]
    fn soft_delete_and_undelete_keep_the_content_hash_in_step() {
        use crate::entry::Verb;
        use crate::time::DatumTime;

        let db = Db::open_in_memory().unwrap();
        let entry = crate::entry::Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(
                DatumTime::from_utc_and_tz(
                    chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                    "UTC",
                )
                .unwrap(),
            ),
            None,
            "dev",
        );
        db.insert_entry(&entry).unwrap();
        let live_hash = db.list_including_deleted().unwrap()[0].content_hash.clone();

        db.soft_delete_entry(entry.slug.as_str()).unwrap();
        let dead = db.list_including_deleted().unwrap().remove(0);
        assert!(dead.deleted_at.is_some());
        assert_ne!(dead.content_hash, live_hash, "tombstone must rehash");
        assert_eq!(dead.content_hash, dead.expected_content_hash());

        db.undelete(entry.slug.as_str()).unwrap();
        let revived = db.list_including_deleted().unwrap().remove(0);
        assert_eq!(
            revived.content_hash, live_hash,
            "clearing the tombstone must restore the live hash"
        );
        assert_eq!(revived.content_hash, revived.expected_content_hash());
    }

    // ---- pre-migration backups (da-ksao) -------------------------------

    /// Build a file-backed DB, put a row in it, then walk the schema back
    /// to `version` so the next `Db::open` has real migration work to do.
    fn legacy_file_db(path: &std::path::Path, version: u32) {
        let db = Db::open(path.to_str().unwrap()).unwrap();
        let entry = crate::entry::Entry::new(
            "coffee",
            crate::entry::Verb::Point,
            None,
            None,
            Some(
                crate::time::DatumTime::from_utc_and_tz(
                    chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                    "UTC",
                )
                .unwrap(),
            ),
            None,
            "dev",
        );
        db.insert_entry(&entry).unwrap();
        migrate_down_to(db.conn(), version).unwrap();
    }

    /// Read a schema version without the `ensure_meta_table` write, so
    /// inspecting a backup cannot modify it.
    fn version_of(path: &std::path::Path) -> u32 {
        let conn = Connection::open(path).unwrap();
        peek_schema_version(&conn).unwrap()
    }

    fn backups_beside(db_path: &std::path::Path) -> Vec<std::path::PathBuf> {
        let prefix = format!("{}.pre-v", db_path.file_name().unwrap().to_string_lossy());
        let mut found: Vec<_> = std::fs::read_dir(db_path.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let n = p.file_name().unwrap().to_string_lossy().into_owned();
                n.starts_with(&prefix) && n.ends_with(".bak")
            })
            .collect();
        found.sort();
        found
    }

    /// A DB that actually needs migrating is snapshotted first, and the
    /// snapshot holds the *pre*-migration schema and the user's rows.
    #[test]
    fn migrating_a_file_db_snapshots_it_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("datum.db");
        legacy_file_db(&path, 30);
        assert_eq!(version_of(&path), 30);
        assert!(backups_beside(&path).is_empty(), "nothing to back up yet");

        let db = Db::open(path.to_str().unwrap()).unwrap();
        let report = db.last_migration().expect("migration must be reported");
        assert_eq!(report.from_version, 30);
        assert_eq!(report.to_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(report.applied.len(), (CURRENT_SCHEMA_VERSION - 30) as usize);
        assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);

        let backup = report
            .backup_path
            .as_ref()
            .expect("file DBs get a snapshot");
        assert_eq!(backups_beside(&path), vec![backup.clone()]);
        assert!(backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&format!("datum.db.pre-v30-to-v{CURRENT_SCHEMA_VERSION}-")));

        // The snapshot is the old database, not the new one — old shape,
        // and the user's row is in it.
        assert_eq!(version_of(backup), 30);
        let snap = Connection::open(backup).unwrap();
        let rows: i64 = snap
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    /// A DB already at the current version is opened untouched: no
    /// snapshot, no report. Migrations are rare; backups must be too.
    #[test]
    fn an_up_to_date_db_is_not_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("datum.db");
        assert!(Db::open(path.to_str().unwrap())
            .unwrap()
            .last_migration()
            .is_none());
        assert!(Db::open(path.to_str().unwrap())
            .unwrap()
            .last_migration()
            .is_none());
        assert!(backups_beside(&path).is_empty());
    }

    /// The guard: if the snapshot cannot be written, the migration must
    /// not run. Forced by a database filename long enough that appending
    /// the 36-byte `.pre-v30-to-v33-<stamp>.bak` suffix overruns the
    /// filesystem's 255-byte name limit while `…db`, `…db-wal` and
    /// `…db-shm` all still fit — so the database opens and is writable
    /// and only the backup target is unreachable, which is the disk-full
    /// / read-only shape we are guarding against.
    #[test]
    fn a_failed_backup_aborts_the_migration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("{}.db", "d".repeat(240)));
        legacy_file_db(&path, 30);

        let msg = match Db::open(path.to_str().unwrap()) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("open must fail when the snapshot cannot be written"),
        };
        assert!(msg.contains("refusing to migrate"), "{msg}");
        assert!(
            msg.contains(&format!("v30→v{CURRENT_SCHEMA_VERSION}")),
            "{msg}"
        );

        // The database is untouched — still at the old version, no
        // half-written snapshot left beside it.
        assert_eq!(version_of(&path), 30);
        assert!(backups_beside(&path).is_empty());
    }

    /// Retention keeps the newest [`MAX_PRE_MIGRATION_BACKUPS`] snapshots
    /// and reports what it removed, so the accumulation is neither
    /// unbounded nor silent.
    #[test]
    fn retention_prunes_the_oldest_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("datum.db");
        std::fs::write(&path, b"").unwrap();
        // Unrelated neighbours that must survive.
        std::fs::write(dir.path().join("other.db.pre-v1-to-v2-x.bak"), b"").unwrap();
        std::fs::write(dir.path().join("datum.db.bak"), b"").unwrap();

        let epoch = std::time::SystemTime::UNIX_EPOCH;
        let mut made = Vec::new();
        for i in 0..MAX_PRE_MIGRATION_BACKUPS + 2 {
            let p = dir.path().join(format!("datum.db.pre-v{i}-to-v33-t.bak"));
            let f = std::fs::File::create(&p).unwrap();
            f.set_modified(epoch + std::time::Duration::from_secs(i as u64))
                .unwrap();
            made.push(p);
        }

        let mut pruned = prune_pre_migration_backups(&path);
        pruned.sort();
        assert_eq!(pruned, made[..2].to_vec(), "the two oldest go");
        assert_eq!(backups_beside(&path).len(), MAX_PRE_MIGRATION_BACKUPS);
        assert!(dir.path().join("other.db.pre-v1-to-v2-x.bak").exists());
        assert!(dir.path().join("datum.db.bak").exists());

        // Idempotent once the directory is inside the keep window.
        assert!(prune_pre_migration_backups(&path).is_empty());
    }
}
