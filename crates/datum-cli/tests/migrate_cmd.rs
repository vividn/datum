//! `datum migrate` integration tests (da-glnr).
//!
//! Ports the five cases from datum's `migrateOne.test.ts`, adapted to
//! daydumb's schema-migration framework. Datum's `migrateOne` operated on
//! individual CouchDB documents (update / overwrite / delete / merge /
//! error-on-unknown-op); daydumb's equivalent is the schema-migration
//! runner, so the ported tests preserve the test *intent* the bead calls
//! out — idempotency, schema-version advancement, and error-on-unknown-
//! migration — and exercise them through the `datum migrate` command
//! surface using the P0 harness.
//!
//! C7's `#[cfg(test)] mod` in `crates/datum-core/src/migrations.rs`
//! already covers framework internals; the tests here stay at the CLI
//! boundary where possible and use synthetic migration slices only when
//! the live `MIGRATIONS` registry (empty at v1) cannot exercise the
//! behavior being verified.

mod common;

use common::run_cli;
use datum_core::migrations::{
    apply_migrations, migration_status, read_schema_version, set_schema_version, Migration,
    CURRENT_SCHEMA_VERSION,
};
use datum_core::test_util::fresh_db;
use rusqlite::Connection;

// Synthetic migration steps used whenever we need a non-empty registry.
// Each step just creates a marker table so the tests can prove the apply
// function actually ran (or didn't).
fn synth_v1_to_v2(conn: &Connection) -> datum_core::error::Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS synth_v2 (x INTEGER);")?;
    Ok(())
}
fn synth_v2_to_v3(conn: &Connection) -> datum_core::error::Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS synth_v3 (x INTEGER);")?;
    Ok(())
}
fn synth_down_v2_to_v1(conn: &Connection) -> datum_core::error::Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS synth_v2;")?;
    Ok(())
}
fn synth_down_v3_to_v2(conn: &Connection) -> datum_core::error::Result<()> {
    conn.execute_batch("DROP TABLE IF EXISTS synth_v3;")?;
    Ok(())
}

const SYNTH: &[Migration] = &[
    Migration {
        from: 1,
        to: 2,
        summary: "synth v1→v2",
        apply: synth_v1_to_v2,
        down: synth_down_v2_to_v1,
    },
    Migration {
        from: 2,
        to: 3,
        summary: "synth v2→v3",
        apply: synth_v2_to_v3,
        down: synth_down_v3_to_v2,
    },
];

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        rusqlite::params![name],
        |row| row.get::<_, i64>(0).map(|n| n > 0),
    )
    .unwrap_or(false)
}

// ─── Port 1: migrateOne "update" ─────────────────────────────────────
// Datum intent: a migration row transforms the stored doc.
// Daydumb intent: a registered migration applies, bumps schema_version,
// and its side-effect is visible afterwards.
#[test]
fn migrate_applies_pending_step_and_advances_version() {
    let db = fresh_db();
    set_schema_version(db.conn(), 1).unwrap();

    let applied = apply_migrations(db.conn(), SYNTH, 1, 2).expect("apply v1→v2");
    assert_eq!(applied.len(), 1, "exactly one step should run");
    assert_eq!(read_schema_version(db.conn()).unwrap(), Some(2));
    assert!(
        table_exists(db.conn(), "synth_v2"),
        "v1→v2 side-effect must be present"
    );
}

// ─── Port 2: migrateOne "overwrite" ──────────────────────────────────
// Datum intent: a row can replace the entire stored doc.
// Daydumb intent: multiple pending migrations walk in order and the end
// state fully reflects every applied step.
#[test]
fn migrate_walks_multiple_pending_steps_in_order() {
    let db = fresh_db();
    set_schema_version(db.conn(), 1).unwrap();

    let applied = apply_migrations(db.conn(), SYNTH, 1, 3).expect("apply v1→v3");
    assert_eq!(applied.len(), 2, "both synthetic steps should run");
    assert_eq!(read_schema_version(db.conn()).unwrap(), Some(3));
    assert!(table_exists(db.conn(), "synth_v2"));
    assert!(table_exists(db.conn(), "synth_v3"));
}

