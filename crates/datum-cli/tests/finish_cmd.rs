//! `datum finish F` (da-udkd3): a FINISHED row, terminal. A new entry dated
//! after it is refused, one before it or with no time is written, and the
//! field is hidden as an ignored one is.

use crate::common;

use chrono::Utc;
use common::run_cli;
use datum_commands::commands::fields::compute_fields;
use datum_core::db::Db;
use datum_core::shape::FINISHED_STATE_VALUE;
use datum_core::test_util::fresh_db;
use serde_json::json;

/// The hid of `field`'s FINISHED row.
fn finish_hid(db: &Db, field: &str) -> String {
    let rows = db.entries_for_field(field, None).unwrap();
    let row = rows
        .iter()
        .find(|e| e.state == Some(json!(FINISHED_STATE_VALUE)))
        .expect("a FINISHED row");
    row.hid.to_string()
}

/// `fields --json`'s row for `field`.
fn listing_of(db: &Db, field: &str) -> serde_json::Value {
    let listing = compute_fields(db).unwrap();
    let row = listing
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["field"] == field);
    row.expect("a row").clone()
}

fn finished_db() -> Db {
    let db = fresh_db();
    run_cli(&["moment", "exp", "-yy", "-t", "10:00"], &db).unwrap();
    run_cli(&["finish", "exp", "-y", "-t", "10:00"], &db).unwrap();
    db
}

#[test]
fn an_entry_after_a_finish_is_refused_saying_how_to_undo_it() {
    let db = finished_db();
    let hid = finish_hid(&db, "exp");
    let err = run_cli(&["moment", "exp", "-y", "-t", "11:00"], &db).unwrap_err();
    let said = format!("{err:#}");
    assert!(said.contains("FINISHED") && said.contains(&hid), "{said}");
    assert!(said.contains(&format!("datum del {hid}")), "{said}");
    // `ignore` is refused; `track` and a second `finish` write nothing.
    run_cli(&["ignore", "exp"], &db).unwrap_err();
    run_cli(&["track", "exp"], &db).unwrap();
    run_cli(&["finish", "exp"], &db).unwrap();
    assert_eq!(db.entries_for_field("exp", None).unwrap().len(), 2);
}

/// da-6iqxu: a field of moments has no off, so its undo is the delete,
/// which leaves no state behind and takes moments again; a field with
/// states, logged or allowed, is told to change the finish's state.
#[test]
fn the_undo_said_is_the_one_that_fits_the_field() {
    let db = finished_db();
    let hid = finish_hid(&db, "exp");
    let advice = |field: &str| {
        let err = run_cli(&["moment", field, "-y", "-t", "11:00"], &db).unwrap_err();
        format!("{err:#}")
    };
    // A remark is a `point` row too, so it gives the field no states.
    run_cli(&["remark", "exp", "a note"], &db).unwrap();
    let said = advice("exp");
    assert!(said.contains(&format!("datum del {hid}")), "{said}");
    run_cli(&["del", &hid], &db).unwrap();
    run_cli(&["moment", "exp", "-y", "-t", "11:00"], &db).unwrap();
    assert_eq!(listing_of(&db, "exp")["states"], json!([]));

    run_cli(&["start", "work", "-yy", "-t", "10:00"], &db).unwrap();
    run_cli(&["finish", "work", "-y", "-t", "10:00"], &db).unwrap();
    let work = finish_hid(&db, "work");
    let said = advice("work");
    assert!(
        said.contains(&format!("datum update {work} state=false")),
        "{said}"
    );

    run_cli(&["setup", "mood", "--add-state", "calm"], &db).unwrap();
    run_cli(&["moment", "mood", "-yy", "-t", "10:00"], &db).unwrap();
    run_cli(&["finish", "mood", "-y", "-t", "10:00"], &db).unwrap();
    let mood = finish_hid(&db, "mood");
    let said = advice("mood");
    assert!(
        said.contains(&format!("datum update {mood} state=false")),
        "{said}"
    );
}

#[test]
fn before_it_untimed_and_the_spec_are_written() {
    let db = finished_db();
    run_cli(&["moment", "exp", "-yy", "-t", "12:00"], &db).unwrap();
    run_cli(&["remark", "exp", "a note"], &db).unwrap();
    run_cli(&["setup", "exp", "--color", "#123456"], &db).unwrap();
    assert_eq!(db.entries_for_field("exp", None).unwrap().len(), 4);
    assert_eq!(
        db.load_spec("exp").unwrap().unwrap().color.as_deref(),
        Some("#123456")
    );
}

#[test]
fn the_finish_edited_to_off_takes_entries_again() {
    let db = finished_db();
    let hid = finish_hid(&db, "exp");
    run_cli(&["update", &hid, "state=false"], &db).unwrap();
    run_cli(&["moment", "exp", "-y", "-t", "11:00"], &db).unwrap();
}

#[test]
fn a_finished_field_is_hidden_as_an_ignored_one() {
    let db = finished_db();
    run_cli(&["moment", "kept", "-y", "-t", "10:00"], &db).unwrap();
    run_cli(&["ignore", "kept"], &db).unwrap();
    run_cli(&["moment", "on", "-y", "-t", "10:00"], &db).unwrap();
    // Hidden, both, and only the finished one finished (da-5n7zh), so
    // datum.day can tell it from an ignored one that `track` reopens.
    let flags = |f: &str| {
        let row = listing_of(&db, f);
        (row["ignored"].clone(), row["finished"].clone())
    };
    assert_eq!(
        (flags("exp"), flags("kept"), flags("on")),
        (
            (json!(true), json!(true)),
            (json!(true), json!(false)),
            (json!(false), json!(false))
        )
    );
    // `now` drops it and `now -A` lists it, by the same flag.
    let tracked = db.get_tracked_fields(Utc::now()).unwrap();
    assert!(tracked.iter().any(|f| f.field == "exp" && f.ignored));
}

#[test]
fn finished_is_a_status_not_a_state() {
    let db = finished_db();
    let err = run_cli(&["setup", "exp", "--add-state", "FINISHED"], &db).unwrap_err();
    assert!(format!("{err:#}").contains("datum finish exp"), "{err:#}");
    run_cli(&["finish", "nothing"], &db).unwrap_err();
    run_cli(&["finish", "timezone"], &db).unwrap_err();
}

/// Of two FINISHED rows the refusal names the first, the one to undo: a
/// bare column beside SQLite's `MIN()` is the minimum's row.
#[test]
fn the_refusal_names_the_first_finish() {
    let db = finished_db();
    let later = finish_hid(&db, "exp");
    run_cli(&["finish", "exp", "-yy", "-t", "11:00"], &db).unwrap();
    let first = db
        .entries_for_field("exp", None)
        .unwrap()
        .into_iter()
        .find(|e| e.state == Some(json!(FINISHED_STATE_VALUE)) && e.hid != later)
        .unwrap()
        .hid
        .to_string();
    let err = run_cli(&["moment", "exp", "-y", "-t", "11:00"], &db).unwrap_err();
    assert!(format!("{err:#}").contains(&first), "{err:#}");
}
