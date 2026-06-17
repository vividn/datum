//! Parity tests for `datum start` (P3 / da-mreg).
//!
//! Ports the 10 cases from `startCmd.test.ts` in the original datum project,
//! adapted to daydumb's flag-based model:
//!   * daydumb supplies positional duration via `--dur`; `-m`/`--moment`
//!     forces a zero-duration point (`dur=MOMENT`). A bare `start` with
//!     neither opens an ongoing block (no `time_end`). `-T` (omit timestamp)
//!     IS supported — it yields a timeless entry (`occur_time` NULL, slug
//!     `field:unset`); see `no_timestamp.rs` for the cross-verb coverage.
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
/// In daydumb, `start` records an open block: `occur_time` set, `dur`
/// unset, state implicitly `true`. Under v3 (da-5ns), this shape derives
/// `Verb::Switch` (state=set, dur=null → switch).
#[test]
fn start_creates_open_block_with_no_time_end() {
    let db = fresh_db();

    run_cli(&["start", "dance"], &db).expect("start dispatch");

    assert_count!(db, "dance", 1);
    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    assert_entry_matches!(entry, field: "dance", verb: "switch");
    assert!(entry.occur_time.is_some(), "start must set occur_time");
    assert!(entry.dur.is_none(), "moment-shape must not set dur");

    let state = db
        .get_active_state("dance", entry.occur_time.as_ref().unwrap().utc)
        .unwrap();
    assert_eq!(
        state,
        json!(true),
        "start compiles to state=true at occur_time"
    );
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
    run_cli(&["switch", "machine", "preparing", "-t=-1"], &db).expect("switch dispatch");
    run_cli(&["start", "machine"], &db).expect("start dispatch");

    let entries = db.entries_for_field("machine", None).unwrap();
    assert_eq!(entries.len(), 2, "switch + start produce two entries");

    // Both entries are Verb::Switch under v3 (state=set, dur=null);
    // distinguish by stored state: the explicit `switch ... preparing`
    // carries state="preparing", `start machine` carries state=true.
    let start_entry = entries
        .iter()
        .find(|e| matches!(e.verb, Verb::Switch) && e.state.as_ref() == Some(&json!(true)))
        .expect("start entry present");
    let switch_entry = entries
        .iter()
        .find(|e| matches!(e.verb, Verb::Switch) && e.state.as_ref() == Some(&json!("preparing")))
        .expect("switch entry present");

    let start_ts = start_entry.occur_time.as_ref().unwrap().utc;
    let switch_ts = switch_entry.occur_time.as_ref().unwrap().utc;

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
    let ts = entry.occur_time.as_ref().unwrap().utc;
    let te = entry.computed_end_time().unwrap().utc;
    assert_eq!((te - ts).num_seconds(), 5 * 60);
}

/// TS #4: "creates a hole of time where state was false when duration is
/// negative".
///
/// Datum's negative duration created a backward block. Under v3
/// (da-5ns), the storage layer's `derive_verb` rejects negative
/// durations, but the CLI insert path clamps the time-end delta to zero
/// in `Entry::new_legacy` (entry.rs: `(e.utc - s.utc).num_seconds().max(0)`),
/// which collapses to `dur="PT0S"`. With state=true (Action::Start
/// canonicalization) and dur=Zero this derives `Verb::Point`.
#[test]
fn negative_dur_collapses_to_point() {
    let db = fresh_db();

    // An earlier `start` anchors the field so the collapsed entry sits
    // alongside real state history.
    run_cli(&["start", "dance", "-t=-20"], &db).expect("anchor start");
    run_cli(&["start", "dance", "--dur=-5m"], &db).expect("collapsed start");

    let entries = db.entries_for_field("dance", None).unwrap();
    assert_eq!(entries.len(), 2);

    let collapsed = entries
        .iter()
        .find(|e| e.duration_seconds() == Some(0))
        .expect("negative-dur entry collapses to zero-second dur");
    assert!(
        matches!(collapsed.verb, Verb::Point),
        "state=true + dur=Zero → Point",
    );
}

