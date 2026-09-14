//! `datum track` writes TRACKED on an occur field and `false` on a field
//! that holds a state, never `true` (da-5ifp).

mod common;

use chrono::Utc;
use common::run_cli;
use datum_commands::commands::fields::compute_fields;
use datum_core::action::Action;
use datum_core::db::{Db, NowFilter};
use datum_core::field_spec::FieldSpec;
use datum_core::shape::TRACKED_STATE_VALUE;
use datum_core::test_util::fresh_db;
use serde_json::{json, Value as JsonValue};

/// The state of the newest entry on `field`.
fn latest_state(db: &Db, field: &str) -> JsonValue {
    let entries = db.entries_for_field(field, None).unwrap();
    let newest = entries
        .iter()
        .max_by_key(|e| e.occur_time.as_ref().map(|t| t.utc))
        .unwrap();
    newest.state.clone().unwrap_or(JsonValue::Null)
}

#[test]
fn occur_field_is_tracked_not_on() {
    let db = fresh_db();
    run_cli(&["track", "coffee"], &db).unwrap();
    assert_eq!(latest_state(&db, "coffee"), json!(TRACKED_STATE_VALUE));

    let now = db.current_now(&NowFilter::default()).unwrap();
    assert!(now.active.is_empty(), "TRACKED is not active: {now:?}");

    // The widget bar's inputs: no TRACKED state to offer, so the button
    // stays an `occur`.
    let listing = compute_fields(&db).unwrap();
    assert_eq!(listing[0]["field"], "coffee");
    assert_eq!(listing[0]["states"], json!([]));
}

#[test]
fn block_field_is_tracked_false() {
    let db = fresh_db();
    db.save_field_spec(&FieldSpec::new("work", Action::Start))
        .unwrap();
    run_cli(&["track", "work"], &db).unwrap();
    assert_eq!(latest_state(&db, "work"), json!(false));

    // Without a spec, the states it has been logged with decide.
    run_cli(&["start", "posture", "-t", "-5"], &db).unwrap();
    run_cli(&["track", "posture"], &db).unwrap();
    assert_eq!(latest_state(&db, "posture"), json!(false));
}

#[test]
fn track_unignores_an_ignored_field() {
    let db = fresh_db();
    // What `datum ignore` writes, a minute back so the slugs differ.
    run_cli(&["switch", "mood", "IGNORED", "-t", "-1"], &db).unwrap();
    let ignored = |db: &Db| {
        db.get_tracked_fields(Utc::now())
            .unwrap()
            .into_iter()
            .find(|t| t.field == "mood")
            .unwrap()
            .ignored
    };
    assert!(ignored(&db));
    run_cli(&["track", "mood"], &db).unwrap();
    assert!(!ignored(&db));
}
