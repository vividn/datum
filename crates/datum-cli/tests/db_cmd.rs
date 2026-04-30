//! `datum db purge` integration tests (da-e16).
//!
//! Covers the new verb's three confirmation paths: `--yes` (no prompt),
//! interactive (path match / mismatch), and non-TTY without `--yes`. Also
//! verifies the deprecated `setup --force-recreate` alias keeps working.

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

/// Mirrors da-9u9.1's `setup_force_recreate_recovers_corrupt_db` — the
/// new verb must inherit the escape-hatch contract since it's the
/// canonical replacement.
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
    // Data must be intact — abort happens before force_recreate_schema.
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

/// `datum setup --force-recreate` is deprecated but must keep working
/// until removal in a future minor release. The integration test in
/// `info_setup_cmd.rs` already asserts the data-wiping behavior; this
/// test exists to pin the deprecation contract from the new verb's
/// vantage.
#[test]
fn deprecated_setup_force_recreate_still_dispatches() {
    let db = fresh_db();
    entry_builder("coffee")
        .value(serde_json::json!(1))
        .insert(&db);
    run_cli(&["setup", "--force-recreate"], &db).expect("deprecated path still works");
    assert_eq!(db.count_entries().unwrap(), 0);
}
