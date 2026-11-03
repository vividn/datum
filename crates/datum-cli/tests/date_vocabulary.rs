//! End-to-end coverage for the date vocabulary added in da-0gs4:
//! day words (`today` / `yesterday` / `tomorrow`) and weekday names on
//! `-d`, plus `-t` falling back to date parsing so both flags accept the
//! same words.
#![allow(clippy::disallowed_types)] // native tests: `chrono::Local` is the host zone the binary reads too

use crate::common;

use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::fresh_db;

/// The local calendar date the single entry for `field` landed on.
fn only_entry_date(db: &Db, field: &str) -> NaiveDate {
    let entries = datum_core::test_util::logged_entries(db, field);
    assert_eq!(entries.len(), 1, "expected exactly one '{field}' entry");
    entries[0]
        .at
        .as_ref()
        .expect("entry has an at")
        .local_time()
        .date_naive()
}

#[test]
fn date_flag_accepts_day_words() {
    let db = fresh_db();
    let today = Local::now().date_naive();

    run_cli(&["occur", "wordtoday", "-d", "today"], &db).expect("-d today");
    run_cli(&["occur", "wordyest", "-d", "yesterday"], &db).expect("-d yesterday");
    run_cli(&["occur", "wordtmrw", "-d", "tomorrow"], &db).expect("-d tomorrow");

    assert_eq!(only_entry_date(&db, "wordtoday"), today);
    assert_eq!(only_entry_date(&db, "wordyest"), today - Duration::days(1));
    assert_eq!(only_entry_date(&db, "wordtmrw"), today + Duration::days(1));
}

/// A bare weekday looks backward — vividn's decision for da-0gs4, matching
/// the intent of every backward-looking flag. The trailing `+` marker
/// (which the parser already understood for `apr29`) forces the coming
/// one instead.
#[test]
fn date_flag_accepts_weekday_names() {
    let db = fresh_db();
    let today = Local::now().date_naive();

    run_cli(&["occur", "wdpast", "-d", "sunday"], &db).expect("-d sunday");
    run_cli(&["occur", "wdnext", "-d", "sunday+"], &db).expect("-d sunday+");

    let past = only_entry_date(&db, "wdpast");
    assert_eq!(past.weekday(), Weekday::Sun);
    assert!(past <= today, "bare weekday looks back (got {past})");
    assert!(today - past < Duration::days(7));

    let next = only_entry_date(&db, "wdnext");
    assert_eq!(next.weekday(), Weekday::Sun);
    assert!(next > today, "`sunday+` looks forward (got {next})");
    assert!(next - today <= Duration::days(7));
}

/// Half one of da-0gs4: `-t` falls back to date parsing, so a caller with
/// "either a time or a date" in hand no longer has to try both flags.
#[test]
fn time_flag_falls_back_to_date_parsing() {
    let db = fresh_db();
    let today = Local::now().date_naive();

    run_cli(&["occur", "tword", "-t", "yesterday"], &db).expect("-t yesterday");
    run_cli(&["occur", "tweekday", "-t", "sunday"], &db).expect("-t sunday");
    run_cli(&["occur", "tdate", "-t", "2024-03-16"], &db).expect("-t 2024-03-16");

    assert_eq!(only_entry_date(&db, "tword"), today - Duration::days(1));
    assert_eq!(only_entry_date(&db, "tweekday").weekday(), Weekday::Sun);
    assert_eq!(
        only_entry_date(&db, "tdate"),
        NaiveDate::from_ymd_opt(2024, 3, 16).unwrap()
    );
}

/// The fallback must not swallow da-4h7k's guard: an unsigned integer
/// that isn't a clock time is still an error, not a day offset. If it
/// reached the date parser, `-t 2500` would land 2500 days out.
#[test]
fn time_flag_still_rejects_out_of_range_integers() {
    let db = fresh_db();
    assert!(
        run_cli(&["occur", "event", "-t", "2500"], &db).is_err(),
        "-t 2500 must stay an error, not become a 2500-day offset"
    );
}

/// da-9rda8: a yearless date is the nearest one, so on Nov 3 `nov6` is
/// three days on, not a year back; `nov 6` takes a space, and a table's
/// localized month (`Okt 3`, German) reads back.
#[test]
fn a_yearless_date_is_the_nearest_and_reads_a_tables_month() {
    let _g = datum_core::test_util::set_time("2026-11-03T12:00:00");
    let db = fresh_db();
    let ymd = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    for (field, date, want) in [
        ("nearnov", "nov6", ymd(2026, 11, 6)),
        ("nearspace", "nov 6", ymd(2026, 11, 6)),
        ("nearoct", "oct20", ymd(2026, 10, 20)),
        ("nearokt", "Okt 3", ymd(2026, 10, 3)),
        ("nearmay", "may 2", ymd(2027, 5, 2)),
        ("farmay", "may 4", ymd(2026, 5, 4)),
        ("neardec", "dec1", ymd(2026, 12, 1)),
    ] {
        run_cli(&["occur", field, "-d", date], &db).unwrap_or_else(|e| panic!("-d {date}: {e}"));
        assert_eq!(only_entry_date(&db, field), want, "-d {date:?}");
    }
    // `-t` too, a day and a clock: November 6 at 22:30, the coming one.
    run_cli(&["occur", "neartime", "-t", "nov 6 22:30"], &db).unwrap();
    let e = &datum_core::test_util::logged_entries(&db, "neartime")[0];
    let local = e.at.as_ref().unwrap().local_time();
    assert_eq!(local.date_naive(), ymd(2026, 11, 6));
    assert_eq!(local.format("%H:%M").to_string(), "22:30");
    // A weekday stays the last one: Tuesday Nov 3's Thursday is Oct 29.
    run_cli(&["occur", "pastthu", "-d", "thu"], &db).unwrap();
    assert_eq!(only_entry_date(&db, "pastthu"), ymd(2026, 10, 29));
}
