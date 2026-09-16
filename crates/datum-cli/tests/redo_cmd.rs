//! da-s6no — `datum redo`, the inverse of `undo` on the history trail.
//!
//! Nothing undone is thrown away: the trail keeps every version, and `undo`
//! writing an `Undo` row is what tells `redo` there is a step ahead to take.
//! These tests hold the pair to being actual inverses — `undo; redo` puts the
//! entry back where it was, and a `redo` with nothing ahead says so instead of
//! oscillating.

mod common;

use common::run_cli;
use datum_core::history::ChangeKind;
use datum_core::test_util::fresh_db;

/// The kinds on an entry's trail, oldest first.
fn kinds(db: &datum_core::db::Db, hid: &str) -> Vec<ChangeKind> {
    datum_core::history::list(db.conn(), hid, false)
        .unwrap()
        .into_iter()
        .map(|v| v.change_kind)
        .collect()
}

fn only_hid(db: &datum_core::db::Db) -> String {
    // The row the test LOGGED: the TRACK document that declares its field
    // on the first use (da-c8mh) is not one of them.
    let entries = datum_core::test_util::logged_all(db);
    assert_eq!(entries.len(), 1, "expected exactly one row");
    entries[0].hid.to_string()
}

#[test]
fn redo_reapplies_the_version_undo_stepped_back_from() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "cups=2"], &db).expect("create");
    run_cli(&["update", "_", "cups=3"], &db).expect("update");
    let hid = only_hid(&db);

    let before_undo = db.get_entry(&hid).unwrap();
    assert_eq!(before_undo.data.as_ref().unwrap()["cups"], 3);

    run_cli(&["undo"], &db).expect("undo");
    assert_eq!(
        db.get_entry(&hid).unwrap().data.as_ref().unwrap()["cups"],
        2
    );

    run_cli(&["redo"], &db).expect("redo");
    let after_redo = db.get_entry(&hid).unwrap();

    // The whole row, not just the key the update touched: a revert applies the
    // snapshot wholesale, including `modify_time`, so the round trip is exact.
    assert_eq!(after_redo.data, before_undo.data);
    assert_eq!(after_redo.state, before_undo.state);
    assert_eq!(after_redo.slug, before_undo.slug);
    assert_eq!(after_redo.modify_time, before_undo.modify_time);
    assert_eq!(after_redo.content_hash, before_undo.content_hash);

    assert_eq!(
        kinds(&db, &hid),
        vec![
            ChangeKind::Create,
            ChangeKind::UserEdit,
            ChangeKind::Undo,
            ChangeKind::Redo
        ],
        "each step is labelled with the direction it went"
    );
}

#[test]
fn a_second_redo_has_nothing_ahead_of_it() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "cups=2"], &db).expect("create");
    run_cli(&["update", "_", "cups=3"], &db).expect("update");
    run_cli(&["undo"], &db).expect("undo");
    run_cli(&["redo"], &db).expect("redo");

    let hid = only_hid(&db);
    let before = db.get_entry(&hid).unwrap();

    let err = run_cli(&["redo"], &db).expect_err("a second redo must fail");
    assert!(
        err.to_string().contains("Nothing to redo"),
        "the refusal must say what is missing, got: {err}"
    );

    // And it must not have moved anything on the way to failing.
    assert_eq!(db.get_entry(&hid).unwrap().data, before.data);
    assert_eq!(kinds(&db, &hid).len(), 4, "no row written by a failed redo");
}

#[test]
fn redo_before_any_undo_refuses() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).expect("create");
    let err = run_cli(&["redo"], &db).expect_err("nothing has been undone");
    assert!(err.to_string().contains("Nothing to redo"), "got: {err}");
}

