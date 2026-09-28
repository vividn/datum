//! da-1n61: `.` as a use-default-or-skip sentinel in optional positional
//! slots for state-bearing verbs (start/stop/switch).
//!
//! Ports datum's `dataArgs.ts` "skip slot" semantics to daydumb's verb-
//! level positional handling. The data-args layer already honoured `.`
//! for declared `-k` slots (see `dot_default_on_optional_key` in
//! `datum-parse`); these tests cover switch's state slot, the `dur=` slot
//! every state verb ends with, and (da-ryjnc) a `.` on start/stop skipping
//! the key slots before it.

use crate::common;

use common::run_cli;
use datum_core::test_util::fresh_db;
use serde_json::json;

// ─── start: `.` skips a key, or the dur slot ─────────────────────────────

/// da-ryjnc: start has no state slot (da-ixrk), so a `.` skips the next
/// open one — a `-k` key here — and the duration fills the `dur=` slot
/// after it. Until da-ryjnc, start/stop dropped every `.` first, and
/// `project` took `30m`.
#[test]
fn start_dot_skips_a_key_to_the_dur() {
    for (argv, data) in [
        (&["start", "w", "-k", "project=", ".", "30m"][..], None),
        (
            &["start", "w", "-k", "a=", "-k", "b=", ".", "x", "30m"][..],
            Some(json!({"b": "x"})),
        ),
    ] {
        let db = fresh_db();
        run_cli(argv, &db).expect("dot-skip dispatch");
        let e = &datum_core::test_util::logged_entries(&db, "w")[0];
        assert_eq!(e.data, data, "{argv:?}");
        assert_eq!(e.duration_seconds(), Some(30 * 60), "{argv:?}");
    }
}

/// With no key before it, the `.` skips the `dur=` slot itself, so
/// `start w . 30m` has nowhere to put `30m` — a refusal, not a guess.
#[test]
fn start_dot_skips_the_dur_slot_when_no_key_precedes_it() {
    let db = fresh_db();
    let err = run_cli(&["start", "w", ".", "30m"], &db).unwrap_err();
    assert!(format!("{err:#}").contains("Extra data"), "{err:#}");
}

/// An alias carrying the key binds the same way (`regions.rs` skips a `.`).
#[test]
fn alias_dot_skips_a_key_to_the_dur() {
    let db = fresh_db();
    run_cli(&["alias", "set", "wk", "start w -k project="], &db).unwrap();
    run_cli(&["wk", ".", "30m"], &db).expect("alias dot-skip");
    let e = &datum_core::test_util::logged_entries(&db, "w")[0];
    assert!(e.data.is_none(), "{:?}", e.data);
    assert_eq!(e.duration_seconds(), Some(30 * 60));
}

/// The alias layer passes its skip on for a switch's state and key slots
/// too, which the side's parser opens again: `sw . 30m` is state `true`
/// and `sk happy . 30m` leaves `a` absent, both with dur 30m.
#[test]
fn alias_dot_skips_switch_state_and_key() {
    let db = fresh_db();
    run_cli(&["alias", "set", "sw", "switch f"], &db).unwrap();
    run_cli(&["alias", "set", "sk", "switch g -k a="], &db).unwrap();
    run_cli(&["sw", ".", "30m"], &db).expect("state skip");
    run_cli(&["sk", "happy", ".", "30m"], &db).expect("key skip");
    for (field, state) in [("f", json!(true)), ("g", json!("happy"))] {
        let e = &datum_core::test_util::logged_entries(&db, field)[0];
        assert_eq!(e.state.as_ref(), Some(&state), "{field}");
        assert!(e.data.is_none(), "{field}: {:?}", e.data);
        assert_eq!(e.duration_seconds(), Some(30 * 60), "{field}");
    }
}

/// A spec key is a slot too (da-9s98k): `work 30m` fills `project`, as
/// the alias would, and `work . 30m` skips to the duration.
#[test]
fn spec_key_dot_skips_to_the_dur() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "work",
            "--verb",
            "start",
            "--data-key",
            "project=",
        ],
        &db,
    )
    .unwrap();
    run_cli(&["work", ".", "30m"], &db).expect("spec dot-skip");
    let e = &datum_core::test_util::logged_entries(&db, "work")[0];
    assert!(e.data.is_none(), "{:?}", e.data);
    assert_eq!(e.duration_seconds(), Some(30 * 60));
}

