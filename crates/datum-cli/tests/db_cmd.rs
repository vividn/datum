//! `datum db purge` integration tests (da-e16).
//!
//! Covers the verb's three confirmation paths: `--yes` (no prompt),
//! interactive (path match / mismatch), and non-TTY without `--yes`.

use crate::common;

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
        .state(serde_json::json!("happy"))
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
        .state(serde_json::json!("blue"))
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
                r#"{"field":"doomed","verb":"occur","subfields":{}}"#,
            ],
        )
        .unwrap();

    run_cli(&["db", "purge", "--yes"], &db).expect("db purge --yes on corrupt DB");
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION),
    );
    // The half-shape table is gone and the current shape is laid down:
    // specs are entries now (da-lc26c), so there is no `field_specs`.
    let field_specs: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'field_specs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(field_specs, 0);
    run_cli(&["setup", "mood", "--verb", "switch"], &db).expect("a spec writes");
    assert!(db.load_spec("mood").unwrap().is_some());
}

/// Interactive: typing the database path proceeds with the purge. In-memory
/// DB's `pragma_database_list` reports an empty path, so a bare newline
/// (trims to "") matches.
#[test]
fn db_purge_interactive_path_match_proceeds() {
    let db = fresh_db();
    entry_builder("coffee")
        .state(serde_json::json!(2))
        .insert(&db);
    let mut stdin = Cursor::new(b"\n".to_vec());
    // In-memory DB → pragma path is "" → bare Enter (trims to "") matches.
    // The confirm token is supplied by `purge`'s wiring; tests pass it
    // directly via `purge_with_io` (da-5df1 added the parameter).
    purge_with_io(DbPurgeArgs::default(), &db, &mut stdin, true, "")
        .expect("matching path should proceed");
    assert_eq!(db.count_entries().unwrap(), 0);
}

#[test]
fn db_purge_interactive_path_mismatch_aborts() {
    let db = fresh_db();
    entry_builder("coffee")
        .state(serde_json::json!(2))
        .insert(&db);
    let mut stdin = Cursor::new(b"not-the-path\n".to_vec());
    let err = purge_with_io(DbPurgeArgs::default(), &db, &mut stdin, true, "")
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
        .state(serde_json::json!(2))
        .insert(&db);
    let mut stdin = Cursor::new(Vec::<u8>::new());
    let err = purge_with_io(DbPurgeArgs::default(), &db, &mut stdin, false, "")
        .expect_err("non-tty without --yes must abort");
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("--yes") && (msg.contains("tty") || msg.contains("non-interactive")),
        "expected tty / --yes message, got: {err}"
    );
    assert_eq!(db.count_entries().unwrap(), 1);
}

/// `datum db setup` is gone (da-a4ptr): `db migrate` applies what is
/// pending, so «setup» means the field command alone.
#[test]
fn db_setup_is_an_unknown_subcommand() {
    let db = fresh_db();
    entry_builder("coffee")
        .state(serde_json::json!(1))
        .insert(&db);
    let err = run_cli(&["db", "setup"], &db).expect_err("db setup is gone");
    let msg = err.to_string();
    assert!(
        msg.contains("unknown db subcommand: 'setup'"),
        "expected unknown-subcommand error, got: {msg}"
    );
    // Data is intact — the rejection happens at parse time.
    assert_eq!(db.count_entries().unwrap(), 1);
}

// ─── da-5df1: purge accepts the bare name when DB was name-resolved ─────────

/// When the user invokes with `--db <name>`, the confirmation prompt
/// accepts the bare name (no full path required). The wiring threads
/// the resolved-by-name token through `purge_with_io`'s `confirm`
/// parameter; here we drive that directly with `confirm = "vremya"`.
#[test]
fn db_purge_accepts_bare_name_confirmation_when_name_resolved() {
    let db = fresh_db();
    entry_builder("coffee")
        .state(serde_json::json!(2))
        .insert(&db);
    let mut stdin = Cursor::new(b"vremya\n".to_vec());
    purge_with_io(DbPurgeArgs::default(), &db, &mut stdin, true, "vremya")
        .expect("bare-name confirmation must proceed when it matches");
    assert_eq!(db.count_entries().unwrap(), 0);
}

/// Mismatched bare name still aborts — the prompt is "type this token";
/// anything else fails. The abort message echoes the expected token so
/// the user sees what they should have typed.
#[test]
fn db_purge_aborts_when_bare_name_mismatched() {
    let db = fresh_db();
    entry_builder("coffee")
        .state(serde_json::json!(2))
        .insert(&db);
    let mut stdin = Cursor::new(b"oops\n".to_vec());
    let err = purge_with_io(DbPurgeArgs::default(), &db, &mut stdin, true, "vremya")
        .expect_err("mismatched bare name must abort");
    let msg = err.to_string();
    assert!(
        msg.contains("vremya"),
        "abort message should echo the expected token, got: {msg}"
    );
    assert_eq!(db.count_entries().unwrap(), 1, "data must survive abort");
}

/// A purge that fails part way leaves the file as it was (da-dfmg8). One
/// corrupt page of `ops` is enough to fail it — DROP walks every page of a
/// table — and dropped one table at a time it had dropped `_meta` and
/// `entries` first, so the next open read the file as schema v7 and
/// refused it: «refusing to migrate schema v7→v59 … malformed», vividn's
/// datum.day on 2026-10-12.
#[test]
fn db_purge_that_fails_leaves_the_file_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("datum.db");
    let path = path.to_str().unwrap();
    {
        let db = Db::open(path).unwrap();
        for i in 0..200 {
            entry_builder("coffee")
                .state(serde_json::json!(i))
                .extra_data(serde_json::json!({ "note": "x".repeat(200) }))
                .insert(&db);
        }
        db.conn()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
    }
    // A leaf of `ops`: the first child of its root, an interior page.
    let mut bytes = std::fs::read(path).unwrap();
    let page = u16::from_be_bytes([bytes[16], bytes[17]]) as usize;
    let root: usize = Db::open(path)
        .unwrap()
        .conn()
        .query_row(
            "SELECT rootpage FROM sqlite_master WHERE name = 'ops'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let at = (root - 1) * page;
    assert_eq!(bytes[at], 0x05, "ops's root is an interior page");
    let cell = u16::from_be_bytes([bytes[at + 12], bytes[at + 13]]) as usize;
    let child = (root - 1) * page + cell;
    let leaf = u32::from_be_bytes(bytes[child..child + 4].try_into().unwrap()) as usize;
    bytes[(leaf - 1) * page..(leaf - 1) * page + 8].fill(0xff);
    std::fs::write(path, bytes).unwrap();

    let db = Db::open(path).unwrap();
    let err = run_cli(&["db", "purge", "--yes"], &db).expect_err("a DROP meets the corrupt page");
    assert!(format!("{err:#}").contains("malformed"), "{err:#}");
    drop(db);

    let db = Db::open(path).expect("the file opens as it did before the purge");
    assert_eq!(
        read_schema_version(db.conn()).unwrap(),
        Some(CURRENT_SCHEMA_VERSION)
    );
    assert_eq!(db.count_entries().unwrap(), 200);
}
