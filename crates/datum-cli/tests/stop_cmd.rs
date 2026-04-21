//! Parity tests for `datum stop` / `datum end` (P4 / da-ww1q).
//!
//! Ports the 8 cases from `endCmd.test.ts` in the original datum project,
//! adapted to daydumb's flag-based model:
//!   * daydumb lacks `--moment` on stop, `-T` (omit-timestamp), and positional
//!     duration — duration is supplied only via `--dur`. "Moment" semantics
//!     are the default for stop (no `--dur` ⇒ no `time_end`).
//!   * "Required keys" (`-k key`) is datum-specific; daydumb's `-k` always
//!     takes `KEY=VALUE`.
//!   * `end` is a CLI alias for `stop` — both produce `Verb::Stop`.
//!   * The "last state" recorded by datum's `endCmd` is mirrored by
//!     daydumb's `stop --last-state` (A6 feature — verified at the command
//!     layer here).
//!
//! Each Rust test maps 1-to-1 to a TS case; doc headers identify the source.

mod common;

use common::run_cli;
use datum_core::entry::Verb;
use datum_core::test_util::fresh_db;
use datum_core::{assert_count, assert_entry_matches};
use serde_json::json;

/// TS #1: "creates an occur document with state: false".
///
/// daydumb records a `Verb::Stop` entry; state compilation emits a `false`
/// event at `time_end` (or `time_start` when `time_end` is missing). A bare
/// `stop` has no `--dur`, so `time_end` is `None` and the `false` event
/// lands at `time_start`.
#[test]
fn stop_records_false_state_at_time_start() {
    let db = fresh_db();

    run_cli(&["stop", "dance"], &db).expect("stop dispatch");

    assert_count!(db, "dance", 1);
    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    assert_entry_matches!(entry, field: "dance", verb: "stop");
    let ts = entry.start_time.as_ref().expect("stop anchors time_start");
    assert!(
        entry.end_time.is_none(),
        "bare stop (no --dur) must not derive time_end",
    );

    // State compiles to `false` at time_start.
    let state = db.get_active_state("dance", ts.utc).unwrap();
    assert_eq!(state, json!(false));
}

/// TS #2: "can end from a different state".
///
/// Datum stores the prior `lastState` directly on the end doc; daydumb
/// records it via the `--last-state` flag, which closes the most recent
/// open block and captures its state under `extra_data.last_state`. Here
/// a prior `switch activity cello` is the block the stop closes, so
/// `last_state == "cello"`.
#[test]
fn stop_last_state_captures_prior_switch_state() {
    let db = fresh_db();

    // Switch 1 minute ago so it strictly precedes the stop.
    run_cli(&["switch", "activity", "cello", "-t=-1"], &db)
        .expect("switch dispatch");
    run_cli(&["stop", "activity", "--last-state"], &db)
        .expect("stop --last-state dispatch");

    let entries = db.entries_for_field("activity", None).unwrap();
    assert_eq!(entries.len(), 2, "switch + stop produce two entries");

    let stop = entries
        .iter()
        .find(|e| matches!(e.verb, Verb::Stop))
        .expect("stop entry present");
    let extra = stop.data.as_ref().expect("extra_data with last_state");
    assert_eq!(
        extra["last_state"],
        json!("cello"),
        "stop --last-state after switch must record the switch value",
    );

    // After the stop, the active state is false.
    let ts = stop.start_time.as_ref().unwrap().utc;
    let after = db
        .get_active_state("activity", ts + chrono::Duration::seconds(1))
        .unwrap();
    assert_eq!(after, json!(false));
}

/// TS #3: "can create a hole of time when duration is present".
///
/// Datum's `endCmd("dance 5")` took a positional duration and produced a
/// backward "hole" spanning `[occurTime-5, occurTime]` during which state
/// was false, with state returning to true afterwards. Daydumb's
/// `stop --dur 5m` also emits a closed block with `time_start=base-dur`
/// and `time_end=base`, but state compilation is simpler: `Verb::Stop`
/// emits a single `false` event at `time_end`, so daydumb does NOT restore
/// the prior state after the hole. The portable assertion is structural —
/// the stop entry carries both time bounds and the correct duration sign.
#[test]
fn stop_dur_produces_closed_backward_block() {
    let db = fresh_db();

    // Anchor a prior open block so the stop sits inside real state history.
    run_cli(&["start", "dance", "-t=-20"], &db).expect("anchor start");
    run_cli(&["stop", "dance", "--dur", "5m"], &db).expect("stop --dur 5m");

    let entries = db.entries_for_field("dance", None).unwrap();
    assert_eq!(entries.len(), 2);
    let stop = entries
        .iter()
        .find(|e| matches!(e.verb, Verb::Stop))
        .expect("stop entry present");

    let ts = stop.start_time.as_ref().expect("stop --dur anchors time_start").utc;
    let te = stop.end_time.as_ref().expect("stop --dur derives time_end").utc;
    assert_eq!(
        (te - ts).num_seconds(),
        5 * 60,
        "stop --dur 5m spans 5 minutes: time_start=base-dur, time_end=base",
    );
    assert_eq!(stop.duration_seconds(), Some(5 * 60));

    // State at time_end is false (the stop event fires here).
    let at_end = db.get_active_state("dance", te).unwrap();
    assert_eq!(at_end, json!(false));
    // Strictly before the stop block started, the anchoring `start` still
    // governs → state=true.
    let before_hole = db.get_active_state("dance", ts - chrono::Duration::seconds(1)).unwrap();
    assert_eq!(before_hole, json!(true));
}

