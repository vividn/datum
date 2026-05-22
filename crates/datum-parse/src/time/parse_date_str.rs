//! Parse `-d` / `--date` arguments.
//!
//! Supports relative day offsets (`+1`, `-1`, `0`), absolute ISO dates
//! (`2024-03-16`), and compact `<mmm><day>` shorthand (`apr29`, `Jan5`,
//! optionally suffixed `-YYYY` / `@YYYY` / `,YYYY`), with an optional
//! `±N[unit]` uncertainty suffix.

use chrono::{Datelike, NaiveDate};

use crate::error::{ParseError, Result};
use crate::time::uncertainty::{
    parse_uncertainty, split_direction_suffix, split_uncertainty, Direction, Uncertainty,
};

/// Parsed result of a date argument (-d flag).
pub struct DateArg {
    pub days_offset: i64,
    pub absolute: Option<NaiveDate>,
    pub uncertainty: Option<Uncertainty>,
    /// Trailing `+` / `-` direction hint. Forces year resolution into the
    /// next (`Future`) or most-recent-prior (`Past`) occurrence for
    /// year-less inputs. Year-pinned ISO dates and relative `±N` offsets
    /// ignore the hint.
    pub direction: Option<Direction>,
}

/// Parse a -d / --date argument.
/// Supports: "+1", "-1", "0", ISO date "2024-03-16", or compact
/// `<mmm><day>[(-|@|,)YYYY]` (e.g. "apr29", "Jan5", "apr29-2025",
/// "oct10,2022"), plus an optional `±N[unit]` uncertainty suffix and
/// optional trailing `+`/`-` direction marker.
///
/// The direction marker drives year inference for year-less compact
/// dates: `apr29+` resolves to the next April 29 (this year if still
/// upcoming, otherwise next year); `apr29-` resolves to the most recent
/// April 29 in the past. Year-pinned forms (ISO dates, compact dates
/// with explicit year) ignore the marker; relative offsets do too.
pub fn parse_date_arg(s: &str) -> Result<DateArg> {
    let (s_no_dir, direction) = split_direction_suffix(s)?;
    let (primary, uncert_body) = split_uncertainty(s_no_dir);
    let uncertainty = uncert_body.map(parse_uncertainty).transpose()?;

    // Relative integer offset
    if let Ok(n) = primary.parse::<i64>() {
        return Ok(DateArg {
            days_offset: n,
            absolute: None,
            uncertainty,
            direction,
        });
    }

    // Absolute ISO date
    if let Ok(d) = NaiveDate::parse_from_str(primary, "%Y-%m-%d") {
        return Ok(DateArg {
            days_offset: 0,
            absolute: Some(d),
            uncertainty,
            direction,
        });
    }

    // Compact `<mmm><day>[(-|@)YYYY]`
    if let Some((month, day, year)) = parse_compact_month_day(primary) {
        let absolute = match year {
            Some(y) => NaiveDate::from_ymd_opt(y, month, day)
                .ok_or_else(|| ParseError::InvalidTime(format!("Invalid date: '{primary}'")))?,
            None => infer_year(month, day, direction)
                .ok_or_else(|| ParseError::InvalidTime(format!("Invalid date: '{primary}'")))?,
        };
        return Ok(DateArg {
            days_offset: 0,
            absolute: Some(absolute),
            uncertainty,
            direction,
        });
    }

    Err(ParseError::InvalidTime(format!("Cannot parse date: '{s}'")))
}

/// Match `<mmm><1-2 digit day>` with an optional `-YYYY` or `@YYYY` tail.
/// Month token is case-insensitive; unknown month names return `None`.
fn parse_compact_month_day(s: &str) -> Option<(u32, u32, Option<i32>)> {
    let bytes = s.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    if !bytes[..3].iter().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let month = month_from_abbr(&s[..3])?;

    // Day digits (1-2), then optional year tail.
    let rest = &s[3..];
    let day_end = rest
        .as_bytes()
        .iter()
        .take(2)
        .take_while(|b| b.is_ascii_digit())
        .count();
    if day_end == 0 {
        return None;
    }
    let day: u32 = rest[..day_end].parse().ok()?;

    let tail = &rest[day_end..];
    let year = if tail.is_empty() {
        None
    } else {
        let sep = tail.as_bytes()[0];
        if sep != b'-' && sep != b'@' && sep != b',' {
            return None;
        }
        let ystr = &tail[1..];
        if ystr.len() != 4 || !ystr.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(ystr.parse().ok()?)
    };

    Some((month, day, year))
}

