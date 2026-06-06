//! Regression tests for `datum retime` quick-id resolution (da-obes).
//!
//! Before da-obes, `retime` resolved its positional through
//! `resolve_quickid_single`, which never split on commas. So vividn's
//! repro `retime '_,__'` errored with `Entry not found: _,__` even
//! though `get '_,__'` happily showed both rows. retime now routes the
//! default (non-bulk-filter) path through the same comma-aware
//! `resolve_batch` helper that get/del/update share, applying the shift
//! to every resolved entry.

mod common;

use chrono::{DateTime, Utc};
use common::run_cli;
use datum_core::test_util::{entry_builder, fresh_db};
use serde_json::json;

fn utc(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

/// vividn's repro: `retime '_,__'` must shift BOTH the most-recent and
/// second-most-recent entries, not error on the literal comma string.
#[test]
fn retime_comma_underscore_shifts_all() {
    let db = fresh_db();
    // `__` (older) and `_` (most recent) by occur_time.
    let older = entry_builder("coffee")
        .time_start_utc(utc("2024-01-01T08:00:00Z"))
        .state(json!(1))
        .insert(&db);
    let newer = entry_builder("mood")
        .time_start_utc(utc("2024-01-01T09:00:00Z"))
        .state(json!("ok"))
        .insert(&db);

    run_cli(&["retime", "_,__", "--offset", "1h"], &db).expect("retime '_,__' must resolve both");

    // hid is stable across a retime (slug is derived from occur_time and
    // changes), so read each row back by hid.
    let after_newer = db.get_entry(newer.hid.as_str()).unwrap();
    let after_older = db.get_entry(older.hid.as_str()).unwrap();
    assert_eq!(
        after_newer.occur_time.unwrap().utc,
        utc("2024-01-01T10:00:00Z")
    );
    assert_eq!(
        after_older.occur_time.unwrap().utc,
        utc("2024-01-01T09:00:00Z")
    );
}

/// The same unification reaches explicit hid-prefix tokens: a
/// comma-joined pair shifts both named entries by the same amount.
#[test]
fn retime_comma_hid_prefixes_shift_both() {
    let db = fresh_db();
    let a = entry_builder("coffee")
        .time_start_utc(utc("2024-01-01T08:00:00Z"))
        .state(json!(1))
        .insert(&db);
    let b = entry_builder("mood")
        .time_start_utc(utc("2024-01-01T09:00:00Z"))
        .state(json!("ok"))
        .insert(&db);

    let pattern = format!("{},{}", &a.hid.as_str()[..4], &b.hid.as_str()[..4]);
    run_cli(&["retime", &pattern, "--offset", "30m"], &db).expect("retime by hid-prefix pair");

    let after_a = db.get_entry(a.hid.as_str()).unwrap();
    let after_b = db.get_entry(b.hid.as_str()).unwrap();
    assert_eq!(after_a.occur_time.unwrap().utc, utc("2024-01-01T08:30:00Z"));
    assert_eq!(after_b.occur_time.unwrap().utc, utc("2024-01-01T09:30:00Z"));
}

/// A single, unambiguous quick-id still resolves and shifts exactly one
/// entry — the comma path must not regress the scalar case.
#[test]
fn retime_single_quickid_unchanged() {
    let db = fresh_db();
    let only = entry_builder("coffee")
        .time_start_utc(utc("2024-01-01T08:00:00Z"))
        .state(json!(1))
        .insert(&db);

    run_cli(&["retime", only.hid.as_str(), "--offset", "1h"], &db).expect("single retime");

    let after = db.get_entry(only.hid.as_str()).unwrap();
    assert_eq!(after.occur_time.unwrap().utc, utc("2024-01-01T09:00:00Z"));
}

/// A comma list naming an unresolvable token surfaces the resolution
/// error rather than silently shifting only the resolvable rows.
#[test]
fn retime_comma_with_missing_token_errors() {
    let db = fresh_db();
    let a = entry_builder("coffee")
        .time_start_utc(utc("2024-01-01T08:00:00Z"))
        .state(json!(1))
        .insert(&db);

    let pattern = format!("{},zzzzzzzz", &a.hid.as_str()[..4]);
    let err = run_cli(&["retime", &pattern, "--offset", "1h"], &db)
        .expect_err("a missing token must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("zzzzzzzz") || msg.to_lowercase().contains("not found"),
        "missing token should surface in the error: {msg}"
    );
}
