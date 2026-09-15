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
    db.save_field_spec(&FieldSpec::with_verb("work", Action::Start))
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

/// da-emze: a colour is not a verb. `track --color` has to mint a spec to
/// put the colour in, and minting one with `occur` as a placeholder turned
/// a start/stop field into a one-tap occur button the moment it was
/// coloured (da-u8n7 honours a spec's verb). The spec it mints now
/// declares no verb, so the widget still reads the field's shape off its
/// log — `fields --json`'s `states` — and still draws a block.
#[test]
fn a_colour_alone_mints_no_verb() {
    let db = fresh_db();
    run_cli(&["start", "sleep", "-t", "-60"], &db).unwrap();
    run_cli(&["end", "sleep", "-t", "-1"], &db).unwrap();
    run_cli(&["track", "sleep", "--color", "blue"], &db).unwrap();

    let spec = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(spec.color.as_deref(), Some("blue"));
    assert_eq!(spec.default_verb, None, "a colour minted a verb");
    // …and no bare-field alias either: nothing decided what `datum sleep`
    // would mean.
    assert!(
        spec.derived_aliases.is_empty(),
        "verbless spec minted aliases: {:?}",
        spec.derived_aliases
    );

    let listing = compute_fields(&db).unwrap();
    assert_eq!(listing[0]["field"], "sleep");
    assert_eq!(listing[0]["spec"]["default_verb"], JsonValue::Null);
    // The widget's evidence: booleans, which is a block button.
    assert_eq!(listing[0]["states"], json!([false, true]));
    // And the CLI reads the same shape off the same log.
    assert_eq!(db.field_verb("sleep").unwrap(), Action::Start);
}

/// The wire format is unchanged: a spec an older build wrote still carries
/// its verb, and the verb still wins over the log (da-u8n7).
#[test]
fn a_spec_written_with_a_verb_still_loads_with_it() {
    let db = fresh_db();
    db.conn()
        .execute(
            "INSERT INTO field_specs (field, spec_json) VALUES (?1, ?2)",
            rusqlite::params!["sleep", r#"{"field":"sleep","default_verb":"occur"}"#],
        )
        .unwrap();
    run_cli(&["start", "sleep", "-t", "-60"], &db).unwrap();
    run_cli(&["end", "sleep", "-t", "-1"], &db).unwrap();

    let spec = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(spec.default_verb, Some(Action::Occur));
    assert_eq!(db.field_verb("sleep").unwrap(), Action::Occur);
}
