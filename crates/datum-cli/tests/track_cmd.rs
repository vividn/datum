//! `datum track` writes TRACKED on an occur field and `false` on a field
//! that holds a state, never `true` (da-5ifp).

use crate::common;

use chrono::Utc;
use common::run_cli;
use datum_commands::commands::fields::compute_fields;
use datum_core::action::Action;
use datum_core::db::{Db, NowFilter};
use datum_core::field_spec::FieldSpec;
use datum_core::shape::TRACKED_STATE_VALUE;
use datum_core::test_util::fresh_db;
use serde_json::{json, Value as JsonValue};

/// The state of the newest entry on `field`, TRACK documents included —
/// this file is about the row `datum track` writes.
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
    run_cli(&["track", "coffee", "--verb", "occur"], &db).unwrap();
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

/// da-c8mh (reversing da-emze): a colour is not a verb, but a spec cannot
/// be born without one. `track --color` on a field that HAS a log takes
/// the verb the log implies — once — and WRITES it, so the field's shape
/// stops being a re-reading of its history.
#[test]
fn a_colour_writes_the_verb_the_log_implies() {
    let db = fresh_db();
    run_cli(&["start", "sleep", "-t", "-60"], &db).unwrap();
    run_cli(&["end", "sleep", "-t", "-1"], &db).unwrap();
    run_cli(&["track", "sleep", "--color", "blue"], &db).unwrap();

    let spec = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(spec.color.as_deref(), Some("blue"));
    assert_eq!(spec.verb, Action::Start, "the log said start");
    // …and the bare-field alias every spec projects.
    let bare = spec
        .derived_aliases
        .iter()
        .find(|a| a.name == "sleep")
        .expect("every spec projects its bare-field alias");
    assert_eq!(bare.expands_to, "start sleep");

    let listing = compute_fields(&db).unwrap();
    assert_eq!(listing[0]["field"], "sleep");
    assert_eq!(listing[0]["spec"]["verb"], json!("start"));
    assert_eq!(listing[0]["states"], json!([false, true]));
    assert_eq!(db.field_verb("sleep").unwrap(), Action::Start);
}

/// A brand-new field has no log to read, so `track` asks rather than
/// guesses, and the error names the four choices (da-c8mh).
#[test]
fn track_without_a_verb_or_a_log_names_the_choices() {
    let db = fresh_db();
    let err = run_cli(&["track", "x"], &db).unwrap_err().to_string();
    assert!(err.contains("--verb"), "err = {err}");
    for choice in ["occur", "start", "switch", "add"] {
        assert!(err.contains(choice), "err = {err}");
    }
    assert!(db.load_spec("x").unwrap().is_none(), "a spec was minted");
}

/// `spec set` on a new field is the same question, asked the same way.
#[test]
fn spec_set_on_a_new_field_needs_a_verb() {
    let db = fresh_db();
    let err = run_cli(&["spec", "set", "y", "--color", "red"], &db)
        .unwrap_err()
        .to_string();
    assert!(err.contains("--verb"), "err = {err}");
}

/// da-c8mh renamed the flag. The old spelling is an error that names the
/// new one. (The STORED key is another matter: a spec written before the
/// rename still reads — `stored_spec_compat.rs`.)
#[test]
fn default_verb_flag_is_an_error_naming_verb() {
    let db = fresh_db();
    for argv in [
        vec!["spec", "set", "y", "--default-verb", "occur"],
        vec!["track", "y", "--default-verb", "occur"],
    ] {
        let err = run_cli(&argv, &db).unwrap_err().to_string();
        assert!(
            err.contains("--default-verb is now --verb"),
            "{argv:?} → {err}"
        );
    }
}

/// The stored spec's verb wins over the log, and the wire key is `verb`.
#[test]
fn a_stored_verb_wins_over_the_log() {
    let db = fresh_db();
    db.conn()
        .execute(
            "INSERT INTO field_specs (field, spec_json) VALUES (?1, ?2)",
            rusqlite::params!["sleep", r#"{"field":"sleep","verb":"occur"}"#],
        )
        .unwrap();
    run_cli(&["start", "sleep", "-t", "-60"], &db).unwrap();
    run_cli(&["end", "sleep", "-t", "-1"], &db).unwrap();

    let spec = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(spec.verb, Action::Occur);
    assert_eq!(db.field_verb("sleep").unwrap(), Action::Occur);
}

/// One `track` declares a switch field whole: `--state NAME` names a state
/// it may be switched to, `--state NAME=COLOR` paints one, and the `=` is
/// what tells the two apart (da-94q1). The web's Track modal runs exactly
/// this argv, so the two forms have to coexist on the one flag.
#[test]
fn a_bare_state_is_an_allowed_state_and_a_keyed_one_is_a_colour() {
    let db = fresh_db();
    run_cli(
        &[
            "track",
            "mood",
            "--verb",
            "switch",
            "--state",
            "good",
            "--state",
            "bad",
            "--state",
            "good=#00ff00",
        ],
        &db,
    )
    .unwrap();

    let spec = db.load_spec("mood").unwrap().unwrap();
    assert_eq!(spec.verb, Action::Switch);
    assert_eq!(
        spec.allowed_states.as_deref(),
        Some(["good".to_string(), "bad".to_string()].as_slice())
    );
    assert_eq!(
        spec.state_colors.get("good").map(String::as_str),
        Some("#00ff00")
    );

    // Naming no state leaves the list alone, so a later `track` — the one
    // that un-ignores a field — does not empty it.
    run_cli(&["track", "mood", "--color", "#800080"], &db).unwrap();
    let again = db.load_spec("mood").unwrap().unwrap();
    assert_eq!(
        again.allowed_states.as_deref().map(<[String]>::len),
        Some(2)
    );
}