#[test]
fn redo_of_an_undone_create_brings_the_same_hid_back() {
    let db = fresh_db();
    run_cli(&["occur", "tea"], &db).expect("create");
    let hid = only_hid(&db);

    run_cli(&["undo"], &db).expect("undo the create");
    assert!(
        db.get_entry_including_deleted(&hid)
            .unwrap()
            .deleted_at
            .is_some(),
        "undoing a create tombstones it"
    );

    run_cli(&["redo"], &db).expect("redo the create");
    let back = db.get_entry(&hid).expect("the entry is live again");
    assert_eq!(back.hid.to_string(), hid, "same hid, not a new entry");
    assert!(back.deleted_at.is_none());
}

#[test]
fn redo_of_an_undone_delete_deletes_again() {
    let db = fresh_db();
    run_cli(&["occur", "tea"], &db).expect("create");
    let hid = only_hid(&db);
    run_cli(&["delete", &hid, "--force"], &db).expect("delete");

    run_cli(&["undo"], &db).expect("undo the delete");
    assert!(db.get_entry(&hid).unwrap().deleted_at.is_none());

    run_cli(&["redo"], &db).expect("redo the delete");
    assert!(
        db.get_entry_including_deleted(&hid)
            .unwrap()
            .deleted_at
            .is_some(),
        "redo of an undone delete deletes again"
    );
}

/// The pair is a toggle, not a one-way door: `undo` after a `redo` walks back
/// to the undone state, and `redo` can then go forward once more.
#[test]
fn undo_and_redo_alternate_without_drifting() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "cups=2"], &db).expect("create");
    run_cli(&["update", "_", "cups=3"], &db).expect("update");
    let hid = only_hid(&db);

    let cups = |db: &datum_core::db::Db| -> i64 {
        db.get_entry(&hid).unwrap().data.as_ref().unwrap()["cups"]
            .as_i64()
            .unwrap()
    };

    for _ in 0..3 {
        run_cli(&["undo"], &db).expect("undo");
        assert_eq!(cups(&db), 2);
        run_cli(&["redo"], &db).expect("redo");
        assert_eq!(cups(&db), 3);
    }
}

/// Trap 1 on the bead: the trail is per entry, so a `_LAST` naming a group
/// steps every row of it — `redo` mirrors `undo` here rather than picking one.
#[test]
fn redo_on_a_multi_entry_last_steps_every_row() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "cups=2"], &db).expect("create coffee");
    let coffee = only_hid(&db);
    run_cli(&["occur", "tea", "cups=2"], &db).expect("create tea");
    let tea = datum_core::test_util::logged_all(&db)
        .into_iter()
        .find(|e| e.field == "tea")
        .unwrap()
        .hid;

    run_cli(&["update", &coffee, "cups=3"], &db).expect("update coffee");
    run_cli(&["update", tea.as_str(), "cups=3"], &db).expect("update tea");

    // Point `_LAST` at both, the way a TRACK pair would (da-c8mh).
    datum_core::last_docs::set_last_docs(&db, &[coffee.clone().into(), tea.clone()]).unwrap();

    run_cli(&["undo"], &db).expect("undo the group");
    for hid in [coffee.as_str(), tea.as_str()] {
        assert_eq!(db.get_entry(hid).unwrap().data.as_ref().unwrap()["cups"], 2);
    }

    run_cli(&["redo"], &db).expect("redo the group");
    for hid in [coffee.as_str(), tea.as_str()] {
        assert_eq!(db.get_entry(hid).unwrap().data.as_ref().unwrap()["cups"], 3);
    }
}

/// `redo <ID>` names its own target, so it works on an entry the pointer has
/// since moved away from.
#[test]
fn redo_accepts_an_explicit_target() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "cups=2"], &db).expect("create coffee");
    let coffee = only_hid(&db);
    run_cli(&["update", &coffee, "cups=3"], &db).expect("update coffee");
    run_cli(&["undo", &coffee], &db).expect("undo coffee");

    // Something else becomes `_LAST`.
    run_cli(&["occur", "tea"], &db).expect("create tea");

    run_cli(&["redo", &coffee], &db).expect("redo coffee by name");
    assert_eq!(
        db.get_entry(&coffee).unwrap().data.as_ref().unwrap()["cups"],
        3
    );
}
