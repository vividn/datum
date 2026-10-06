//! da-z0zh3 — bare `undo` / `redo` walk a stack of commands, as an editor's
//! do: `undo` keeps walking back, `redo` forward, and a new command drops
//! what was undone. Neither is a command on the stack itself.

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::{fresh_db, logged_all};

/// The live logged rows' fields, oldest first.
fn live(db: &Db) -> Vec<String> {
    let mut rows: Vec<_> = logged_all(db)
        .into_iter()
        .filter(|e| e.deleted_at.is_none())
        .collect();
    rows.sort_by_key(|e| e.create_time);
    rows.into_iter().map(|e| e.field).collect()
}

fn hid_of(db: &Db, field: &str) -> String {
    logged_all(db)
        .into_iter()
        .find(|e| e.field == field)
        .unwrap()
        .hid
        .to_string()
}

#[test]
fn undo_walks_back_and_redo_walks_forward() {
    let db = fresh_db();
    run_cli(&["occur", "alpha"], &db).unwrap();
    run_cli(&["occur", "beta"], &db).unwrap();

    run_cli(&["undo"], &db).unwrap();
    assert_eq!(live(&db), ["alpha"]);
    run_cli(&["undo"], &db).unwrap();
    assert!(
        live(&db).is_empty(),
        "the second undo walks back past alpha"
    );
    let err = run_cli(&["undo"], &db).unwrap_err();
    assert!(err.to_string().contains("Nothing to undo"), "{err}");

    run_cli(&["redo"], &db).unwrap();
    assert_eq!(live(&db), ["alpha"]);
    run_cli(&["redo"], &db).unwrap();
    assert_eq!(live(&db), ["alpha", "beta"]);
    let err = run_cli(&["redo"], &db).unwrap_err();
    assert!(err.to_string().contains("Nothing to redo"), "{err}");
}

#[test]
fn a_new_command_after_an_undo_drops_the_redo_tail() {
    let db = fresh_db();
    run_cli(&["occur", "alpha"], &db).unwrap();
    run_cli(&["undo"], &db).unwrap();
    run_cli(&["occur", "gamma"], &db).unwrap();

    let err = run_cli(&["redo"], &db).unwrap_err();
    assert!(err.to_string().contains("Nothing to redo"), "{err}");
    run_cli(&["undo"], &db).unwrap();
    assert!(live(&db).is_empty());
    let err = run_cli(&["undo"], &db).unwrap_err();
    assert!(
        err.to_string().contains("Nothing to undo"),
        "alpha's undone command went with the tail: {err}"
    );
}

#[test]
fn a_command_over_two_entries_undoes_as_one() {
    let db = fresh_db();
    run_cli(&["occur", "alpha", "-t", "10:00"], &db).unwrap();
    run_cli(&["occur", "beta", "-t", "11:00"], &db).unwrap();
    let (a, b) = (hid_of(&db, "alpha"), hid_of(&db, "beta"));
    let time = |hid: &str| db.get_entry(hid).unwrap().occur_time.unwrap().utc;
    let before = (time(&a), time(&b));

    run_cli(&["retime", &format!("{a},{b}"), "-5"], &db).unwrap();
    assert_ne!((time(&a), time(&b)), before);

    run_cli(&["undo"], &db).unwrap();
    assert_eq!((time(&a), time(&b)), before, "one undo moves both back");
    assert_eq!(live(&db), ["alpha", "beta"]);

    run_cli(&["undo"], &db).unwrap();
    assert_eq!(live(&db), ["alpha"], "and the next undo is beta's log");
}

/// A read never moves the stack: `get a` then `undo` reverts the last WRITE.
#[test]
fn undo_after_a_get_reverts_the_last_write() {
    let db = fresh_db();
    run_cli(&["occur", "a"], &db).unwrap();
    run_cli(&["occur", "b"], &db).unwrap();
    let a = hid_of(&db, "a");
    run_cli(&["get", &a], &db).unwrap();

    let mut undo: Vec<String> = datum_core::ids::resolve::resolve_quickid(&db, "_UNDO")
        .unwrap()
        .into_iter()
        .map(|e| e.field)
        .collect();
    undo.sort();
    // Exactly b's command: the log, and the `spec:b` its first log declared.
    assert_eq!(
        undo,
        ["b", "spec"],
        "`_UNDO` is the stack's cursor, not `_LAST`"
    );

    run_cli(&["undo"], &db).unwrap();
    assert_eq!(live(&db), ["a"]);
}

/// …and undoing what the `get` showed takes naming it.
#[test]
fn undo_last_after_a_get_reverts_what_it_showed() {
    let db = fresh_db();
    run_cli(&["occur", "a"], &db).unwrap();
    run_cli(&["occur", "b"], &db).unwrap();
    run_cli(&["get", &hid_of(&db, "a")], &db).unwrap();

    run_cli(&["undo", "_LAST"], &db).unwrap();
    assert_eq!(live(&db), ["b"]);
}

/// da-pxsa8 — a stack step is one transaction. A member that fails part-way
/// leaves every member, and the cursor, where they were: nothing half-moved,
/// and the same `undo` / `redo` goes through once the failure is gone.
#[test]
fn a_step_that_fails_part_way_moves_nothing() {
    let db = fresh_db();
    run_cli(&["occur", "alpha", "-t", "10:00"], &db).unwrap();
    run_cli(&["occur", "beta", "-t", "11:00"], &db).unwrap();
    let (a, b) = (hid_of(&db, "alpha"), hid_of(&db, "beta"));
    let time = |hid: &str| db.get_entry(hid).unwrap().occur_time.unwrap().utc;
    let before = (time(&a), time(&b));
    run_cli(&["retime", &format!("{a},{b}"), "-5"], &db).unwrap();
    let after = (time(&a), time(&b));

    // Whichever member the step reaches second refuses to be written.
    let jam = |hid: &str| {
        db.conn()
            .execute_batch(&format!(
                "CREATE TRIGGER jam BEFORE UPDATE ON entries WHEN OLD.hid = '{hid}'
                 BEGIN SELECT RAISE(ABORT, 'jammed'); END;"
            ))
            .unwrap()
    };
    let unjam = || db.conn().execute_batch("DROP TRIGGER jam").unwrap();
    for jammed in [&a, &b] {
        for (verb, still, moved) in [("undo", after, before), ("redo", before, after)] {
            jam(jammed);
            let err = run_cli(&[verb], &db).unwrap_err();
            assert!(format!("{err:#}").contains("jammed"), "{err:#}");
            assert_eq!((time(&a), time(&b)), still, "a failed {verb} moves nothing");
            unjam();
            run_cli(&[verb], &db).unwrap();
            assert_eq!(
                (time(&a), time(&b)),
                moved,
                "and the cursor stayed: the same {verb} goes through"
            );
        }
    }
}
