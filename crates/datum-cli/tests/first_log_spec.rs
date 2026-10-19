//! A field's last live entry takes a still-bare spec with it, whatever
//! removed it — `delete`, `undo`, `redo` — in that same change, so the
//! first log and its spec are undone and redone together (da-elv6d).
//! `_LAST` stays the entry: a bare `get` or `update` after a first log
//! would otherwise reach the spec too.
//!
//! vividn, 2026-09-29: «I thought I specified that both the created spec and
//! the first document should appear in _LAST, so that they would be undone
//! together», and «deleting the last entry of something should probably
//! propagate and remove the spec».

use crate::common;

use common::run_cli;
use datum_core::assert_count;
use datum_core::db::Db;
use datum_core::test_util::{fresh_db, logged_entries};

fn listed(db: &Db, field: &str) -> bool {
    db.spec_fields().unwrap().iter().any(|f| f == field)
}

/// Every path that declares a field: `undo` — and `delete _LAST` — take
/// the entry and its spec.
#[test]
fn a_first_log_and_its_spec_go_together() {
    let firsts: [&[&str]; 5] = [
        &["occur", "x"],
        &["start", "x"],
        &["switch", "x", "happy"],
        &["note", "x", "-T", "hi"],
        &["occur", "x", ",", "occur", "y"],
    ];
    for argv in firsts {
        for then in ["undo", "delete"] {
            let db = fresh_db();
            run_cli(argv, &db).unwrap();
            assert!(
                db.load_spec("x").unwrap().is_some(),
                "{argv:?} writes a spec"
            );
            let then: &[&str] = match then {
                "undo" => &["undo"],
                _ => &["delete", "_LAST"],
            };
            run_cli(then, &db).unwrap();
            for field in ["x", "y"] {
                assert_count!(db, field, 0);
                assert!(
                    db.load_spec(field).unwrap().is_none(),
                    "{argv:?} then {then:?}"
                );
                assert!(!listed(&db, field), "{argv:?} then {then:?}");
            }
        }
    }
}

/// `_` is the most recent live entry, not `_LAST` — the spec goes because
/// that entry was the field's only one.
#[test]
fn deleting_the_only_entry_by_underscore_takes_the_spec() {
    let db = fresh_db();
    run_cli(&["occur", "x"], &db).unwrap();
    run_cli(&["delete", "_"], &db).unwrap();
    assert!(db.load_spec("x").unwrap().is_none());
    assert!(!listed(&db, "x"));
}

/// The spec outlives every entry but the last, and one `undo` brings the
/// last entry and the spec back together.
#[test]
fn the_last_live_entry_takes_the_spec_and_undo_returns_both() {
    let db = fresh_db();
    run_cli(&["occur", "x", "-t", "1000"], &db).unwrap();
    run_cli(&["occur", "x", "-t", "1100"], &db).unwrap();
    let hids: Vec<String> = logged_entries(&db, "x")
        .into_iter()
        .map(|e| e.hid.0)
        .collect();

    run_cli(&["delete", &hids[0]], &db).unwrap();
    assert!(db.load_spec("x").unwrap().is_some(), "one entry is left");

    run_cli(&["delete", &hids[1]], &db).unwrap();
    assert!(db.load_spec("x").unwrap().is_none(), "the last one took it");

    run_cli(&["undo"], &db).unwrap();
    assert_count!(db, "x", 1);
    assert!(
        db.load_spec("x").unwrap().is_some(),
        "undo brings both back"
    );
}

/// A spec someone shaped is a field with no entries, not a leftover.
#[test]
fn a_shaped_spec_survives_its_last_entry() {
    let db = fresh_db();
    run_cli(&["occur", "x"], &db).unwrap();
    run_cli(&["setup", "x", "--data-key", "size"], &db).unwrap();
    run_cli(&["delete", "_"], &db).unwrap();
    assert_count!(db, "x", 0);
    assert!(db.load_spec("x").unwrap().is_some());
    assert!(listed(&db, "x"));
}

/// A kit's field stays declared, even when its spec says nothing a first
/// log would not.
#[test]
fn a_kit_declared_spec_survives_its_last_entry() {
    let db = fresh_db();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("k.toml");
    std::fs::write(
        &path,
        "[kit]\nname = \"k\"\n\n[[field]]\nname = \"x\"\nverb = \"occur\"\n",
    )
    .unwrap();
    run_cli(&["kit", "install", &path.to_string_lossy(), "--yes"], &db).unwrap();
    assert!(
        db.load_spec("x").unwrap().unwrap().is_bare(),
        "the test's premise"
    );
    run_cli(&["occur", "x"], &db).unwrap();
    run_cli(&["delete", "_"], &db).unwrap();
    assert!(db.load_spec("x").unwrap().is_some());
}

/// `redo` steps the pair forward as `undo` stepped it back, and a second
/// `undo` takes both again.
#[test]
fn undo_and_redo_move_the_first_log_and_its_spec_together() {
    let db = fresh_db();
    run_cli(&["occur", "x"], &db).unwrap();
    run_cli(&["undo"], &db).unwrap();
    run_cli(&["redo"], &db).unwrap();
    assert_count!(db, "x", 1);
    assert!(
        db.load_spec("x").unwrap().is_some(),
        "redo brings both back"
    );
    run_cli(&["undo"], &db).unwrap();
    assert_count!(db, "x", 0);
    assert!(
        db.load_spec("x").unwrap().is_none(),
        "undo takes both again"
    );
}

/// Once a step brings the pair back, `_LAST` is the entry alone again: a
/// bare `get` reads one row and a bare `update` leaves the spec alone.
#[test]
fn a_revived_pair_leaves_the_spec_out_of_last() {
    let revivals: [&[&[&str]]; 2] = [&[&["delete", "_"], &["undo"]], &[&["undo"], &["redo"]]];
    for steps in revivals {
        let db = fresh_db();
        run_cli(&["occur", "x"], &db).unwrap();
        for step in steps {
            run_cli(step, &db).unwrap();
        }
        assert!(db.load_spec("x").unwrap().is_some(), "{steps:?}");
        run_cli(&["get", "::hid"], &db).expect("`_LAST` is one row");
        run_cli(&["update", "n=1"], &db).unwrap();
        assert!(db.load_spec("x").unwrap().unwrap().is_bare(), "{steps:?}");
        assert_eq!(logged_entries(&db, "x")[0].data.as_ref().unwrap()["n"], 1);
    }
}