fn month_from_abbr(s: &str) -> Option<u32> {
    match s.to_ascii_lowercase().as_str() {
        "jan" => Some(1),
        "feb" => Some(2),
        "mar" => Some(3),
        "apr" => Some(4),
        "may" => Some(5),
        "jun" => Some(6),
        "jul" => Some(7),
        "aug" => Some(8),
        "sep" => Some(9),
        "oct" => Some(10),
        "nov" => Some(11),
        "dec" => Some(12),
        _ => None,
    }
}

/// Resolve a year-less compact date against "today", honouring an
/// optional direction marker. With no marker, behaviour matches the
/// pre-existing default (most-recent-past). With `Future`, picks the
/// next strictly-future occurrence; with `Past`, picks the strictly-past
/// occurrence — at-tie cases push *with* the marker so `apr29+` on
/// April 29 yields next year and `apr29-` yields last year, mirroring
/// the time-side tie-break.
#[cfg(feature = "native")]
fn infer_year(month: u32, day: u32, direction: Option<Direction>) -> Option<NaiveDate> {
    let today = crate::time::clock::now().local_time().date_naive();
    resolve_year(month, day, today, direction)
}

#[cfg(not(feature = "native"))]
fn infer_year(_month: u32, _day: u32, _direction: Option<Direction>) -> Option<NaiveDate> {
    // Without a clock we can't decide a year. Callers on wasm must spell
    // out the year explicitly (e.g. `apr29-2025`).
    None
}

fn resolve_year(
    month: u32,
    day: u32,
    today: NaiveDate,
    direction: Option<Direction>,
) -> Option<NaiveDate> {
    match direction {
        None | Some(Direction::Past) => most_recent_past(month, day, today, direction.is_some()),
        Some(Direction::Future) => next_future(month, day, today),
    }
}

/// Pick the most-recent occurrence of (month, day) on or before today.
/// When `strict` is true, the candidate must be *strictly* before today
/// (the `-` direction marker pushes a tie to the prior year). Walks
/// back up to 8 years to skip past invalid Feb-29 in non-leap years.
fn most_recent_past(month: u32, day: u32, today: NaiveDate, strict: bool) -> Option<NaiveDate> {
    let mut y = today.year();
    for _ in 0..8 {
        if let Some(d) = NaiveDate::from_ymd_opt(y, month, day) {
            let ok = if strict { d < today } else { d <= today };
            if ok {
                return Some(d);
            }
        }
        y -= 1;
    }
    None
}