// ─── Port 3: migrateOne "delete" ─────────────────────────────────────
// Datum intent: after a delete row, the doc is gone and re-checking is safe.
// Daydumb intent: once the schema is at target, `migrate run` is a clean
// no-op — re-dispatch does not advance the version or fail.
#[test]
fn migrate_run_is_idempotent_across_repeated_dispatches() {
    let db = fresh_db();
    // `fresh_db()` already stamps CURRENT_SCHEMA_VERSION; two migrate runs
    // back-to-back must both succeed and leave the version untouched.
    run_cli(&["migrate", "run"], &db).expect("first migrate run");
    run_cli(&["migrate", "run"], &db).expect("second migrate run (idempotent)");
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION)
    );
}

// ─── Port 4: migrateOne "apply an update" (merge) ────────────────────
// Datum intent: a partial transform merges into the existing doc rather
// than replacing it.
// Daydumb intent: from a mid-sequence state, only pending steps run;
// previously-applied steps are not re-executed.
#[test]
fn migrate_executes_only_pending_steps_from_partial_state() {
    let db = fresh_db();
    // Stamp mid-sequence: v1→v2 already "applied" by a prior run; only
    // v2→v3 remains pending.
    set_schema_version(db.conn(), 2).unwrap();

    let applied = apply_migrations(db.conn(), SYNTH, 2, 3).expect("apply v2→v3");
    assert_eq!(applied.len(), 1, "only v2→v3 should execute");
    assert_eq!(read_schema_version(db.conn()).unwrap(), Some(3));
    assert!(
        !table_exists(db.conn(), "synth_v2"),
        "v1→v2 must NOT re-run on a DB already past v2"
    );
    assert!(
        table_exists(db.conn(), "synth_v3"),
        "v2→v3 is the one pending step"
    );
}

// ─── Port 5: migrateOne "throws on unknown operator" ─────────────────
// Datum intent: an unrecognized op surfaces as a typed error.
// Daydumb intent: a DB stamped at a version this binary cannot reach is
// rejected cleanly by `migrate run`.
#[test]
fn migrate_run_errors_when_db_is_newer_than_binary() {
    let db = fresh_db();
    set_schema_version(db.conn(), CURRENT_SCHEMA_VERSION + 5).unwrap();
    let err = run_cli(&["migrate", "run"], &db).unwrap_err();
    let msg = format!("{err}").to_lowercase();
    assert!(
        msg.contains("newer"),
        "expected 'newer' in error message, got: {err}"
    );
}

// ─── Acceptance: `migrate status` surfaces applied vs pending ─────────
#[test]
fn migrate_status_lists_applied_and_pending() {
    let db = fresh_db();
    // Stamp mid-sequence so the SYNTH registry has one applied and one
    // pending row — the discriminator `migrate status` presents to users.
    set_schema_version(db.conn(), 2).unwrap();

    let (current, _target, rows) =
        migration_status(db.conn(), SYNTH).expect("status against synthetic registry");
    assert_eq!(current, 2);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].applied, "v1→v2 must be marked applied at current=2");
    assert!(
        !rows[1].applied,
        "v2→v3 must be marked pending at current=2"
    );

    // And the real CLI dispatch path also runs cleanly against the live
    // (empty) MIGRATIONS registry at any schema version ≤ CURRENT.
    set_schema_version(db.conn(), CURRENT_SCHEMA_VERSION).unwrap();
    run_cli(&["migrate", "status"], &db).expect("migrate status dispatch");
}

// ─── Acceptance: legacy `migrate import --from backup.json` still works
#[test]
fn legacy_backup_json_import_still_works() {
    let db = fresh_db();

    // Minimal datum-compatible backup.json with a single coffee entry.
    let backup = serde_json::json!({
        "backupTime": "2024-03-16T09:20:00Z",
        "docs": [
            {
                "_id": "coffee:2024-03-16T09:15Z",
                "data": {
                    "field": "coffee",
                    "occurTime": {
                        "utc": "2024-03-16T09:15:00Z",
                        "o": 0,
                        "tz": "UTC"
                    }
                },
                "meta": {
                    "createTime": {
                        "utc": "2024-03-16T09:16:00Z",
                        "o": 0,
                        "tz": "UTC"
                    }
                }
            }
        ]
    });
    let tmp = tempfile::NamedTempFile::new().expect("tempfile");
    std::fs::write(tmp.path(), serde_json::to_string(&backup).unwrap()).expect("write backup.json");

    run_cli(
        &["migrate", "import", "--from", tmp.path().to_str().unwrap()],
        &db,
    )
    .expect("legacy backup.json import dispatch");

    let entries = db.entries_for_field("coffee", None).expect("list coffee");
    assert_eq!(entries.len(), 1, "backup.json doc must land as one entry");
    assert_eq!(entries[0].field, "coffee");
}
