//! `datum db purge` integration tests (da-e16).
//!
//! Covers the verb's three confirmation paths: `--yes` (no prompt),
//! interactive (path match / mismatch), and non-TTY without `--yes`.

mod common;

use common::run_cli;
use datum_commands::args::DbPurgeArgs;
use datum_commands::commands::db::purge_with_io;
use datum_core::db::Db;
use datum_core::migrations::{read_schema_version, CURRENT_SCHEMA_VERSION};
use datum_core::test_util::{entry_builder, fresh_db};
use std::io::Cursor;

#[test]
fn db_purge_yes_wipes_data() {
    let db = fresh_db();
    entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .value(serde_json::json!("happy"))
        .insert(&db);
    assert_eq!(db.count_entries().unwrap(), 1);

    run_cli(&["db", "purge", "--yes"], &db).expect("db purge --yes dispatch");
    assert_eq!(db.count_entries().unwrap(), 0);
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION)
    );
}

#[test]
fn db_purge_short_yes_flag_wipes_data() {
    let db = fresh_db();
    entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .value(serde_json::json!("blue"))
        .insert(&db);
    run_cli(&["db", "purge", "-y"], &db).expect("db purge -y dispatch");
    assert_eq!(db.count_entries().unwrap(), 0);
}

/// `db purge --yes` must recover a deliberately corrupt DB (no `_meta`,
/// half-shape `field_specs` without `kit_id`). Exercises the
/// `Db::open_unmigrated` path: the auto-migrate pipeline would error
/// before recovery code gets a chance to run, so the binary skips it for
/// this command.
#[test]
fn db_purge_yes_recovers_corrupt_db() {
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

    run_cli(&["db", "purge", "--yes"], &db).expect("db purge --yes on corrupt DB");
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION),
    );
    let kit_id_present: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('field_specs') WHERE name = 'kit_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(kit_id_present, 1);
}

/// Interactive: typing the database path proceeds with the purge. In-memory
/// DB's `pragma_database_list` reports an empty path, so a bare newline
/// (trims to "") matches.
#[test]
fn db_purge_interactive_path_match_proceeds() {
    let db = fresh_db();
    entry_builder("coffee")
        .value(serde_json::json!(2))
        .insert(&db);
    let mut stdin = Cursor::new(b"\n".to_vec());
    purge_with_io(DbPurgeArgs { yes: false }, &db, &mut stdin, true)
        .expect("matching path should proceed");
    assert_eq!(db.count_entries().unwrap(), 0);
}

#[test]
fn db_purge_interactive_path_mismatch_aborts() {
    let db = fresh_db();
    entry_builder("coffee")
        .value(serde_json::json!(2))
        .insert(&db);
    let mut stdin = Cursor::new(b"not-the-path\n".to_vec());
    let err = purge_with_io(DbPurgeArgs { yes: false }, &db, &mut stdin, true)
        .expect_err("mismatched path must abort");
    assert!(
        err.to_string().to_lowercase().contains("did not match"),
        "expected 'did not match' message, got: {err}"
    );
    // Data must be intact — abort happens before the schema rebuild.
    assert_eq!(db.count_entries().unwrap(), 1);
}

#[test]
fn db_purge_non_tty_without_yes_aborts() {
    let db = fresh_db();
    entry_builder("coffee")
        .value(serde_json::json!(2))
        .insert(&db);
    let mut stdin = Cursor::new(Vec::<u8>::new());
    let err = purge_with_io(DbPurgeArgs { yes: false }, &db, &mut stdin, false)
        .expect_err("non-tty without --yes must abort");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("--yes") && (msg.contains("tty") || msg.contains("non-interactive")),
        "expected tty / --yes message, got: {err}"
    );
    assert_eq!(db.count_entries().unwrap(), 1);
}

/// `datum setup --force-recreate` was removed (da-57t); the flag must now
/// be rejected as unknown so users discover `datum db purge` instead.
#[test]
fn setup_force_recreate_flag_is_rejected() {
    let db = fresh_db();
    entry_builder("coffee")
        .value(serde_json::json!(1))
        .insert(&db);
    let err = run_cli(&["setup", "--force-recreate"], &db)
        .expect_err("--force-recreate is no longer accepted");
    let msg = err.to_string();
    assert!(
        msg.contains("--force-recreate") || msg.to_lowercase().contains("unexpected flag"),
        "expected unknown-flag error, got: {msg}"
    );
    // Data is intact — the rejection happens at parse time.
    assert_eq!(db.count_entries().unwrap(), 1);
}
