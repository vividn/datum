//! Parity tests for `datum start` (P3 / da-mreg).
//!
//! Ports the 10 cases from `startCmd.test.ts` in the original datum project,
//! adapted to daydumb's flag-based model:
//!   * daydumb lacks `--moment` / `-T` / positional duration — duration is
//!     supplied only via `--dur`. "Moment" semantics are the default (no
//!     `--dur` ⇒ no `time_end`), and `-T` (omit timestamp) has no equivalent
//!     because daydumb always anchors entries to `time_start`.
//!   * "Required keys" (`-k key`) is also datum-specific; daydumb's `-k`
//!     always takes `KEY=VALUE`.
//!
//! Each Rust test maps 1-to-1 to a TS case; headers identify the source it.

mod common;

use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::fresh_db;
use datum_core::{assert_count, assert_entry_matches};
use serde_json::json;

/// TS #1: "creates an occur document with state: true"
///
/// In daydumb, `start` records an open block: `time_start` set, `time_end`
/// unset. The state at time_start compiles to `true` (see
/// `state::compile_state` for the Start verb).
#[test]
fn start_creates_open_block_with_no_time_end() {
    let db = fresh_db();

    run_cli(&["start", "dance"], &db).expect("start dispatch");

    assert_count!(db, "dance", 1);
    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    assert_entry_matches!(entry, field: "dance", verb: "start");
    assert!(entry.start_time.is_some(), "start must set time_start");
    assert!(entry.end_time.is_none(), "open block must not set time_end");

    let state = db
        .get_active_state("dance", entry.start_time.as_ref().unwrap().utc)
        .unwrap();
    assert_eq!(state, json!(true), "start compiles to state=true at time_start");
}

/// TS #2: "can start from a different state"
///
/// Datum stores `lastState` on the start entry; daydumb does not. The
/// equivalent is verifiable via state compilation: the prior `switch` value
/// governs strictly before the new `start`, and `true` governs from `start`
/// onward.
#[test]
fn start_after_switch_preserves_prior_state() {
    let db = fresh_db();

    // Switch one minute ago so its time_start is strictly earlier.
    // `-t=-1` attaches the negative value to avoid `-1` being treated as a
    // standalone unknown-argument.
    run_cli(&["switch", "machine", "preparing", "-t=-1"], &db)
        .expect("switch dispatch");
    run_cli(&["start", "machine"], &db).expect("start dispatch");

    let entries = db.entries_for_field("machine", None).unwrap();
    assert_eq!(entries.len(), 2, "switch + start produce two entries");

    let start_entry = entries
        .iter()
        .find(|e| matches!(e.verb, Verb::Start))
        .expect("start entry present");
    let switch_entry = entries
        .iter()
        .find(|e| matches!(e.verb, Verb::Switch))
        .expect("switch entry present");

    let start_ts = start_entry.start_time.as_ref().unwrap().utc;
    let switch_ts = switch_entry.start_time.as_ref().unwrap().utc;

    // A hair before the start, the switch state still governs.
    let just_before_start = start_ts - chrono::Duration::seconds(1);
    assert!(just_before_start > switch_ts);
    assert_eq!(
        db.get_active_state("machine", just_before_start).unwrap(),
        json!("preparing"),
    );
    // At the start time, the Start verb takes over as `true`.
    assert_eq!(
        db.get_active_state("machine", start_ts).unwrap(),
        json!(true),
    );
}

/// TS #3: "has an optional duration arg that creates a block of time where
/// state was true".
///
/// Datum accepts a positional duration (`start dance 5`); daydumb uses the
/// `--dur` flag. The semantic is identical: `time_end = time_start + dur`.
#[test]
fn dur_flag_sets_time_end_for_closed_block() {
    let db = fresh_db();

    run_cli(&["start", "dance", "--dur", "5m"], &db).expect("start --dur 5m");

    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    // Start with a closed duration → shape=block, generated verb='block'.
    assert_entry_matches!(entry, field: "dance", verb: "block");
    assert_eq!(entry.duration_seconds(), Some(5 * 60));
    let ts = entry.start_time.as_ref().unwrap().utc;
    let te = entry.end_time.as_ref().unwrap().utc;
    assert_eq!((te - ts).num_seconds(), 5 * 60);
}

/// TS #4: "creates a hole of time where state was false when duration is
/// negative".
///
/// Datum's negative duration creates a backward block representing "state
/// was false for N min *before* now". Daydumb mirrors this: `--dur=-5m`
/// yields `time_end = time_start - 5min`, so `time_end < time_start`.
/// (This produces a `time_end_before_start` consistency flag — that check
/// is opt-in, not a hard constraint at insert time.)
#[test]
fn negative_dur_creates_backward_block() {
    let db = fresh_db();

    // An earlier `start` anchors the field so the hole sits inside real
    // state history (matches the datum fixture's prelude).
    run_cli(&["start", "dance", "-t=-20"], &db).expect("anchor start");
    run_cli(&["start", "dance", "--dur=-5m"], &db).expect("hole start");

    let entries = db.entries_for_field("dance", None).unwrap();
    assert_eq!(entries.len(), 2);

    let hole = entries
        .iter()
        .find(|e| e.duration_seconds().is_some())
        .expect("negative-dur entry has both time bounds");
    let dur = hole.duration_seconds().unwrap();
    assert_eq!(dur, -5 * 60, "negative --dur preserves its sign");
    // Start with a (negative) duration → shape=block, verb='block'.
    assert!(matches!(hole.verb, Verb::Block));
}

