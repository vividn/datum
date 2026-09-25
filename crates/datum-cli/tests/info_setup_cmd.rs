//! `datum info` and `datum setup` integration tests (da-ndd.11).

use crate::common;

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
    run_cli(&["db", "setup"], &db).expect("first setup");
    run_cli(&["db", "setup"], &db).expect("second setup is a no-op");
    // Schema version unchanged.
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
    let err = run_cli(&["db", "setup"], &db).unwrap_err();
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
        .state(serde_json::json!(2))
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

    run_cli(&["db", "migrate", "status"], &db).expect("migrate status dispatch");

    let (current, target, rows) =
        datum_core::migrations::migration_status(db.conn(), datum_core::migrations::MIGRATIONS)
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
    run_cli(&["db", "migrate", "run"], &db).expect("migrate run dispatch");
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

/// da-3n9t1: `info` reports on a database without migrating it. One
/// older and one newer than this binary sit in the data directory; `info
/// --all` must leave both files byte for byte and read each version as it
/// is stored — which is the only way either warning row can ever print.
#[test]
fn info_all_reports_without_migrating() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let stamp = |name: &str, version: u32| {
        let path = home.join(name);
        let db = datum_core::db::Db::open(path.to_str().unwrap()).unwrap();
        set_schema_version(db.conn(), version).unwrap();
        drop(db);
        (path.clone(), std::fs::read(&path).unwrap())
    };
    let (old_path, old_bytes) = stamp("old.db", CURRENT_SCHEMA_VERSION - 1);
    let (new_path, new_bytes) = stamp("new.db", CURRENT_SCHEMA_VERSION + 1);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
        .env_remove("DATUM_DB")
        .env("DATUM_HOME", &home)
        .env("DATUM_CONFIG_HOME", tmp.path().join("config"))
        .env("DATUM_COLOR", "0")
        .args(["info", "--all"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");

    assert!(
        std::fs::read(&old_path).unwrap() == old_bytes,
        "old.db rewritten"
    );
    assert!(
        std::fs::read(&new_path).unwrap() == new_bytes,
        "new.db rewritten"
    );
    let old_row = format!(
        "v{} ⚠ (older than current v{CURRENT_SCHEMA_VERSION})",
        CURRENT_SCHEMA_VERSION - 1
    );
    let new_row = format!(
        "v{} ✗ (newer than binary (v{CURRENT_SCHEMA_VERSION}))",
        CURRENT_SCHEMA_VERSION + 1
    );
    assert!(stdout.contains(&old_row), "no {old_row:?} in:\n{stdout}");
    assert!(stdout.contains(&new_row), "no {new_row:?} in:\n{stdout}");
}