/// `start abc .` (lone `.`) is equivalent to `start abc` — the dot skips
/// the dur slot.
#[test]
fn start_lone_dot_skips_the_dur() {
    let db = fresh_db();

    run_cli(&["start", "project", "."], &db).expect("lone dot");

    let e = &datum_core::test_util::logged_entries(&db, "project")[0];
    assert_eq!(e.state.as_ref(), Some(&json!(true)));
    assert!(e.dur.is_none(), "no trailing positional → dur stays unset");
}

// ─── stop: `.` skips a key, or the dur slot ──────────────────────────────

/// `stop abc .` ends the block: the `.` skips the dur slot.
#[test]
fn stop_lone_dot_ends() {
    for verb in ["stop", "end"] {
        let db = fresh_db();
        run_cli(&[verb, "project", "."], &db).expect("stop dot");
        let e = &datum_core::test_util::logged_entries(&db, "project")[0];
        assert_eq!(e.state.as_ref(), Some(&json!(false)), "{verb}");
        assert!(e.dur.is_none(), "{verb}");
    }
}

/// `stop abc -k reason= . 30m` skips the key, as start does.
#[test]
fn stop_dot_skips_a_key_to_the_dur() {
    let db = fresh_db();

    run_cli(&["stop", "project", "-k", "reason=", ".", "30m"], &db).expect("stop dot-skip");

    let e = &datum_core::test_util::logged_entries(&db, "project")[0];
    assert_eq!(e.state.as_ref(), Some(&json!(false)));
    assert!(e.data.is_none(), "{:?}", e.data);
    assert_eq!(e.duration_seconds(), Some(30 * 60));
}

// ─── switch: `.` skips the state slot ────────────────────────────────────

/// `switch abc . 30m` → state defaults to `true`, dur=30m. The dot skips
/// the state slot just like for start/stop.
#[test]
fn switch_dot_skips_state_with_trailing_dur() {
    let db = fresh_db();

    run_cli(&["switch", "project", ".", "30m"], &db).expect("switch dot-skip");

    let e = &datum_core::test_util::logged_entries(&db, "project")[0];
    assert_eq!(e.state.as_ref(), Some(&json!(true)));
    assert_eq!(e.duration_seconds(), Some(30 * 60));
}

// ─── `.` in the trailing-duration slot is a no-op skip ───────────────────

/// `switch abc state .` — `.` in the duration slot is a skip (no
/// duration recorded). Without this, switch's trailing-reserved walker
/// would reject `.` as "not a valid duration".
#[test]
fn switch_dot_in_dur_slot_skips_duration() {
    let db = fresh_db();

    run_cli(&["switch", "project", "active", "."], &db).expect("switch dot in dur slot");

    let e = &datum_core::test_util::logged_entries(&db, "project")[0];
    assert_eq!(e.state.as_ref(), Some(&json!("active")));
    assert!(e.dur.is_none(), "`.` in dur slot leaves duration unset");
}

/// Per da-ixrk, `start` no longer accepts a non-duration positional like
/// `active` — state on start is always `true` (or `state=<v>`). The dot
/// in the duration slot stays a skip when paired with an explicit state
/// keyword.
#[test]
fn start_dot_in_dur_slot_skips_duration() {
    let db = fresh_db();

    run_cli(&["start", "project", "state=active", "."], &db).expect("dot in dur slot");

    let e = &datum_core::test_util::logged_entries(&db, "project")[0];
    assert_eq!(e.state.as_ref(), Some(&json!("active")));
    assert!(e.dur.is_none());
    // Make sure the trailing `.` didn't leak into the data payload.
    assert!(
        e.data.is_none(),
        "trailing `.` should not pollute data, got {:?}",
        e.data,
    );
}

// ─── escape: `\.` is a literal "." string ────────────────────────────────

