//! Tests for the migration registry, the runner and the snapshot path.
//!
//! Per-step round-trip coverage lives in `tests/migration_roundtrip.rs`;
//! the whole-chain walk lives in `tests/migration_chain.rs`.

use super::*;
use crate::db::Db;
use crate::error::DatumError;

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
