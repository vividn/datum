//! `datum track` writes TRACKED on a field of any verb (da-6fvys), never
//! `true` (da-5ifp).

use crate::common;

use chrono::Utc;
use common::run_cli;
use datum_commands::commands::fields::compute_fields;
use datum_core::action::Action;
use datum_core::db::IGNORED_STATE_VALUE;
use datum_core::db::{Db, NowFilter};
use datum_core::shape::TRACKED_STATE_VALUE;
use datum_core::test_util::{advance_time, fresh_db, set_time};
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

/// da-6fvys: tracking began, and that is all the row says. A switch or span
/// field used to get `false`, which `tail` drew as an `end` nobody logged.
#[test]
fn a_field_of_any_verb_is_tracked_not_ended() {
    let db = fresh_db();
    for verb in ["occur", "add", "switch", "start"] {
        let field = format!("f{verb}");
        run_cli(&["track", &field, "--verb", verb], &db).unwrap();
        assert_eq!(latest_state(&db, &field), json!(TRACKED_STATE_VALUE));
    }
    let now = db.current_now(&NowFilter::default()).unwrap();
    assert!(now.active.is_empty(), "TRACKED is not active: {now:?}");

    // And the first real state after it is that state, from TRACKED.
    run_cli(&["switch", "fswitch", "happy"], &db).unwrap();
    assert_eq!(latest_state(&db, "fswitch"), json!("happy"));
    let now = db.current_now(&NowFilter::default()).unwrap();
    assert_eq!(now.active.len(), 1, "{now:?}");
}

/// vividn, 2026-10-09: «track should only change the state of a field if it
/// is IGNORED or nothing (because no data exists) und zwar to TRACKED not
/// to false». Any other field gets its spec and no row, so a running field
/// keeps running and a second `track` is no repeat.
#[test]
fn track_writes_its_row_only_for_an_ignored_or_empty_field() {
    let db = fresh_db();
    let rows = |field: &str| db.entries_for_field(field, None).unwrap().len();

    // Running: still running, and the spec is written.
    run_cli(&["switch", "mood", "happy", "-t", "-5"], &db).unwrap();
    run_cli(&["track", "mood", "--color", "#123456"], &db).unwrap();
    assert_eq!(
        (rows("mood"), latest_state(&db, "mood")),
        (1, json!("happy"))
    );
    let spec = db.load_spec("mood").unwrap().unwrap();
    assert_eq!(spec.color.as_deref(), Some("#123456"));

    // Ended.
    run_cli(&["start", "work", "-t", "-5"], &db).unwrap();
    run_cli(&["end", "work", "-t", "-1"], &db).unwrap();
    run_cli(&["track", "work"], &db).unwrap();
    assert_eq!((rows("work"), latest_state(&db, "work")), (2, json!(false)));

    // Fresh: TRACKED, once. The second track of the idle field adds nothing.
    run_cli(&["track", "nap", "--verb", "start"], &db).unwrap();
    assert_eq!(latest_state(&db, "nap"), json!(TRACKED_STATE_VALUE));
    run_cli(&["track", "nap", "--color", "blue"], &db).unwrap();
    assert_eq!(rows("nap"), 1);

    // Ignored: TRACKED again.
    run_cli(&["ignore", "work"], &db).unwrap();
    run_cli(&["track", "work"], &db).unwrap();
    assert_eq!(
        (rows("work"), latest_state(&db, "work")),
        (4, json!(TRACKED_STATE_VALUE))
    );
}

