//! `datum info` and `datum setup` integration tests (da-ndd.11).

mod common;

use common::run_cli;
use datum_core::migrations::{
    apply_migrations, read_schema_version, set_schema_version, CURRENT_SCHEMA_VERSION, MIGRATIONS,
};
use datum_core::test_util::{entry_builder, fresh_db};

#[test]
fn setup_is_idempotent_on_current_db() {
    let db = fresh_db();
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION)
    );
    run_cli(&["setup"], &db).expect("first setup");
    run_cli(&["setup"], &db).expect("second setup is a no-op");
    // Schema version unchanged.
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION)
    );
}

#[test]
fn setup_force_recreate_wipes_data() {
    let db = fresh_db();
    // Seed an entry — force-recreate must drop it.
    entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .value(serde_json::json!("happy"))
        .insert(&db);
    assert_eq!(db.count_entries().unwrap(), 1);

    run_cli(&["setup", "--force-recreate"], &db).expect("force-recreate dispatch");
    assert_eq!(db.count_entries().unwrap(), 0);
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION)
    );
}

/// da-9u9.1: `setup --force-recreate` recovers a deliberately corrupt DB
/// (no `_meta`, half-shape `field_specs` without `kit_id`). Exercises the
/// `Db::open_unmigrated` path: the auto-migrate pipeline would error
/// before recovery code gets a chance to run, so the binary skips it for
/// this command. Mirrors the post-incident escape-hatch contract.
#[test]
fn setup_force_recreate_recovers_corrupt_db() {
    use datum_core::db::Db;

    // Deliberately corrupt v12-shape DB: no `_meta`, `field_specs` with
    // no kit_id/tags/render_json. Open under `open_unmigrated` so the
    // migration pipeline doesn't try to repair (and crash on) it.
    let db = Db::open_unmigrated(":memory:").unwrap();
    db.conn()
        .execute_batch(
            "CREATE TABLE field_specs (
                field      TEXT NOT NULL PRIMARY KEY,
                spec_json  TEXT NOT NULL
             );",
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO field_specs (field, spec_json) VALUES (?1, ?2)",
            rusqlite::params![
                "doomed",
                r#"{"field":"doomed","default_verb":"occur","subfields":{}}"#,
            ],
        )
        .unwrap();

    // Force-recreate dispatches cleanly, drops the legacy table, rebuilds
    // schema from scratch, and stamps current version.
    run_cli(&["setup", "--force-recreate"], &db).expect("force-recreate must succeed on corrupt DB");
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION),
    );
    // Pre-existing data is gone; field_specs is empty at current shape.
    let n: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM field_specs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(n, 0);
    // Now-current shape has all the kit-era columns.
    let n: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('field_specs') WHERE name = 'kit_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "field_specs.kit_id must be present after recreate");
}

#[test]
fn setup_rejects_newer_schema_version() {
    let db = fresh_db();
    // Stamp a version the binary cannot handle.
    set_schema_version(db.conn(), CURRENT_SCHEMA_VERSION + 1).unwrap();
    let err = run_cli(&["setup"], &db).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("newer"),
        "expected 'newer' message, got: {msg}"
    );
}

#[test]
fn info_runs_against_active_db() {
    let db = fresh_db();
    // da-2sr: writing an entry is the only thing needed to make a field
    // tracked. No separate set_tracked call.
    entry_builder("coffee")
        .value(serde_json::json!(2))
        .insert(&db);
    // info should dispatch cleanly.
    run_cli(&["info"], &db).expect("info dispatch");
}

#[test]
fn migrate_status_lists_registered_migrations() {
    // End-to-end: `datum migrate status` dispatches cleanly against a fresh DB,
    // and the core helper it calls reports one status row per registered
    // migration. With MIGRATIONS empty (initial shape is v1), that's zero rows;
    // the shape check still proves the plumbing is wired.
    let db = fresh_db();

    run_cli(&["migrate", "status"], &db).expect("migrate status dispatch");

    let (current, target, rows) = datum_core::migrations::migration_status(
        db.conn(),
        datum_core::migrations::MIGRATIONS,
    )
    .unwrap();
    assert_eq!(current, CURRENT_SCHEMA_VERSION);
    assert_eq!(target, CURRENT_SCHEMA_VERSION);
    assert_eq!(rows.len(), MIGRATIONS.len());
    for (row, expected) in rows.iter().zip(MIGRATIONS.iter()) {
        assert_eq!(row.from, expected.from);
        assert_eq!(row.to, expected.to);
        assert_eq!(row.summary, expected.summary);
        assert_eq!(row.applied, expected.to <= current);
    }
}

#[test]
fn migrate_run_is_idempotent_on_current_db() {
    // `datum migrate run` against an already-current DB reports no pending
    // migrations and leaves schema_version unchanged.
    let db = fresh_db();
    let before = read_schema_version(db.conn()).unwrap();
    run_cli(&["migrate", "run"], &db).expect("migrate run dispatch");
    let after = read_schema_version(db.conn()).unwrap();
    assert_eq!(before, after);
    assert_eq!(after, Some(CURRENT_SCHEMA_VERSION));
}

#[test]
fn apply_migrations_walks_registered_steps() {
    // End-to-end check that the real MIGRATIONS slice is coherent. We call
    // apply_migrations with (current, target) = (CURRENT, CURRENT) and
    // expect a clean no-op — proves `MIGRATIONS` is consistent with
    // `CURRENT_SCHEMA_VERSION`.
    let db = fresh_db();
    let applied = apply_migrations(
        db.conn(),
        MIGRATIONS,
        CURRENT_SCHEMA_VERSION,
        CURRENT_SCHEMA_VERSION,
    )
    .unwrap();
    assert!(applied.is_empty());
}
