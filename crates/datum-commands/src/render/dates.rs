//! A date in a table (da-9rda8, vividn: «the due date column loses a lot
//! of space because repeating 2026- every time»): `Nov 6` this year, the
//! month in the locale's language, and the ISO date in any other year.
//! `--iso-times` prints every time and date cell as ISO 8601 instead, for
//! a spreadsheet or another program. Machine output (`--json`, a view's
//! tsv/csv/json) never comes here.

use std::cell::Cell;

use chrono::{Datelike, NaiveDate, SecondsFormat};
use datum_core::time::DatumTime;
use datum_parse::time::month_abbr;

thread_local! {
    /// `--iso-times` for the command running on this thread.
    static ISO_TIMES: Cell<bool> = const { Cell::new(false) };
}

/// `--iso-times` in force until the guard drops, as `WidthOverride`.
#[must_use = "--iso-times lasts only as long as the guard"]
pub struct IsoTimes(bool);

impl IsoTimes {
    pub fn set() -> Self {
        Self(ISO_TIMES.with(|c| c.replace(true)))
    }
}

impl Drop for IsoTimes {
    fn drop(&mut self) {
        ISO_TIMES.with(|c| c.set(self.0));
    }
}

pub fn iso_times() -> bool {
    ISO_TIMES.with(Cell::get)
}

/// The locale a month is named in: `LC_ALL`, then `LC_TIME`, then `LANG`,
/// the first set and not empty, as POSIX reads them. datum.day passes the
/// browser's language as `LANG`.
fn locale() -> String {
    ["LC_ALL", "LC_TIME", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default()
}

/// `d` as a table prints it: `Nov 6` in `today`'s year, `2027-11-06` in
/// another, and ISO always under `--iso-times`.
pub fn table_date(d: NaiveDate, today: NaiveDate) -> String {
    table_date_in(d, today, &locale())
}

fn table_date_in(d: NaiveDate, today: NaiveDate, locale: &str) -> String {
    if iso_times() || d.year() != today.year() {
        return d.format("%Y-%m-%d").to_string();
    }
    format!("{} {}", month_abbr(locale, d.month()), d.day())
}

/// A cell whose text is a date and nothing else, `YYYY-MM-DD`, as
/// [`table_date`] prints it; any other cell as it is. A view's columns are
/// plain values, so its dates are known by their shape.
pub fn date_cell(cell: String, today: NaiveDate) -> String {
    match as_date(&cell) {
        Some(d) => table_date(d, today),
        None => cell,
    }
}

/// The date a cell is, when it is `YYYY-MM-DD` and nothing else. The shape
/// is tried first, so a cell that is no date reads no locale.
fn as_date(cell: &str) -> Option<NaiveDate> {
    let shaped = cell.len() == 10
        && cell.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    shaped.then(|| NaiveDate::parse_from_str(cell, "%Y-%m-%d").ok())?
}

/// An instant as `--iso-times` prints it: RFC 3339 in its own offset, to
/// the second (`2026-11-03T09:24:25+01:00`).
pub fn iso_instant(t: &DatumTime) -> String {
    t.local_time().to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// Today's date where the host is.
pub fn today() -> NaiveDate {
    DatumTime::now().local_time().date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn a_date_this_year_is_its_month_and_day() {
        let today = ymd(2026, 11, 3);
        let cell = |s: &str, locale| match as_date(s) {
            Some(d) => table_date_in(d, today, locale),
            None => s.to_string(),
        };
        assert_eq!(cell("2026-11-06", "C"), "Nov 6");
        assert_eq!(cell("2026-10-03", "de_DE.UTF-8"), "Okt 3");
        assert_eq!(cell("2027-11-06", "C"), "2027-11-06");
        assert_eq!(cell("2026-11-06T10:00", "C"), "2026-11-06T10:00");
        assert_eq!(cell("2026-13-06", "C"), "2026-13-06");
        assert_eq!(cell("vacuum", "C"), "vacuum");
        let _iso = IsoTimes::set();
        assert_eq!(cell("2026-11-06", "C"), "2026-11-06");
    }
}
