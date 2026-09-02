//! End-to-end coverage for the date vocabulary added in da-0gs4:
//! day words (`today` / `yesterday` / `tomorrow`) and weekday names on
//! `-d`, plus `-t` falling back to date parsing so both flags accept the
//! same words.

mod common;

use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::fresh_db;

/// The local calendar date the single entry for `field` landed on.
fn only_entry_date(db: &Db, field: &str) -> NaiveDate {
    let entries = db.entries_for_field(field, None).unwrap();
    assert_eq!(entries.len(), 1, "expected exactly one '{field}' entry");
    entries[0]
        .occur_time
        .as_ref()
        .expect("entry has an occur_time")
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