/// TS #4: "when --moment/-m is specified, dur is null and there is no
/// duration positional argument".
///
/// Daydumb has no `--moment` on stop — but the default `stop` behavior is
/// already moment-like: without `--dur`, no `time_end` is derived. The
/// portable observation: `-k skillPoints=3` attaches extra data, and no
/// duration is inferred.
#[test]
fn stop_without_dur_is_moment_like() {
    let db = fresh_db();

    run_cli(&["stop", "dance", "-k", "skillPoints=3"], &db)
        .expect("stop with -k");

    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    assert_entry_matches!(entry, field: "dance", verb: "stop");
    assert!(entry.start_time.is_some());
    assert!(
        entry.end_time.is_none(),
        "no --dur ⇒ no time_end (moment semantics)",
    );
    let extra = entry.data.as_ref().expect("extra_data with skillPoints");
    assert_eq!(extra["skillPoints"], json!(3));
}

/// TS #5: "when --omit-timestamp/-T is specified, there is no positional
/// duration argument".
///
/// `-T` has no daydumb equivalent — daydumb entries are always
/// time-anchored. The salvageable behavior: multiple `-k` flags attach
/// cleanly, and no trailing positional gets misinterpreted as a duration.
#[test]
fn stop_extras_do_not_imply_duration() {
    let db = fresh_db();

    run_cli(
        &["stop", "dance", "-k", "skillPoints=3", "-k", "tempo=fast"],
        &db,
    )
    .expect("stop with multiple -k");

    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    assert_entry_matches!(entry, field: "dance", verb: "stop");
    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["skillPoints"], json!(3));
    assert_eq!(extra["tempo"], json!("fast"));
    assert!(
        entry.end_time.is_none(),
        "keys alone never derive a duration on stop",
    );
}

/// TS #6: "can skip the duration if the duration is given as . or ''".
///
/// Datum treated bare `.` / `''` as "skip duration"; daydumb has no
/// positional duration to skip. The equivalent guarantee: a stray
/// positional after the field becomes the primary value, never an
/// implicit duration.
#[test]
fn stop_positional_values_never_become_duration() {
    let db = fresh_db();

    run_cli(
        &["stop", "field", "50", "-k", "abc=123"],
        &db,
    )
    .expect("stop with positional value");

    let entry = &db.entries_for_field("field", None).unwrap()[0];
    assert_entry_matches!(entry, field: "field", verb: "stop", value: 50);
    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["abc"], json!(123));
    assert!(
        entry.end_time.is_none(),
        "positional values on stop never derive a time_end",
    );
}

/// TS #7: "throws an error if the duration supplied is invalid".
///
/// Datum surfaces `BadDurationError`; daydumb's `parse_duration` returns
/// a `DatumError::InvalidTime`, which bubbles through `cmd::dispatch` as
/// an `anyhow::Error`.
#[test]
fn stop_invalid_dur_is_rejected() {
    let db = fresh_db();

    let err = run_cli(&["stop", "field", "--dur", "30asd"], &db)
        .expect_err("30asd must fail duration parsing");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("Invalid time") || msg.contains("duration"),
        "error should describe duration parse failure, got: {msg}",
    );
    assert_count!(db, "field", 0);
}

/// TS #8: "still assigns a state of false even with required keys, and
/// duration comes before keys".
///
/// Daydumb has no "required keys" concept, but the test's essence — that
/// `--dur` + `-k` together still produce `verb=stop` with both the
/// derived time block and the extra key present — ports cleanly.
#[test]
fn stop_dur_plus_extra_still_marks_verb_stop() {
    let db = fresh_db();

    run_cli(
        &["stop", "field", "reqVal1", "--dur", "30m", "-k", "req1=extra"],
        &db,
    )
    .expect("stop --dur 30m -k ... dispatch");

    let entry = &db.entries_for_field("field", None).unwrap()[0];
    assert_entry_matches!(entry, field: "field", verb: "stop", value: "reqVal1");
    assert_eq!(entry.duration_seconds(), Some(30 * 60));
    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["req1"], json!("extra"));
}

// ─── Extra parity checks (from bead acceptance criteria) ─────────────────────

/// `stop` and its `end` alias must dispatch to the same command and
/// produce structurally identical entries (same verb, same field, same
/// shape). The CLI defines `end` as a clap alias of `stop`.
#[test]
fn stop_end_alias_equivalence() {
    let db_stop = fresh_db();
    let db_end = fresh_db();

    run_cli(&["stop", "dance"], &db_stop).expect("stop dispatch");
    run_cli(&["end", "dance"], &db_end).expect("end dispatch");

    let s = &db_stop.entries_for_field("dance", None).unwrap()[0];
    let e = &db_end.entries_for_field("dance", None).unwrap()[0];

    assert_eq!(s.field, e.field);
    assert_eq!(s.verb, e.verb, "end must dispatch to Verb::Stop");
    assert!(matches!(s.verb, Verb::Stop));
    assert!(matches!(e.verb, Verb::Stop));
    // Neither form should derive a time_end (no --dur given).
    assert!(s.end_time.is_none());
    assert!(e.end_time.is_none());
}

/// `stop --last-state` must close the most recent open block. If the
/// prior block was a `start` (state=true), `last_state=true` is captured;
/// if it was a `switch <value>`, the value is captured.
#[test]
fn stop_last_state_closes_open_start_block() {
    let db = fresh_db();

    run_cli(&["start", "working", "-t=-1"], &db).expect("start dispatch");
    run_cli(&["stop", "working", "--last-state"], &db)
        .expect("stop --last-state dispatch");

    let entries = db.entries_for_field("working", None).unwrap();
    let stop = entries
        .iter()
        .find(|e| matches!(e.verb, Verb::Stop))
        .expect("stop entry present");

    let extra = stop.data.as_ref().expect("extra_data present");
    assert_eq!(
        extra["last_state"],
        json!(true),
        "start opens state=true; stop --last-state must record that",
    );
}
