//! Migration bodies for schema versions 1 → 10 — the first era.
//!
//! v1 is the oldest shape this binary can lift: a `sync_state` table with
//! `(device_id, last_seq, updated_at)` and the pre-v2 `entries`. This era
//! rebuilds both, adds the history/conflict tables, and lays down the
//! indexes the read paths still use.

use rusqlite::{params, Connection};

use super::support::*;
use crate::error::Result;

pub(super) fn m1_to_2_sync_state(conn: &Connection) -> Result<()> {
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

pub(super) fn m2_to_3_entries_v2(conn: &Connection) -> Result<()> {
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

pub(super) fn m3_to_4_entry_history(conn: &Connection) -> Result<()> {
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

pub(super) fn m4_to_5_sync_conflicts(conn: &Connection) -> Result<()> {
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

pub(super) fn m5_to_6_live_field_index(conn: &Connection) -> Result<()> {
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

pub(super) fn m6_to_7_sync_state_my_push_seq(conn: &Connection) -> Result<()> {
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

pub(super) fn m7_to_8_entries_user_asserted_last_state(conn: &Connection) -> Result<()> {
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

pub(super) fn m8_to_9_track_ignore_unification(conn: &Connection) -> Result<()> {
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

pub(super) fn m9_to_10_hybrid_time_index(conn: &Connection) -> Result<()> {
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

/// Reverse v9→v10: drop the hybrid-time expression index.
pub(super) fn down_10_to_9_hybrid_time_index(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_hybrid_time;")?;
    Ok(())
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
pub(super) fn down_9_to_8_track_ignore_unification(conn: &Connection) -> Result<()> {
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

/// Reverse v7→v8: drop `entries.user_asserted_last_state` via a table
/// rebuild. The column is metadata-only and is not referenced by any
/// generated column. We use rebuild rather than `ALTER TABLE … DROP
/// COLUMN` so we don't depend on SQLite's stored-DDL re-parser. Any -L
/// assertion values recorded between up and down are discarded.
pub(super) fn down_8_to_7_entries_user_asserted_last_state(conn: &Connection) -> Result<()> {
    rebuild_entries_v3(
        conn, /* with_user_asserted = */ false, /* with_extra_metadata = */ false,
    )
}

/// Reverse v6→v7: drop the `my_push_seq` column from sync_state via a
/// table-rebuild. SQLite's `ALTER TABLE … DROP COLUMN` re-parses the
/// stored CREATE statement and stumbles on inline `--` comments in our
/// schema.sql, so we use the 12-step rebuild pattern instead.
/// Any per-device push-seq counters are lost — but at v6 there were
/// no production pushes that depended on them.
pub(super) fn down_7_to_6_sync_state_my_push_seq(conn: &Connection) -> Result<()> {
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

/// Reverse v5→v6: drop the partial live-field-start index. The base
/// `entries_field_start` index is unaffected.
pub(super) fn down_6_to_5_live_field_index(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS entries_live_field_start;")?;
    Ok(())
}

/// Reverse v4→v5: drop the sync_conflicts table and its indexes.
/// Any pending conflict rows are discarded.
pub(super) fn down_5_to_4_sync_conflicts(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS sync_conflicts_hid;
         DROP INDEX IF EXISTS sync_conflicts_slug;
         DROP INDEX IF EXISTS sync_conflicts_pending;
         DROP TABLE IF EXISTS sync_conflicts;",
    )?;
    Ok(())
}

/// Reverse v3→v4: drop the entry_history table and its indexes.
/// Forward step was purely additive, so the down is a clean schema
/// reversal — but any history rows recorded between the up and down
/// are lost.
pub(super) fn down_4_to_3_entry_history(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entry_history_entry;
         DROP INDEX IF EXISTS entry_history_change_kind;
         DROP TABLE IF EXISTS entry_history;",
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
pub(super) fn down_3_to_2_entries_v2(conn: &Connection) -> Result<()> {
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

/// Reverse v1→v2: restore the v1 sync_state placeholder shape.
/// Drops the C4a-shape table and recreates the (device_id, last_seq,
/// updated_at) placeholder. The v2 table was a no-op for any DB that
/// hadn't synced, so this loses no real data on rollback.
pub(super) fn down_2_to_1_sync_state(conn: &Connection) -> Result<()> {
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