/// The state `track` decides on is read at the instant its row is written,
/// on `DatumTime::now()`'s clock: under one set ahead of the wall clock the
/// ignore is still to come by the wall's reading, and the field stayed
/// ignored.
#[test]
fn track_reads_the_state_on_the_clock_it_writes_with() {
    use datum_core::time::clock::{install, uninstall, MockClock};
    use datum_core::time::DatumTime;

    let db = fresh_db();
    let clock = std::sync::Arc::new(MockClock::new(
        DatumTime::from_utc_and_tz(Utc::now() + chrono::Duration::days(1), "UTC").unwrap(),
    ));
    install(clock.clone());
    run_cli(&["track", "nap", "--verb", "occur"], &db).unwrap();
    clock.advance(chrono::Duration::minutes(1));
    run_cli(&["ignore", "nap"], &db).unwrap();
    clock.advance(chrono::Duration::minutes(1));
    let tracked = run_cli(&["track", "nap"], &db);
    uninstall();
    tracked.unwrap();
    assert_eq!(latest_state(&db, "nap"), json!(TRACKED_STATE_VALUE));
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
    let blue = datum_core::kit_color::resolve_color(Some("blue"), "sleep").unwrap();
    assert_eq!(
        spec.color, blue,
        "a hue name, as a kit's `color` (da-xw1rx)"
    );
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

/// `setup` on a new field is the same question, asked the same way.
#[test]
fn spec_set_on_a_new_field_needs_a_verb() {
    let db = fresh_db();
    let err = run_cli(&["setup", "y", "--color", "red"], &db)
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
        vec!["setup", "y", "--default-verb", "occur"],
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
    db.save_field_spec(&serde_json::from_str(r#"{"field":"sleep","verb":"occur"}"#).unwrap())
        .unwrap();
    run_cli(&["start", "sleep", "-t", "-60"], &db).unwrap();
    run_cli(&["end", "sleep", "-t", "-1"], &db).unwrap();

    let spec = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(spec.verb, Action::Occur);
    assert_eq!(db.field_verb("sleep").unwrap(), Action::Occur);
}

/// da-ijf3p: `track F` alone on a field with a spec writes the TRACK row
/// and leaves the spec as it was, and `ignore F` alone the IGNORED row.
/// Again, in the status it is in, it writes nothing.
#[test]
fn track_or_ignore_alone_on_a_spec_is_the_status_only() {
    let _clock = set_time("2026-01-15T12:00:00Z");
    let db = fresh_db();
    let spec_line = &["setup", "mood", "--verb", "switch", "--add-state", "calm"];
    run_cli(spec_line, &db).unwrap();
    let spec_row = || serde_json::to_value(db.spec_entry("mood").unwrap()).unwrap();
    let before = spec_row();
    let rows = |db: &Db| db.entries_for_field("mood", None).unwrap().len();
    for (line, state, n) in [
        ("ignore", json!(IGNORED_STATE_VALUE), 1),
        ("ignore", json!(IGNORED_STATE_VALUE), 1),
        ("track", json!(TRACKED_STATE_VALUE), 2),
        ("track", json!(TRACKED_STATE_VALUE), 2),
    ] {
        advance_time("1s");
        run_cli(&[line, "mood"], &db).unwrap();
        assert_eq!((latest_state(&db, "mood"), rows(&db)), (state, n), "{line}");
        assert_eq!(spec_row(), before, "{line} changed the spec");
    }
}

/// `track F <changes>` is `setup F <changes> --track`, and `ignore` the same
/// with `--ignore`: the same spec and the same row, and one `undo` takes
/// both back.
#[test]
fn track_or_ignore_with_changes_is_spec_with_the_status() {
    let changes = [
        "mood",
        "--verb",
        "switch",
        "--add-state",
        "good",
        "--state-color",
        "good=#00ff00",
        "-k",
        "why=",
    ];
    for (word, flag, state) in [
        ("track", "--track", TRACKED_STATE_VALUE),
        ("ignore", "--ignore", IGNORED_STATE_VALUE),
    ] {
        let sugar = fresh_db();
        let spec = fresh_db();
        let line = [&[word][..], &changes].concat();
        run_cli(&line, &sugar).unwrap();
        run_cli(&[&["setup"][..], &changes, &[flag]].concat(), &spec).unwrap();
        let shape = |db: &Db| {
            let mut spec = serde_json::to_value(db.load_spec("mood").unwrap()).unwrap();
            spec.as_object_mut().unwrap().remove("modify_time");
            (spec, latest_state(db, "mood"))
        };
        assert_eq!(shape(&sugar), shape(&spec), "{word}");
        assert_eq!(latest_state(&sugar, "mood"), json!(state), "{word}");

        run_cli(&["undo"], &sugar).unwrap();
        assert!(sugar.load_spec("mood").unwrap().is_none(), "{word}");
        assert!(sugar.entries_for_field("mood", None).unwrap().is_empty());
    }
}

/// `track F` on a field with no spec declares one, as `setup F --verb V
/// --track`: the log answers the verb, and a field with none is asked
/// (`track_without_a_verb_or_a_log_names_the_choices`). `ignore F` there
/// writes the row and declares nothing (vremya, 10-16, until vividn rules).
#[test]
fn alone_on_a_field_with_no_spec() {
    let _clock = set_time("2026-01-15T12:00:00Z");
    let db = fresh_db();
    run_cli(&["start", "nap", "-t", "-5"], &db).unwrap();
    run_cli(&["setup", "nap", "--delete"], &db).unwrap();
    advance_time("1s");
    run_cli(&["track", "nap"], &db).unwrap();
    assert_eq!(db.load_spec("nap").unwrap().unwrap().verb, Action::Start);
    assert_eq!(
        latest_state(&db, "nap"),
        json!(true),
        "a running field runs on"
    );

    run_cli(&["ignore", "typo"], &db).unwrap();
    assert_eq!(latest_state(&db, "typo"), json!(IGNORED_STATE_VALUE));
    assert!(db.load_spec("typo").unwrap().is_none());
}

/// `track` has no flags of its own: the ones it had are refused, `--state`
/// and `--tag` naming what replaced them, as on `setup`.
#[test]
fn track_has_no_flags_of_its_own() {
    let db = fresh_db();
    for (line, says) in [
        (
            &["track", "f", "--verb", "occur", "--key", "a"][..],
            "--key",
        ),
        (
            &["track", "f", "--verb", "occur", "--required-key", "a"],
            "--required-key",
        ),
        (&["track", "f", "--verb", "occur", "a=1"], "a=1"),
        (
            &["track", "f", "--verb", "switch", "--state", "a"],
            "--add-state",
        ),
        (
            &["track", "f", "--verb", "occur", "--tag", "a"],
            "--add-tag",
        ),
        (&["ignore", "f", "--tag", "a"], "--add-tag"),
        (&["track", "f", "--verb", "occur", "--ignore"], "give one"),
    ] {
        let err = run_cli(line, &db).unwrap_err().to_string();
        assert!(err.contains(says), "{line:?}: {err}");
    }
    assert!(db.load_spec("f").unwrap().is_none());
}

/// `-R KEY` says where a line's leftover words go in the command that
/// declares the field (da-fu29p): datum.day saves a new field with one
/// `track`, and a field whose entries are «whatever words follow» is that
/// key alone.
#[test]
fn the_remainder_rides_on_the_track() {
    let db = fresh_db();
    let remainder = |db: &Db| db.load_spec("emotion").unwrap().unwrap().remainder;
    run_cli(
        &["track", "emotion", "--verb", "occur", "-R", "emotions"],
        &db,
    )
    .unwrap();
    assert_eq!(remainder(&db).as_deref(), Some("emotions"));

    run_cli(&["emotion", "calm", "tired"], &db).unwrap();
    let logged = db.entries_for_field("emotion", None).unwrap();
    assert!(
        logged
            .iter()
            .any(|e| e.data == Some(json!({"emotions": ["calm", "tired"]}))),
        "{logged:?}"
    );

    // A re-track that does not name it keeps it; `''` clears it.
    run_cli(&["track", "emotion"], &db).unwrap();
    assert_eq!(remainder(&db).as_deref(), Some("emotions"));
    run_cli(&["track", "emotion", "--remainder", ""], &db).unwrap();
    assert_eq!(remainder(&db), None);
}

/// A bad name already in the database (logging is permissive, da-b049) is
/// tracked and ignored alone, which declares nothing; a change to it is a
/// declaration, and refused.
#[test]
fn a_bad_name_is_tracked_and_ignored_but_not_changed() {
    let _clock = set_time("2026-01-15T12:00:00Z");
    let db = fresh_db();
    run_cli(&["occur", " lead"], &db).unwrap();
    for line in [["ignore", " lead"], ["track", " lead"]] {
        advance_time("1s");
        run_cli(&line, &db).unwrap_or_else(|e| panic!("{line:?}: {e:#}"));
    }
    assert_eq!(latest_state(&db, " lead"), json!(TRACKED_STATE_VALUE));
    let err = run_cli(&["track", " lead", "--color", "red"], &db).unwrap_err();
    assert!(err.to_string().contains("cannot declare"), "{err:#}");
}