/// TS #5: "when --moment is specified, dur is null and there is no duration
/// positional argument".
///
/// This covers the bare `start` (no `-m`, no `--dur`): it opens an ongoing
/// block (verb=switch, no `time_end`). For the `-m` point-dot behavior see
/// `start_moment_records_point_dot` below. `-k` flags accept `KEY=VALUE` (a
/// datum divergence: datum's `-k` takes a key name and pulls the value from
/// the next positional).
#[test]
fn start_without_dur_is_moment_like() {
    let db = fresh_db();

    run_cli(&["start", "dance", "-k", "skillPoints=3"], &db).expect("start with -k");

    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    assert_entry_matches!(entry, field: "dance", verb: "switch");
    assert!(entry.occur_time.is_some());
    assert!(entry.dur.is_none(), "no --dur ⇒ no dur (moment semantics)",);
    let extra = entry.data.as_ref().expect("extra_data with skillPoints");
    assert_eq!(extra["skillPoints"], json!(3));
}

/// `start foo -m` records a momentary point (an occur-style dot), not an open
/// block: `-m` forces `dur=MOMENT`, which locks the entry to `Verb::Point`.
/// Regression for the bug where `-m` was ignored on `start` — it only fired
/// for `switch` (vividn, 2026-06-17).
#[test]
fn start_moment_records_point_dot() {
    let db = fresh_db();

    run_cli(&["start", "meeting", "-m"], &db).expect("start -m dispatch");

    let entry = &db.entries_for_field("meeting", None).unwrap()[0];
    assert_entry_matches!(entry, field: "meeting", verb: "point");
    assert!(
        matches!(entry.dur, Some(datum_core::dur::Dur::Moment)),
        "-m must set dur to MOMENT, got {:?}",
        entry.dur
    );
    let ts = entry
        .occur_time
        .as_ref()
        .expect("a moment still anchors a time");
    // A MOMENT has no span: no derived end time...
    assert!(
        entry.computed_end_time().is_none(),
        "MOMENT carries no end time",
    );
    // ...and it must NOT register as an open block (verb=point, not switch).
    assert!(
        db.find_open_block("meeting", ts.utc).unwrap().is_none(),
        "a -m start is a dot, not an open block",
    );
}

/// An explicit duration overrides `-m`: `start foo -m dur=30m` keeps the
/// duration, not MOMENT — "set dur to MOMENT unless explicitly set otherwise
/// (dur=3)" (vividn, 2026-06-17).
#[test]
fn start_moment_yields_to_explicit_dur() {
    let db = fresh_db();

    run_cli(&["start", "meeting", "-m", "dur=30m"], &db).expect("start -m dur=30m");

    let entry = &db.entries_for_field("meeting", None).unwrap()[0];
    assert!(entry.dur.is_some(), "explicit dur must be kept");
    assert!(
        !matches!(entry.dur, Some(datum_core::dur::Dur::Moment)),
        "explicit dur must win over -m, got {:?}",
        entry.dur
    );
}

/// TS #6: "when --omit-timestamp/-T is specified, there is no positional
/// duration argument".
///
/// daydumb does support `-T` (see `no_timestamp.rs`); the behavior ported
/// here is the orthogonal one: extra keys attach correctly and no duration
/// is derived from extra positionals.
#[test]
fn start_extras_do_not_imply_duration() {
    let db = fresh_db();

    run_cli(
        &["start", "dance", "-k", "skillPoints=3", "-k", "tempo=fast"],
        &db,
    )
    .expect("start with multiple -k");

    let entry = &db.entries_for_field("dance", None).unwrap()[0];
    assert_entry_matches!(entry, field: "dance", verb: "switch");
    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["skillPoints"], json!(3));
    assert_eq!(extra["tempo"], json!("fast"));
    assert!(entry.dur.is_none(), "keys alone never derive a duration",);
}