/// Pick the next strictly-future occurrence of (month, day). The `+`
/// direction marker treats today as already-past, so `apr29+` on
/// April 29 yields next year. Walks forward up to 8 years.
fn next_future(month: u32, day: u32, today: NaiveDate) -> Option<NaiveDate> {
    for y in today.year()..today.year() + 8 {
        if let Some(d) = NaiveDate::from_ymd_opt(y, month, day) {
            if d > today {
                return Some(d);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_recent_past_uses_current_year_when_already_passed() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, None),
            Some(NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
    }

    #[test]
    fn most_recent_past_rolls_back_when_future_in_current_year() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        assert_eq!(
            resolve_year(6, 7, today, None),
            Some(NaiveDate::from_ymd_opt(2025, 6, 7).unwrap())
        );
    }

    #[test]
    fn most_recent_past_skips_non_leap_years_for_feb_29() {
        // Today 2026-03-01: Feb 29 doesn't exist in 2026; latest valid Feb 29 is 2024.
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(
            resolve_year(2, 29, today, None),
            Some(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap())
        );
    }

    // ── direction marker drives year inference ────────────────────────

    #[test]
    fn future_direction_picks_this_year_when_upcoming() {
        // 2026-04-05: apr29 still ahead → this year.
        let today = NaiveDate::from_ymd_opt(2026, 4, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Future)),
            Some(NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
    }

    #[test]
    fn future_direction_picks_next_year_when_passed() {
        // 2026-05-05: apr29 already past → next year.
        let today = NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Future)),
            Some(NaiveDate::from_ymd_opt(2027, 4, 29).unwrap())
        );
    }

    #[test]
    fn future_direction_at_tie_picks_next_year() {
        // 2026-04-29: candidate equals today → push to next year.
        let today = NaiveDate::from_ymd_opt(2026, 4, 29).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Future)),
            Some(NaiveDate::from_ymd_opt(2027, 4, 29).unwrap())
        );
    }

    #[test]
    fn past_direction_picks_this_year_when_already_passed() {
        // 2026-05-05: apr29 already past → this year.
        let today = NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Past)),
            Some(NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
    }

    #[test]
    fn past_direction_picks_last_year_when_upcoming() {
        // 2026-04-05: apr29 not yet → last year.
        let today = NaiveDate::from_ymd_opt(2026, 4, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Past)),
            Some(NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
        );
    }

    #[test]
    fn past_direction_at_tie_picks_last_year() {
        // 2026-04-29: candidate equals today → push to last year.
        let today = NaiveDate::from_ymd_opt(2026, 4, 29).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Past)),
            Some(NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
        );
    }

    #[test]
    fn future_direction_skips_non_leap_years_for_feb_29() {
        // 2026-03-01 + apr29+: next valid Feb 29 forward is 2028.
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(
            resolve_year(2, 29, today, Some(Direction::Future)),
            Some(NaiveDate::from_ymd_opt(2028, 2, 29).unwrap())
        );
    }

    #[test]
    fn no_direction_at_tie_picks_today() {
        // Default behaviour (no marker): on April 29, "apr29" resolves to today.
        let today = NaiveDate::from_ymd_opt(2026, 4, 29).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, None),
            Some(NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
    }

    #[test]
    fn compact_month_day_basic() {
        assert_eq!(parse_compact_month_day("apr29"), Some((4, 29, None)));
        assert_eq!(parse_compact_month_day("Jan5"), Some((1, 5, None)));
        assert_eq!(parse_compact_month_day("APR29"), Some((4, 29, None)));
    }

    #[test]
    fn compact_month_day_with_explicit_year() {
        assert_eq!(
            parse_compact_month_day("apr29-2025"),
            Some((4, 29, Some(2025)))
        );
        assert_eq!(
            parse_compact_month_day("apr29@2025"),
            Some((4, 29, Some(2025)))
        );
        assert_eq!(
            parse_compact_month_day("oct10,2022"),
            Some((10, 10, Some(2022)))
        );
    }

    #[test]
    fn parse_date_arg_compact_comma_year() {
        // Regression for da-3405: `-d oct10,2022` should resolve to 2022-10-10.
        let arg = parse_date_arg("oct10,2022").expect("parse oct10,2022");
        assert_eq!(arg.days_offset, 0);
        assert_eq!(arg.absolute, NaiveDate::from_ymd_opt(2022, 10, 10));
        assert!(arg.uncertainty.is_none());
        assert!(arg.direction.is_none());
    }

    #[test]
    fn compact_month_day_rejects_unknown_month() {
        assert_eq!(parse_compact_month_day("nope29"), None);
    }

    #[test]
    fn compact_month_day_rejects_no_day() {
        assert_eq!(parse_compact_month_day("apr"), None);
    }

    #[test]
    fn compact_month_day_rejects_three_digit_day() {
        // The day field is at most 2 digits; a third digit slips into the
        // tail and falls through as an invalid year separator.
        assert_eq!(parse_compact_month_day("apr291"), None);
    }
}
