//! Schema version tracking and forward migrations.
//!
//! Every DB carries a `_meta.schema_version` row that this binary compares
//! against [`CURRENT_SCHEMA_VERSION`]. Forward-compatible migrations
//! (ADD COLUMN, CREATE INDEX IF NOT EXISTS, CREATE TABLE IF NOT EXISTS)
//! run silently on open; anything destructive (column removal, PK change)
//! must go through `datum db purge` or a proper migration entry here.
//!
//! ## Layout
//!
//! `migrations.rs` used to hold the registry, every migration body, the
//! runner and the tests in one file, and it grew by a block per schema
//! version — the LOC cap became a countdown rather than a limit. The
//! shape here is meant to stop that:
//!
//! * this file is the **registry** — the ordered index of version steps,
//!   one entry per migration, and nothing else;
//! * [`engine`] is the **runner** — it walks a registry and does not
//!   change when a migration is added;
//! * [`support`] holds the introspection and table-rebuild helpers used
//!   by more than one era;
//! * `v01_v10`, `v11_v20`, `v21_v30`, `v31_v40` hold the migration
//!   **bodies**, one module per decade of schema versions.
//!
//! Adding a migration therefore appends one entry here and one function
//! to the current era module — and when the version crosses the next
//! multiple of ten, a *new* era file. No file grows without bound.

use rusqlite::Connection;

use crate::error::Result;

mod engine;
mod support;
mod v01_v10;
mod v11_v20;
mod v21_v30;
mod v31_v40;

#[cfg(test)]
mod tests;

pub use engine::*;

use v01_v10::*;
use v11_v20::*;
use v21_v30::*;
use v31_v40::*;

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