/// TS #7: "can skip the duration if the duration is given as . or ''".
///
/// da-ooe2: `start` never coerces a positional into state — state is always
/// `true`. With no `-k` slot to land on, a bare positional falls into the
/// injected `dur=` slot; a non-duration token like `reqVal1` is then
/// rejected at the dur-parse boundary so a typo can't silently land a bogus
/// state. Use `state=<v>` to set an explicit state.
#[test]
fn non_duration_positional_is_rejected() {
    let db = fresh_db();

    let err = run_cli(&["start", "field", "reqVal1"], &db)
        .expect_err("non-duration positional must be rejected on start");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("invalid dur value") || msg.contains("dur"),
        "error should flag the bad duration, got: {msg}",
    );
    assert_count!(db, "field", 0);
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
/// Per da-l3f3, daydumb restored original-datum positional duration on
/// `start`/`stop`: a trailing bare positional that parses as a duration
/// is consumed as the duration, not as state. So `start field -y 315`
/// anchors to yesterday with `dur = 315 minutes` and the default
/// `state = true` (Start canonicalization).
///
/// Per da-aaa1, `-y` alone is a date-only event: `time_start` lands at
/// 00:00:00 of yesterday in the entry's timezone (not "wall-clock minus
/// one day"). The duration assertion is what this test actually exists
/// for; the time-anchor assertion exists to catch regressions in the day
/// shift, which is now expressed as "yesterday's midnight."
#[test]
fn yesterday_flag_consumes_trailing_numeric_as_duration() {
    let db = fresh_db();

    run_cli(&["start", "field", "-y", "315"], &db).expect("start -y 315");

    let entry = &db.entries_for_field("field", None).unwrap()[0];
    assert_entry_matches!(entry, field: "field", verb: "block");
    assert_eq!(
        entry.duration_seconds(),
        Some(315 * 60),
        "`315` parses as a 315-minute duration",
    );
    assert_eq!(entry.state.as_ref(), Some(&json!(true)));

    let occur = entry.occur_time.as_ref().unwrap();
    let tz: chrono_tz::Tz = occur
        .timezone
        .parse()
        .expect("entry must carry a valid IANA timezone");
    let local = occur.utc.with_timezone(&tz);
    assert_eq!(
        local.time(),
        chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        "yesterday-alone strips to midnight (got {local})",
    );
    let today_local = chrono::Utc::now().with_timezone(&tz).date_naive();
    assert_eq!(
        local.date_naive(),
        today_local - chrono::Duration::days(1),
        "occur_time must be yesterday's date in the entry's tz",
    );
}

// ─── da-l3f3: positional duration on start/stop ──────────────────────────

/// `dm start abc 3` → field=abc, dur=3 minutes, default state=true.
/// Bare numeric positional with no explicit state is the duration.
#[test]
fn start_bare_numeric_positional_is_duration() {
    let db = fresh_db();

    run_cli(&["start", "abc", "3"], &db).expect("start abc 3");

    let entry = &db.entries_for_field("abc", None).unwrap()[0];
    assert_eq!(entry.field, "abc");
    assert_eq!(entry.duration_seconds(), Some(3 * 60));
    assert_eq!(entry.state.as_ref(), Some(&json!(true)));
}

/// da-ooe2: `dm start abc sit` is rejected. State on start is always
/// `true`; positionals can never become state. The lone bare positional
/// falls into the injected `dur=` slot and fails the dur parse. Use
/// `state=sit` instead.
#[test]
fn start_non_duration_positional_errors() {
    let db = fresh_db();

    let err = run_cli(&["start", "abc", "sit"], &db)
        .expect_err("non-duration positional must be rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("invalid dur value") || msg.contains("dur"),
        "error should flag the bad duration: {msg}",
    );
    assert_count!(db, "abc", 0);
}

/// da-ooe2: two bare positionals on start is rejected. The first fills the
/// `dur=` slot; the second has no remaining slot and no remainder bucket
/// (start), so it errors as extra data. Explicit state must be set via
/// `state=<v>`.
#[test]
fn start_two_bare_positionals_errors() {
    let db = fresh_db();

    let err =
        run_cli(&["start", "abc", "3", "sit"], &db).expect_err("second bare positional is extra");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("Extra data") || msg.contains("no matching key"),
        "error should flag the extra positional: {msg}",
    );
    assert_count!(db, "abc", 0);
}

/// Per da-ixrk: `state=<v>` is the explicit channel for setting a
/// non-default state on start. Combined with a duration positional, it
/// should produce state="sit", dur=3min, no errors.
#[test]
fn start_explicit_state_keyword_plus_duration() {
    let db = fresh_db();

    run_cli(&["start", "abc", "state=sit", "3"], &db).expect("start abc state=sit 3");

    let entry = &db.entries_for_field("abc", None).unwrap()[0];
    assert_eq!(entry.state.as_ref(), Some(&json!("sit")));
    assert_eq!(entry.duration_seconds(), Some(3 * 60));
}

