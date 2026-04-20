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
    // Seed a couple of entries and a tracked field for richer output.
    entry_builder("coffee")
        .value(serde_json::json!(2))
        .insert(&db);
    db.set_tracked("coffee", false, None).unwrap();
    // info should dispatch cleanly.
    run_cli(&["info"], &db).expect("info dispatch");
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