/// TS #5: "when --moment is specified, dur is null and there is no duration
/// positional argument".
///
/// Daydumb has no `--moment` flag — the default `start` behavior is already
/// "moment-like": without `--dur`, no `time_end` is derived. `-k` flags
/// accept `KEY=VALUE` (a datum divergence: datum's `-k` takes a key name
/// and pulls the value from the next positional).
#[test]
fn start_without_dur_is_moment_like() {
    let db = fresh_db();

    run_cli(&["start", "dance", "-k", "skillPoints=3"], &db)
        .expect("start with -k");

    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    assert_entry_matches!(entry, field: "dance", verb: "start");
    assert!(entry.start_time.is_some());
    assert!(
        entry.end_time.is_none(),
        "no --dur ⇒ no time_end (moment semantics)",
    );
    let extra = entry.data.as_ref().expect("extra_data with skillPoints");
    assert_eq!(extra["skillPoints"], json!(3));
}

/// TS #6: "when --omit-timestamp/-T is specified, there is no positional
/// duration argument".
///
/// `-T` has no daydumb equivalent (daydumb entries are always time-anchored).
/// The salvageable behavior being tested: extra keys attach correctly and
/// no duration is derived from extra positionals.
#[test]
fn start_extras_do_not_imply_duration() {
    let db = fresh_db();

    run_cli(
        &["start", "dance", "-k", "skillPoints=3", "-k", "tempo=fast"],
        &db,
    )
    .expect("start with multiple -k");

    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    assert_entry_matches!(entry, field: "dance", verb: "start");
    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["skillPoints"], json!(3));
    assert_eq!(extra["tempo"], json!("fast"));
    assert!(
        entry.end_time.is_none(),
        "keys alone never derive a duration",
    );
}

/// TS #7: "can skip the duration if the duration is given as . or ''".
///
/// Datum treated bare `.` / `''` as "skip duration"; daydumb has no
/// positional duration to skip. The equivalent: positionals after the field
/// become the primary value, never an implicit duration.
#[test]
fn positional_values_never_become_duration() {
    let db = fresh_db();

    run_cli(&["start", "field", "reqVal1"], &db).expect("start with value");

    let entry = &db.entries_for_field("field", None).unwrap()[0];
    assert_entry_matches!(entry, field: "field", verb: "start", value: "reqVal1");
    assert!(
        entry.end_time.is_none(),
        "positional values do not derive a time_end",
    );
}

/// TS #8: "throws an error if the duration supplied is invalid".
///
/// Datum surfaces `BadDurationError`; daydumb's `parse_duration` returns
/// `DatumError::InvalidTime`, which bubbles through `cmd::dispatch` as an
/// `anyhow::Error`.
#[test]
fn invalid_dur_is_rejected() {
    let db = fresh_db();

    let err = run_cli(&["start", "field", "--dur", "30asd"], &db)
        .expect_err("30asd must fail duration parsing");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("Invalid time") || msg.contains("duration"),
        "error should describe duration parse failure, got: {msg}",
    );
    assert_count!(db, "field", 0);
}

/// TS #9: "still assigns a state of true even with required keys, and
/// duration comes after required keys".
///
/// Daydumb has no "required keys" concept, but the test's essence — that
/// `--dur` + `-k` together still produce a closed block with both the
/// derived `time_end` and the key present in `data` — ports cleanly. Under
/// the data-model-v2 schema, `start --dur` produces `verb='block'` rather
/// than `start` (shape=block wins in the generated verb CASE).
#[test]
fn dur_plus_extra_still_marks_verb_start() {
    let db = fresh_db();

    run_cli(
        &["start", "field", "--dur", "30m", "-k", "req1=reqVal1"],
        &db,
    )
    .expect("start --dur 30m -k req1=reqVal1");

    let entry = &db.entries_for_field("field", None).unwrap()[0];
    assert_entry_matches!(entry, field: "field", verb: "block");
    assert_eq!(entry.duration_seconds(), Some(30 * 60));
    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["req1"], json!("reqVal1"));
}

/// TS #10: "fails when using a full day and a duration".
///
/// Datum's `-y` (yesterday) plus a trailing positional "315" triggered an
/// `ExtraDataError` because the positional couldn't be coerced into a
/// duration. Daydumb eliminates that ambiguity entirely: `-y` is a pure
/// counter flag, `--dur` is the sole duration source, and any trailing
/// positional becomes the primary value. This test locks in the
/// non-ambiguity: `start field -y 315` yields `value=315` anchored to
/// yesterday — no error, no duration inference.
#[test]
fn yesterday_flag_does_not_consume_positional_as_duration() {
    let db = fresh_db();

    let before = chrono::Utc::now();
    run_cli(&["start", "field", "-y", "315"], &db).expect("start -y 315");
    let after = chrono::Utc::now();

    let entry = &db.entries_for_field("field", None).unwrap()[0];
    assert_entry_matches!(entry, field: "field", verb: "start", value: 315);
    assert!(
        entry.end_time.is_none(),
        "`315` is a value, not a duration",
    );

    // Anchored roughly one day back (within the test's wall-clock window).
    let ts = entry.start_time.as_ref().unwrap().utc;
    let day = chrono::Duration::days(1);
    let lower = before - day - chrono::Duration::seconds(1);
    let upper = after - day + chrono::Duration::seconds(1);
    assert!(
        ts >= lower && ts <= upper,
        "time_start must be ~1 day ago (lo={lower} hi={upper} got={ts})",
    );
}