/// On start the escaped dot is the literal `"."`, and the slot it lands in
/// is the dur's — refused, where before da-ryjnc it was dropped unseen.
#[test]
fn start_escaped_dot_is_an_invalid_dur() {
    let db = fresh_db();
    let err = run_cli(&["start", "w", r"\."], &db).unwrap_err();
    assert!(format!("{err:#}").contains("invalid dur value"), "{err:#}");
}

/// `switch abc \.` — the user wants a literal "." as the state value.
/// The `\.` escape unambiguates this from the skip sentinel.
#[test]
fn switch_escaped_dot_yields_literal_dot_state() {
    let db = fresh_db();

    run_cli(&["switch", "project", r"\."], &db).expect("escaped dot");

    let e = &datum_core::test_util::logged_entries(&db, "project")[0];
    assert_eq!(
        e.state.as_ref(),
        Some(&json!(".")),
        "`\\.` in the state slot resolves to the literal `.` string",
    );
}

// ─── the string-remainder peek keeps its own `.` rule ────────────────────

/// Under `-SR` the trailing `.` is the peek's skip-dur sentinel, so a final
/// number stays in the title (vividn, 09-28). Measured on main 459a6e81;
/// da-ryjnc leaves every row as it was.
#[test]
fn string_remainder_peek_keeps_its_dot() {
    for (tail, title, dur) in [
        (&["2", "."][..], "star wars episode 2", None),
        (&["2"][..], "star wars episode", Some(2 * 60)),
        (&["2", "90m"][..], "star wars episode 2", Some(90 * 60)),
        (&["2", r"\."][..], "star wars episode 2 .", None),
    ] {
        let db = fresh_db();
        let head = ["start", "movie", "-SR", "title", "star", "wars", "episode"];
        run_cli(&[&head[..], tail].concat(), &db).expect("-SR start");
        let e = &datum_core::test_util::logged_entries(&db, "movie")[0];
        assert_eq!(e.data, Some(json!({"title": title})), "{tail:?}");
        assert_eq!(e.duration_seconds(), dur, "{tail:?}");
    }
}

/// Through an alias the reserved dur rides the window's tail, so the
/// side's peek reads it as the direct command does: before da-ryjnc the
/// alias emitted it ahead of the window (`90m star wars episode`, dur 2m)
/// and dropped a `.`.
#[test]
fn string_remainder_peek_through_an_alias() {
    for (tail, title, dur) in [
        (&["2", "."][..], "star wars episode 2", None),
        (&["2"][..], "star wars episode", Some(2 * 60)),
        (&["2", "90m"][..], "star wars episode 2", Some(90 * 60)),
    ] {
        let db = fresh_db();
        run_cli(&["alias", "set", "mv", "start movie -SR title"], &db).unwrap();
        let head = ["mv", "star", "wars", "episode"];
        run_cli(&[&head[..], tail].concat(), &db).expect("alias -SR");
        let e = &datum_core::test_util::logged_entries(&db, "movie")[0];
        assert_eq!(e.data, Some(json!({"title": title})), "{tail:?}");
        assert_eq!(e.duration_seconds(), dur, "{tail:?}");
    }
}

/// An alias hands its window's `\.` on as typed, so each form stores what
/// the direct command does: the alias unescaped it first, and the side
/// then read a trailing `.` as its skip-dur sentinel and a lone one as no
/// value at all (da-gn23l).
#[test]
fn escaped_dot_through_an_alias_matches_the_direct_command() {
    for (alias, body, field) in [
        ("mv", "start movie -SR title", "movie"),
        ("sw", "switch f -I", "f"),
    ] {
        for tail in [&["a", "b", r"\."][..], &["a", r"\.", "b"], &[r"\."]] {
            let stored = |argv: Vec<&str>| {
                let db = fresh_db();
                run_cli(&["alias", "set", alias, body], &db).unwrap();
                run_cli(&argv, &db).expect("log");
                let e = &datum_core::test_util::logged_entries(&db, field)[0];
                (e.state.clone(), e.data.clone())
            };
            let direct = stored(body.split(' ').chain(tail.iter().copied()).collect());
            let aliased = stored([alias].into_iter().chain(tail.iter().copied()).collect());
            assert_eq!(aliased, direct, "{alias} {tail:?}");
        }
    }
}
