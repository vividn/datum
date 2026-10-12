//! `<field>:<state>:_` — the latest entry of a field in one state (da-jloyq).
//!
//! vividn's case: a run opened at 9:30, a 3-minute lift at 9:50, and at the
//! end of the run `update body:_ distanceKm=10` lands on the lift. The
//! state-scoped form names the run.

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::ids::{resolve_quickid_single, Slug};
use datum_core::test_util::{entry_builder, fresh_db};
use serde_json::json;

fn get(db: &Db, id: &str) -> Entry {
    resolve_quickid_single(db, id).unwrap_or_else(|e| panic!("{id} must resolve: {e}"))
}

fn run_then_lift(db: &Db) -> (Entry, Entry) {
    run_cli(&["switch", "body", "run", "-t", "9:30"], db).unwrap();
    run_cli(&["switch", "body", "lift", "-t", "9:50", "3"], db).unwrap();
    (get(db, "body:run:_"), get(db, "body:_"))
}

#[test]
fn state_scoped_names_the_run_and_update_lands_on_it() {
    let db = fresh_db();
    let (run, lift) = run_then_lift(&db);
    assert_eq!(run.state, Some(json!("run")));
    assert_eq!(lift.state, Some(json!("lift")));
    run_cli(&["get", "body:run:_"], &db).unwrap();

    run_cli(&["update", "body:run:_", "distanceKm=10"], &db).unwrap();
    let run = db.get_entry(run.hid.as_str()).unwrap();
    assert_eq!(run.data.unwrap()["distanceKm"], json!(10));
    let lift = db.get_entry(lift.hid.as_str()).unwrap();
    assert!(lift.data.is_none_or(|d| d.get("distanceKm").is_none()));
}

#[test]
fn double_underscore_is_the_older_run() {
    let db = fresh_db();
    run_cli(&["switch", "body", "run", "-t", "8:00"], &db).unwrap();
    let older = get(&db, "body:_");
    run_cli(&["switch", "body", "rest", "-t", "8:40"], &db).unwrap();
    let (newer, _) = run_then_lift(&db);
    assert_ne!(older.hid, newer.hid);
    assert_eq!(get(&db, "body:run:__").hid, older.hid);
    assert_eq!(get(&db, "body:run:_2").hid, older.hid);
}

#[test]
fn undefined_is_a_state_name_not_null() {
    let db = fresh_db();
    let undef = entry_builder("body").state(json!("undefined")).insert(&db);
    assert_eq!(get(&db, "body:undefined:_").hid, undef.hid);
}

#[test]
fn a_parallel_state_holding_it_matches() {
    let db = fresh_db();
    run_then_lift(&db);
    let both = entry_builder("body")
        .state(json!(["run", "stretch"]))
        .time_start_utc(chrono::Utc::now() + chrono::Duration::days(1))
        .insert(&db);
    assert_eq!(get(&db, "body:run:_").hid, both.hid);
}

#[test]
fn no_entry_in_the_state_names_field_and_state() {
    let db = fresh_db();
    let err = resolve_quickid_single(&db, "body:run:_")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("live body entries with the state run: 0"),
        "{err}"
    );
    run_then_lift(&db);
    let err = run_cli(&["get", "body:swim:_"], &db)
        .unwrap_err()
        .to_string();
    assert!(err.contains("body") && err.contains("swim"), "{err}");
    let err = resolve_quickid_single(&db, "body:run:__")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("live body entries with the state run: 1"),
        "{err}"
    );
}

#[test]
fn a_slug_spelled_like_it_still_wins() {
    let db = fresh_db();
    run_then_lift(&db);
    // The state `x` is not run, so were the slug not matched exactly first,
    // `body:run:_` would resolve to the 9:30 run and the hids would differ.
    for slug in ["body:run:_", "body:run:long"] {
        let mut e = entry_builder("body").state(json!("x")).build();
        e.slug = Slug::new(slug);
        db.insert_entry(&e).unwrap();
        assert_eq!(get(&db, slug).hid, e.hid, "{slug}");
    }
}
