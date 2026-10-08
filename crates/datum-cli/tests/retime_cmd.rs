//! Regression tests for `datum retime` quick-id resolution (da-obes).
//!
//! Before da-obes, `retime` resolved its positional through
//! `resolve_quickid_single`, which never split on commas. So vividn's
//! repro `retime '_,__'` errored with `Entry not found: _,__` even
//! though `get '_,__'` happily showed both rows. retime now routes the
//! default (non-bulk-filter) path through the same comma-aware
//! `resolve_batch` helper that get/del/update share, applying the shift
//! to every resolved entry.

use crate::common;

use chrono::{DateTime, Utc};
use common::run_cli;
use datum_core::test_util::{entry_builder, fresh_db};
use serde_json::json;

fn utc(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

/// A point at `at` (RFC 3339) carrying the zone `tz`, and its hid.
fn point_in(db: &datum_core::db::Db, field: &str, at: &str, tz: &str) -> String {
    let t = datum_core::time::DatumTime::from_utc_and_tz(utc(at), tz).unwrap();
    let e = entry_builder(field).time_start(t).state(json!(1));
    e.insert(db).hid.to_string()
}

fn time_of(db: &datum_core::db::Db, hid: &str) -> datum_core::time::DatumTime {
    db.get_entry(hid).unwrap().occur_time.unwrap()
}

/// da-jix52: on an entry at 2026-10-05 09:15 (Berlin), `retime <hid> 1040`
/// and `retime <hid> -t 1040` both give 10:40 on THAT day — not 1040
/// minutes later, and not 10:40 today — and `+5` gives 09:20. The entry's
/// own zone is what its day is read in, so the host's does not matter.
#[test]
fn retime_positional_and_t_are_read_from_the_entrys_own_time() {
    // 09:15 in Berlin (+02:00), retimed by `argv` in a database of its own.
    let retimed = |argv: &[&str]| {
        let db = fresh_db();
        let hid = point_in(&db, "coffee", "2026-10-05T07:15:00Z", "Europe/Berlin");
        run_cli(&[&["retime", &hid][..], argv].concat(), &db).unwrap();
        time_of(&db, &hid)
    };
    for (argv, want) in [
        (&["1040"][..], "2026-10-05T08:40:00Z"),
        (&["-t", "1040"], "2026-10-05T08:40:00Z"),
        (&["+5"], "2026-10-05T07:20:00Z"),
        (&["-t", "+5"], "2026-10-05T07:20:00Z"),
        (&["-1h"], "2026-10-05T06:15:00Z"),
        (&["yesterday 10am"], "2026-10-04T08:00:00Z"),
        (&["-y", "1000"], "2026-10-04T08:00:00Z"),
    ] {
        let after = retimed(argv);
        assert_eq!(after.utc, utc(want), "{argv:?}");
        assert_eq!(after.timezone.as_deref(), Some("Europe/Berlin"));
    }
    // A day word alone is that day, whole, as `occur -t yesterday` logs it.
    let after = retimed(&["yesterday"]);
    assert_eq!(after.utc, utc("2026-10-03T22:00:00Z"));
    assert_eq!(after.uncertainty.as_deref(), Some("P1D"));
}

/// da-jix52: a comma list resolves each entry against its own time — two
/// entries on two days, in two zones, each go to 10:40 on their own day.
#[test]
fn retime_batch_reads_each_entry_from_its_own_time() {
    let db = fresh_db();
    let berlin = point_in(&db, "coffee", "2026-10-05T07:15:00Z", "Europe/Berlin");
    // 23:30 on Oct 1 in Tokyo, which is still Oct 1 in UTC and in Berlin.
    let tokyo = point_in(&db, "tea", "2026-10-01T14:30:00Z", "Asia/Tokyo");

    run_cli(&["retime", &format!("{berlin},{tokyo}"), "1040"], &db).unwrap();

    assert_eq!(time_of(&db, &berlin).utc, utc("2026-10-05T08:40:00Z"));
    assert_eq!(time_of(&db, &tokyo).utc, utc("2026-10-01T01:40:00Z"));
    assert_eq!(time_of(&db, &tokyo).timezone.as_deref(), Some("Asia/Tokyo"));

    // The bulk filter is the same path: every row from its own time.
    run_cli(&["retime", "--field", "tea", "-t", "+5"], &db).unwrap();
    assert_eq!(time_of(&db, &tokyo).utc, utc("2026-10-01T01:45:00Z"));
    assert_eq!(time_of(&db, &berlin).utc, utc("2026-10-05T08:40:00Z"));
}

/// A relative time states no clock time, so the entry's uncertainty rides
/// along as it did under the old minutes shift; a clock time replaces it.
#[test]
fn retime_relative_time_keeps_the_uncertainty_a_clock_time_replaces() {
    let db = fresh_db();
    let mut t =
        datum_core::time::DatumTime::from_utc_and_tz(utc("2026-10-05T07:15:00Z"), "Europe/Berlin")
            .unwrap();
    t.uncertainty = Some("PT150S".into());
    let hid = entry_builder("coffee").time_start(t).insert(&db).hid;

    run_cli(&["retime", &hid, "+5"], &db).unwrap();
    let after = time_of(&db, &hid);
    assert_eq!(after.utc, utc("2026-10-05T07:20:00Z"));
    assert_eq!(after.uncertainty.as_deref(), Some("PT150S"));

    run_cli(&["retime", &hid, "1040"], &db).unwrap();
    assert_eq!(time_of(&db, &hid).uncertainty, None);
}

/// da-jix52: an unsigned number after the id is a clock time now, so the
/// old `retime <id> 30` (minutes) is refused with the way to write it,
/// rather than moving the entry somewhere unasked.
#[test]
fn retime_unsigned_number_is_a_clock_time_not_minutes() {
    let db = fresh_db();
    let at = "2026-10-05T07:15:00Z";
    let hid = point_in(&db, "coffee", at, "Europe/Berlin");

    let msg = run_cli(&["retime", &hid, "30"], &db)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("use '+30' or '-30'"), "{msg}");
    assert_eq!(time_of(&db, &hid).utc, utc(at), "nothing moved");
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

/// da-bgq2: a retime that moved nothing is an error, not a success with a
/// `Retimed 0` line. The bulk-filter path is the way to get here — a
/// field with no live timed rows selects nothing at all.
#[test]
fn retime_bulk_filter_matching_nothing_errors() {
    let db = fresh_db();
    entry_builder("coffee")
        .time_start_utc(utc("2024-01-01T08:00:00Z"))
        .state(json!(1))
        .insert(&db);

    let err = run_cli(&["retime", "--field", "tea", "--offset", "1h"], &db)
        .expect_err("a filter that matches no rows must not report success");
    assert!(
        format!("{err:?}").contains("no entries retimed"),
        "unexpected error: {err:?}"
    );
}

/// da-496ai: since da-q1k4p a lone unsigned number is an id, so `retime 30`
/// meant as the old shift of `_LAST` names no entry. The error says how to
/// write the shift; a non-numeric miss, or a number beside a time arg (the
/// shift it suggests would be refused there) or a shift of its own, gets
/// no such hint.
#[test]
fn retime_lone_unsigned_number_not_found_hints_signed_shift() {
    let db = fresh_db();
    entry_builder("coffee")
        .time_start_utc(utc("2024-01-01T08:00:00Z"))
        .state(json!(1))
        .insert(&db);

    let msg = run_cli(&["retime", "30"], &db).unwrap_err().to_string();
    assert!(msg.contains("Entry not found: 30"), "{msg}");
    assert!(
        msg.contains("to shift _LAST by 30 minutes, write `retime +30`"),
        "{msg}"
    );

    // The message alone: under RUST_BACKTRACE a `{:?}` carries a backtrace.
    let msg = run_cli(&["retime", "zzzzzzzz"], &db)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("Entry not found: zzzzzzzz"), "{msg}");
    assert!(!msg.contains("to shift _LAST"), "{msg}");

    // Past i64::MAX the suggested `retime +N` could not parse as an offset.
    let msg = run_cli(&["retime", "9999999999999999999"], &db)
        .unwrap_err()
        .to_string();
    assert!(!msg.contains("to shift _LAST"), "{msg}");

    for beside in [
        &["-t", "0930"][..],
        &["--offset", "1h"],
        &["-r", "-t", "+5"],
        &["5"],
    ] {
        let argv = [&["retime", "30"][..], beside].concat();
        let msg = run_cli(&argv, &db).unwrap_err().to_string();
        assert!(msg.contains("Entry not found: 30"), "{argv:?}: {msg}");
        assert!(!msg.contains("to shift _LAST"), "{argv:?}: {msg}");
    }
}

/// da-1d0v2: `retime -T` takes the time away, turning a start into a note
/// about its state rather than moving it to now.
#[test]
fn retime_no_timestamp_makes_a_start_a_note() {
    let db = fresh_db();
    let start = entry_builder("work")
        .time_start_utc(utc("2024-01-01T08:00:00Z"))
        .state(json!(true))
        .insert(&db);

    run_cli(&["retime", start.hid.as_str(), "-T"], &db).expect("retime -T");

    let after = db.get_entry(start.hid.as_str()).unwrap();
    assert!(after.occur_time.is_none(), "got {:?}", after.occur_time);
    assert_eq!(after.verb, datum_core::entry::Verb::Point);
    assert_eq!(after.state, Some(json!(true)));
}
