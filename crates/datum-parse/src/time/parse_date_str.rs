//! Parse `-d` / `--date` arguments.
//!
//! Supports relative day offsets (`+1`, `-1`, `0`), absolute ISO dates
//! (`2024-03-16`), and compact `<mmm><day>` shorthand (`apr29`, `Jan5`,
//! optionally suffixed `-YYYY` / `@YYYY`), with an optional `±N[unit]`
//! uncertainty suffix.

use chrono::{Datelike, NaiveDate};

use crate::error::{ParseError, Result};
use crate::time::uncertainty::{parse_uncertainty, split_uncertainty, Uncertainty};

/// Parsed result of a date argument (-d flag).
pub struct DateArg {
    pub days_offset: i64,
    pub absolute: Option<NaiveDate>,
    pub uncertainty: Option<Uncertainty>,
}

/// Parse a -d / --date argument.
/// Supports: "+1", "-1", "0", ISO date "2024-03-16", or compact
/// `<mmm><day>[(-|@)YYYY]` (e.g. "apr29", "Jan5", "apr29-2025"),
/// plus an optional `±N[unit]` uncertainty suffix.
pub fn parse_date_arg(s: &str) -> Result<DateArg> {
    let (primary, uncert_body) = split_uncertainty(s);
    let uncertainty = uncert_body.map(parse_uncertainty).transpose()?;

    // Relative integer offset
    if let Ok(n) = primary.parse::<i64>() {
        return Ok(DateArg {
            days_offset: n,
            absolute: None,
            uncertainty,
        });
    }

    // Absolute ISO date
    if let Ok(d) = NaiveDate::parse_from_str(primary, "%Y-%m-%d") {
        return Ok(DateArg {
            days_offset: 0,
            absolute: Some(d),
            uncertainty,
        });
    }

    // Compact `<mmm><day>[(-|@)YYYY]`
    if let Some((month, day, year)) = parse_compact_month_day(primary) {
        let absolute = match year {
            Some(y) => NaiveDate::from_ymd_opt(y, month, day)
                .ok_or_else(|| ParseError::InvalidTime(format!("Invalid date: '{primary}'")))?,
            None => infer_most_recent_past(month, day)
                .ok_or_else(|| ParseError::InvalidTime(format!("Invalid date: '{primary}'")))?,
        };
        return Ok(DateArg {
            days_offset: 0,
            absolute: Some(absolute),
            uncertainty,
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
        if sep != b'-' && sep != b'@' {
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

/// Pick the most-recent occurrence of (month, day) on or before today.
/// Walks back up to 8 years to skip past invalid Feb-29 in non-leap years.
#[cfg(feature = "native")]
fn infer_most_recent_past(month: u32, day: u32) -> Option<NaiveDate> {
    let today = crate::time::clock::now().local_time().date_naive();
    most_recent_past(month, day, today)
}

#[cfg(not(feature = "native"))]
fn infer_most_recent_past(_month: u32, _day: u32) -> Option<NaiveDate> {
    // Without a clock we can't decide a year. Callers on wasm must spell
    // out the year explicitly (e.g. `apr29-2025`).
    None
}

fn most_recent_past(month: u32, day: u32, today: NaiveDate) -> Option<NaiveDate> {
    let mut y = today.year();
    for _ in 0..8 {
        if let Some(d) = NaiveDate::from_ymd_opt(y, month, day) {
            if d <= today {
                return Some(d);
            }
        }
        y -= 1;
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
            most_recent_past(4, 29, today),
            Some(NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
    }

    #[test]
    fn most_recent_past_rolls_back_when_future_in_current_year() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        assert_eq!(
            most_recent_past(6, 7, today),
            Some(NaiveDate::from_ymd_opt(2025, 6, 7).unwrap())
        );
    }

    #[test]
    fn most_recent_past_skips_non_leap_years_for_feb_29() {
        // Today 2026-03-01: Feb 29 doesn't exist in 2026; latest valid Feb 29 is 2024.
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(
            most_recent_past(2, 29, today),
            Some(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap())
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