/// da-ixrk regression: `dm start work 10m30s` must parse `10m30s` as a
/// compound duration (10 min + 30 sec = 630 sec), not coerce it into
/// state. State stays at the default `true`.
#[test]
fn start_compound_duration_positional() {
    let db = fresh_db();

    run_cli(&["start", "work", "10m30s"], &db).expect("start work 10m30s");

    let entry = &db.entries_for_field("work", None).unwrap()[0];
    assert_eq!(entry.state.as_ref(), Some(&json!(true)));
    assert_eq!(entry.duration_seconds(), Some(10 * 60 + 30));
}

// ─── da-ooe2: flexible-positional duration restoration ───────────────────

/// da-ooe2 (original bug): declared `-k` slots consume their positionals
/// first, leaving the trailing positional for the injected `dur=` slot.
/// `start field -k key1 -k key2 -k key3 val1 val2 val3 30m` must fill all
/// three keys AND record the duration — the old peeler choked on this with
/// a "too many durations" error.
#[test]
fn start_multi_key_slots_plus_trailing_duration() {
    let db = fresh_db();

    run_cli(
        &[
            "start", "field", "-k", "key1", "-k", "key2", "-k", "key3", "val1", "val2", "val3",
            "30m",
        ],
        &db,
    )
    .expect("multi-key + trailing dur should not choke");

    let entry = &db.entries_for_field("field", None).unwrap()[0];
    assert_eq!(entry.duration_seconds(), Some(30 * 60));
    assert_eq!(entry.state.as_ref(), Some(&json!(true)));
    let extra = entry.data.as_ref().unwrap();
    assert_eq!(extra["key1"], json!("val1"));
    assert_eq!(extra["key2"], json!("val2"));
    assert_eq!(extra["key3"], json!("val3"));
    assert!(extra.get("dur").is_none(), "dur routes onto the column");
}

/// da-ooe2: `dur=<v>` may appear anywhere in argv — it fills the `dur=`
/// slot directly, while a later bare positional fills the remaining `-k`
/// slot. `start acb -k key1 dur=5 10` → key1=10, dur=5min.
#[test]
fn start_explicit_dur_anywhere_with_key_slot() {
    let db = fresh_db();

    run_cli(&["start", "acb", "-k", "key1", "dur=5", "10"], &db).expect("flexible dur position");

    let entry = &db.entries_for_field("acb", None).unwrap()[0];
    assert_eq!(entry.duration_seconds(), Some(5 * 60));
    assert_eq!(entry.data.as_ref().unwrap()["key1"], json!(10));
}

/// da-ooe2: an explicit `-k dur=` slot declaration coexists with the
/// auto-injected one (consolidated to a single slot), filled by the
/// trailing positional. `start work -k dur= 30m` → dur=30min.
#[test]
fn start_explicit_dur_slot_declaration() {
    let db = fresh_db();

    run_cli(&["start", "work", "-k", "dur=", "30m"], &db).expect("explicit dur slot");

    let entry = &db.entries_for_field("work", None).unwrap()[0];
    assert_eq!(entry.duration_seconds(), Some(30 * 60));
}

/// `dm stop abc 3` mirrors start: trailing numeric is duration. For stop,
/// `--dur` interprets the entry as `time_start = base - dur`, so the bare
/// `3` must take the same code path.
#[test]
fn stop_bare_numeric_positional_is_duration() {
    let db = fresh_db();

    run_cli(&["stop", "abc", "3"], &db).expect("stop abc 3");

    let entry = &db.entries_for_field("abc", None).unwrap()[0];
    assert_eq!(entry.duration_seconds(), Some(3 * 60));
}

/// Trailing duration positional + explicit `--dur` is a conflict — better
/// to fail loudly than to silently pick a winner. Mirrors switch's rule.
#[test]
fn start_trailing_duration_conflicts_with_dur_flag() {
    let db = fresh_db();

    let err = run_cli(&["start", "abc", "3", "--dur", "5m"], &db)
        .expect_err("trailing dur + --dur should conflict");
    let msg = err.to_string();
    assert!(
        msg.contains("--dur") || msg.contains("conflict"),
        "error should mention conflict: {msg}",
    );
    assert_count!(db, "abc", 0);
}
