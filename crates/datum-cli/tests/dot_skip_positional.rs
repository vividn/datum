//! da-1n61: `.` as a use-default-or-skip sentinel in optional positional
//! slots for state-bearing verbs (start/stop/switch).
//!
//! Ports datum's `dataArgs.ts` "skip slot" semantics to daydumb's verb-
//! level positional handling. The data-args layer already honoured `.`
//! for declared `-k` slots (see `dot_default_on_optional_key` in
//! `datum-parse`); these tests cover the verb-level state slot and
//! switch's trailing-duration slot. (Start/stop trailing-duration is
//! covered by da-l3f3's right-walking peeler.)

mod common;

use common::run_cli;
use datum_core::test_util::fresh_db;
use serde_json::json;

// ─── start: `.` skips the state slot ─────────────────────────────────────

/// `start abc . 30m` → no state (default `true`), dur=30m. The `.` skips
/// past the state slot; da-l3f3's right-walk peels `30m` as the duration.
#[test]
fn start_dot_skips_state_with_trailing_dur() {
    let db = fresh_db();

    run_cli(&["start", "project", ".", "30m"], &db).expect("dot-skip dispatch");

    let entries = db.entries_for_field("project", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(
        e.state.as_ref(),
        Some(&json!(true)),
        "skipped state slot defaults to start's `true`",
    );
    assert_eq!(
        e.duration_seconds(),
        Some(30 * 60),
        "trailing positional fills the duration slot",
    );
}

/// `start abc .` (lone `.`) is equivalent to `start abc` — the dot is
/// the explicit form of "skip — use default state".
#[test]
fn start_lone_dot_skips_state() {
    let db = fresh_db();

    run_cli(&["start", "project", "."], &db).expect("lone dot");

    let e = &db.entries_for_field("project", None).unwrap()[0];
    assert_eq!(e.state.as_ref(), Some(&json!(true)));
    assert!(e.dur.is_none(), "no trailing positional → dur stays unset");
}

// ─── stop: `.` skips the state slot ──────────────────────────────────────

/// `stop abc . 30m` → state=false (stop's default), dur=30m. Mirrors the
/// start path.
#[test]
fn stop_dot_skips_state_with_trailing_dur() {
    let db = fresh_db();

    run_cli(&["stop", "project", ".", "30m"], &db).expect("stop dot-skip");

    let e = &db.entries_for_field("project", None).unwrap()[0];
    assert_eq!(
        e.state.as_ref(),
        Some(&json!(false)),
        "skipped state slot defaults to stop's `false`",
    );
    assert_eq!(e.duration_seconds(), Some(30 * 60));
}

// ─── switch: `.` skips the state slot ────────────────────────────────────

/// `switch abc . 30m` → state defaults to `true`, dur=30m. The dot skips
/// the state slot just like for start/stop.
#[test]
fn switch_dot_skips_state_with_trailing_dur() {
    let db = fresh_db();

    run_cli(&["switch", "project", ".", "30m"], &db).expect("switch dot-skip");

    let e = &db.entries_for_field("project", None).unwrap()[0];
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

    let e = &db.entries_for_field("project", None).unwrap()[0];
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

    let e = &db.entries_for_field("project", None).unwrap()[0];
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

/// `switch abc \.` — the user wants a literal "." as the state value.
/// The `\.` escape unambiguates this from the skip sentinel.
#[test]
fn switch_escaped_dot_yields_literal_dot_state() {
    let db = fresh_db();

    run_cli(&["switch", "project", r"\."], &db).expect("escaped dot");

    let e = &db.entries_for_field("project", None).unwrap()[0];
    assert_eq!(
        e.state.as_ref(),
        Some(&json!(".")),
        "`\\.` in the state slot resolves to the literal `.` string",
    );
}
