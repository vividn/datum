//! Migration bodies for schema versions 20 → 30 — the model-cleanup era.
//!
//! Verb canonicalization, the `field · occur_time` uniqueness rule, the
//! sync_state cluster columns, and the nullable-`occur_time` rebuild that
//! made timeless entries representable.

use rusqlite::Connection;

use super::support::*;
use crate::error::{DatumError, Result};

/// SQL expression deriving epoch-ms from the RFC3339 `modify_time` string.
/// Kept in one place so the generated column (migration + schema.sql) and any
/// future reader agree on the metric. `unixepoch(.., 'subsec')` returns
/// fractional seconds; ×1000 + truncating CAST yields milliseconds. The push
/// pipeline uses this metric for BOTH the watermark filter and the watermark
/// it advances to, so the (sub-millisecond) float deviation from chrono's
/// `timestamp_millis` is self-consistent and cannot skip a row.
const MODIFY_TIME_MS_EXPR: &str = "CAST(unixepoch(modify_time, 'subsec') * 1000 AS INTEGER)";

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
pub(super) fn m20_to_21_drop_derived_from_field(conn: &Connection) -> Result<()> {
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
pub(super) fn m21_to_22_verb_canonicalize(conn: &Connection) -> Result<()> {
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
pub(super) fn m22_to_23_unit_drop_dur_precision_add(conn: &Connection) -> Result<()> {
    let has_unit = has_column(conn, "entries", "unit")?;
    let has_dur_precision = has_column(conn, "entries", "dur_precision")?;
    if !has_unit && has_dur_precision {
        return Ok(());
    }
    rebuild_entries_v23(
        conn, /* with_unit = */ false, /* with_dur_precision = */ true,
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
pub(super) fn m23_to_24_id_field_structure_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS entries_id_structure \
            ON entries (hid) WHERE id_structure IS NOT NULL;
         CREATE INDEX IF NOT EXISTS entries_field_structure \
            ON entries (hid) WHERE field_structure IS NOT NULL;",
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
pub(super) fn m24_to_25_field_occurtime_unique(conn: &Connection) -> Result<()> {
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
pub(super) fn m25_to_26_rename_integrity_warnings(conn: &Connection) -> Result<()> {
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

/// da-eu7o: add per-DB `cluster_id` (nullable until first sync echoes
/// back the relay's assignment) and `cluster_generation` (default 0)
/// columns to `sync_state`. Both `ALTER TABLE ... ADD COLUMN` steps are
/// natively idempotent via an introspection check so replaying this
/// migration is safe.
pub(super) fn m26_to_27_sync_state_cluster_columns(conn: &Connection) -> Result<()> {
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
pub(super) fn m27_to_28_drop_stored_same_state_repeat(conn: &Connection) -> Result<()> {
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

/// da-lxyw: forward 28→29 — drop the `NOT NULL` on `entries.occur_time`
/// so a bare `add` (no `-t`) can be stored timeless (`occur_time` NULL).
/// SQLite has no `ALTER COLUMN`, so rebuild the table preserving every
/// row, then re-derive `verb` is unnecessary — the column shape and data
/// are unchanged apart from the relaxed constraint.
pub(super) fn m28_to_29_occur_time_nullable(conn: &Connection) -> Result<()> {
    rebuild_entries_occur_time(conn, true)
}

/// da-x3da: add a `modify_time_ms` virtual generated column (epoch-ms of the
/// RFC3339 `modify_time`) plus an index, so `select_entries_since` can push
/// its watermark filter into SQL (`WHERE modify_time_ms > ?`) and stop
/// scanning the whole `entries` table on every push.
///
/// VIRTUAL (not STORED) because `ALTER TABLE ADD COLUMN` only permits virtual
/// generated columns; the index materializes the computed values so the range
/// filter still uses a b-tree seek. Additive and idempotent.
pub(super) fn m29_to_30_modify_time_ms_index(conn: &Connection) -> Result<()> {
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

/// Reverse 30→29 — drop the index and the generated column. The index must
/// go first: SQLite refuses to drop a column an index still references.
pub(super) fn down_30_to_29_modify_time_ms_index(conn: &Connection) -> Result<()> {
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

/// Reverse 29→28 — restore `NOT NULL` on `occur_time`. Timeless rows
/// (NULL `occur_time`) created under v29 are backfilled with
/// `create_time` during the rebuild so they satisfy the constraint —
/// matching the pre-da-lxyw stub behaviour that defaulted to create_time.
pub(super) fn down_29_to_28_occur_time_nullable(conn: &Connection) -> Result<()> {
    rebuild_entries_occur_time(conn, false)
}

/// Reverse v27→v28: there is nothing to put back. The stored rows we
/// deleted were stale by construction (the live query at v28 would
/// have ignored them anyway); we have no preserved snapshot to
/// re-insert. The down step is a structural no-op so a downgrade
/// chain can pass through without erroring.
pub(super) fn down_28_to_27_drop_stored_same_state_repeat(_conn: &Connection) -> Result<()> {
    Ok(())
}

/// Reverse v26→v27: drop the cluster_id / cluster_generation columns
/// via a 12-step table rebuild (SQLite's stored-DDL re-parser tripped
/// on inline `--` comments in our schema.sql when we tried `ALTER TABLE
/// ... DROP COLUMN`, mirroring v6→v7's workaround). Any locally-stored
/// cluster_id mapping is discarded — at pre-v1 there are no production
/// clusters that would suffer.
pub(super) fn down_27_to_26_sync_state_cluster_columns(conn: &Connection) -> Result<()> {
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

/// Reverse of [`m25_to_26_rename_integrity_warnings`]. Renames back to
/// the legacy `sync_conflicts` shape so the migration is round-trippable.
pub(super) fn down_26_to_25_rename_integrity_warnings(conn: &Connection) -> Result<()> {
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

/// Reverse of [`m24_to_25_field_occurtime_unique`]. Index-only; no data
/// is touched.
pub(super) fn down_25_to_24_field_occurtime_unique(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_field_occurtime;")?;
    Ok(())
}

/// Reverse of [`m23_to_24_id_field_structure_indexes`]. Index-only
/// changes; no data is touched.
pub(super) fn down_24_to_23_id_field_structure_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_id_structure;
         DROP INDEX IF EXISTS entries_field_structure;",
    )?;
    Ok(())
}

/// Reverse v22→v23: re-add the `unit` column and drop `dur_precision`
/// via table rebuild. Surviving rows have `unit = NULL` — the forward
/// step does not preserve the dropped column's data because the column
/// is vestigial. Pre-v1 hard-cut; this down step exists for the
/// chain-test harness.
pub(super) fn down_23_to_22_unit_drop_dur_precision_add(conn: &Connection) -> Result<()> {
    rebuild_entries_v23(
        conn, /* with_unit = */ true, /* with_dur_precision = */ false,
    )
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
pub(super) fn down_22_to_21_verb_canonicalize(conn: &Connection) -> Result<()> {
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

/// Reverse v20→v21: re-add the `derived_from_field` column to
/// `aliases` via table rebuild. Surviving rows have `derived_from_field
/// = NULL` — the v21 forward step deleted derived-alias rows, so the
/// information needed to re-populate the column does not exist on disk.
/// Pre-v1 hard-cut: this down step exists for the chain-test harness
/// only.
pub(super) fn down_21_to_20_drop_derived_from_field(conn: &Connection) -> Result<()> {
    rebuild_aliases(conn, /* with_derived_from_field = */ true)
}
