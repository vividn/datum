//! Parse `-d` / `--date` arguments.
//!
//! Supports relative day offsets (`+1`, `-1`, `0`), day words (`today`,
//! `yesterday`, `tomorrow`), weekday names (`sunday`, `sun`), absolute
//! ISO dates (`2024-03-16`), and compact `<mmm><day>` shorthand (`apr29`,
//! `Jan5`, optionally suffixed `-YYYY` / `@YYYY` / `,YYYY`), with an
//! optional `±N[unit]` uncertainty suffix.
//!
//! Forms that name a recurring point rather than a moment (weekdays,
//! year-less compact dates) resolve against the [`TimeCtx`] the caller
//! passes in — see that type for the reference/direction contract.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

use crate::error::{ParseError, Result};
use crate::time::context::{DefaultDirection, TimeCtx};
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
/// Supports: "+1", "-1", "0", the day words "today" / "yesterday" /
/// "tomorrow", weekday names ("sunday", "sun"), ISO date "2024-03-16",
/// or compact `<mmm><day>[(-|@|,)YYYY]` (e.g. "apr29", "Jan5",
/// "apr29-2025", "oct10,2022"), plus an optional `±N[unit]` uncertainty
/// suffix and optional trailing `+`/`-` direction marker.
///
/// The direction marker drives resolution of the year-less forms:
/// `apr29+` resolves to the next April 29 (this year if still upcoming,
/// otherwise next year); `sunday-` to the Sunday strictly before the
/// reference. Absent a marker, `ctx.default_direction` decides. Fully
/// pinned forms (ISO dates, compact dates with explicit year) ignore the
/// marker; day words and relative offsets do too.
pub fn parse_date(s: &str, ctx: TimeCtx) -> Result<DateArg> {
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

    // Day words. Unambiguous, so the direction marker is inert on them
    // exactly as it is on a `±N` offset.
    if let Some(n) = day_word_offset(primary) {
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

    // Weekday names — `sunday`, `sun`. No overlap with the month
    // abbreviations, and the compact form needs a digit, so this can be
    // tried before it without ambiguity.
    if let Some(wd) = weekday_from_name(primary) {
        let today = reference_date(&ctx, s)?;
        return Ok(DateArg {
            days_offset: 0,
            absolute: Some(resolve_weekday(wd, today, direction, ctx.default_direction)),
            uncertainty,
            direction,
        });
    }

    // Compact `<mmm><day>[(-|@)YYYY]`
    if let Some((month, day, year)) = parse_compact_month_day(primary) {
        let absolute = match year {
            Some(y) => NaiveDate::from_ymd_opt(y, month, day)
                .ok_or_else(|| ParseError::InvalidTime(format!("Invalid date: '{primary}'")))?,
            None => resolve_year(
                month,
                day,
                reference_date(&ctx, s)?,
                direction,
                ctx.default_direction,
            )
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

/// Day offset for the three day words. Case-insensitive.
fn day_word_offset(s: &str) -> Option<i64> {
    match s.to_ascii_lowercase().as_str() {
        "today" => Some(0),
        "yesterday" => Some(-1),
        "tomorrow" => Some(1),
        _ => None,
    }
}

/// Weekday from a full name or its common abbreviation. Case-insensitive.
fn weekday_from_name(s: &str) -> Option<Weekday> {
    match s.to_ascii_lowercase().as_str() {
        "sun" | "sunday" => Some(Weekday::Sun),
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "weds" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        _ => None,
    }
}

/// The calendar day the ambiguous forms measure from. Errors rather than
/// guessing when the caller supplied no anchor (wasm with no clock).
fn reference_date(ctx: &TimeCtx, s: &str) -> Result<NaiveDate> {
    ctx.reference.map(|r| r.date()).ok_or_else(|| {
        ParseError::InvalidTime(format!("Cannot resolve '{s}' without a reference date"))
    })
}

/// Pick whichever candidate sits closer to `today`; ties go to the past.
fn closer(past: NaiveDate, future: NaiveDate, today: NaiveDate) -> NaiveDate {
    if today - past <= future - today {
        past
    } else {
        future
    }
}

/// Resolve a weekday name against `today`. An explicit marker searches
/// strictly (`sunday-` on a Sunday is a week back); a default lets today
/// itself answer.
fn resolve_weekday(
    target: Weekday,
    today: NaiveDate,
    direction: Option<Direction>,
    default: DefaultDirection,
) -> NaiveDate {
    match (direction, default) {
        (Some(Direction::Past), _) => weekday_back(target, today, true),
        (Some(Direction::Future), _) => weekday_forward(target, today, true),
        (None, DefaultDirection::Past) => weekday_back(target, today, false),
        (None, DefaultDirection::Future) => weekday_forward(target, today, false),
        (None, DefaultDirection::Nearest) => closer(
            weekday_back(target, today, false),
            weekday_forward(target, today, false),
            today,
        ),
    }
}

fn weekday_back(target: Weekday, today: NaiveDate, strict: bool) -> NaiveDate {
    let back = (today.weekday().num_days_from_monday() + 7 - target.num_days_from_monday()) % 7;
    let back = if back == 0 && strict { 7 } else { back };
    today - Duration::days(back.into())
}

fn weekday_forward(target: Weekday, today: NaiveDate, strict: bool) -> NaiveDate {
    let ahead = (target.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
    let ahead = if ahead == 0 && strict { 7 } else { ahead };
    today + Duration::days(ahead.into())
}

/// Resolve a year-less compact date against `today`. See
/// [`resolve_weekday`] for the strict/non-strict split between an
/// explicit marker and a default direction.
fn resolve_year(
    month: u32,
    day: u32,
    today: NaiveDate,
    direction: Option<Direction>,
    default: DefaultDirection,
) -> Option<NaiveDate> {
    match (direction, default) {
        (Some(Direction::Past), _) => most_recent_past(month, day, today, true),
        (Some(Direction::Future), _) => next_future(month, day, today, true),
        (None, DefaultDirection::Past) => most_recent_past(month, day, today, false),
        (None, DefaultDirection::Future) => next_future(month, day, today, false),
        (None, DefaultDirection::Nearest) => {
            match (
                most_recent_past(month, day, today, false),
                next_future(month, day, today, false),
            ) {
                (Some(p), Some(f)) => Some(closer(p, f, today)),
                (p, f) => p.or(f),
            }
        }
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

/// Pick the next occurrence of (month, day) on or after today. When
/// `strict` is true the candidate must be *strictly* after today (the `+`
/// direction marker pushes a tie to the next year, so `apr29+` on
/// April 29 yields next year). Walks forward up to 8 years.
fn next_future(month: u32, day: u32, today: NaiveDate, strict: bool) -> Option<NaiveDate> {
    for y in today.year()..today.year() + 8 {
        if let Some(d) = NaiveDate::from_ymd_opt(y, month, day) {
            let ok = if strict { d > today } else { d >= today };
            if ok {
                return Some(d);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every pre-existing case here predates the ctx parameter, when the
    /// no-marker default was "most recent past, today counts".
    const PAST: DefaultDirection = DefaultDirection::Past;

    #[test]
    fn most_recent_past_uses_current_year_when_already_passed() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, None, PAST),
            Some(NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
    }

    #[test]
    fn most_recent_past_rolls_back_when_future_in_current_year() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        assert_eq!(
            resolve_year(6, 7, today, None, PAST),
            Some(NaiveDate::from_ymd_opt(2025, 6, 7).unwrap())
        );
    }

    #[test]
    fn most_recent_past_skips_non_leap_years_for_feb_29() {
        // Today 2026-03-01: Feb 29 doesn't exist in 2026; latest valid Feb 29 is 2024.
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(
            resolve_year(2, 29, today, None, PAST),
            Some(NaiveDate::from_ymd_opt(2024, 2, 29).unwrap())
        );
    }

    // ── direction marker drives year inference ────────────────────────

    #[test]
    fn future_direction_picks_this_year_when_upcoming() {
        // 2026-04-05: apr29 still ahead → this year.
        let today = NaiveDate::from_ymd_opt(2026, 4, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Future), PAST),
            Some(NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
    }

    #[test]
    fn future_direction_picks_next_year_when_passed() {
        // 2026-05-05: apr29 already past → next year.
        let today = NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Future), PAST),
            Some(NaiveDate::from_ymd_opt(2027, 4, 29).unwrap())
        );
    }

    #[test]
    fn future_direction_at_tie_picks_next_year() {
        // 2026-04-29: candidate equals today → push to next year.
        let today = NaiveDate::from_ymd_opt(2026, 4, 29).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Future), PAST),
            Some(NaiveDate::from_ymd_opt(2027, 4, 29).unwrap())
        );
    }

    #[test]
    fn past_direction_picks_this_year_when_already_passed() {
        // 2026-05-05: apr29 already past → this year.
        let today = NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Past), PAST),
            Some(NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())
        );
    }

    #[test]
    fn past_direction_picks_last_year_when_upcoming() {
        // 2026-04-05: apr29 not yet → last year.
        let today = NaiveDate::from_ymd_opt(2026, 4, 5).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Past), PAST),
            Some(NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
        );
    }

    #[test]
    fn past_direction_at_tie_picks_last_year() {
        // 2026-04-29: candidate equals today → push to last year.
        let today = NaiveDate::from_ymd_opt(2026, 4, 29).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, Some(Direction::Past), PAST),
            Some(NaiveDate::from_ymd_opt(2025, 4, 29).unwrap())
        );
    }

    #[test]
    fn future_direction_skips_non_leap_years_for_feb_29() {
        // 2026-03-01 + apr29+: next valid Feb 29 forward is 2028.
        let today = NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(
            resolve_year(2, 29, today, Some(Direction::Future), PAST),
            Some(NaiveDate::from_ymd_opt(2028, 2, 29).unwrap())
        );
    }

    #[test]
    fn no_direction_at_tie_picks_today() {
        // Default behaviour (no marker): on April 29, "apr29" resolves to today.
        let today = NaiveDate::from_ymd_opt(2026, 4, 29).unwrap();
        assert_eq!(
            resolve_year(4, 29, today, None, PAST),
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
    fn parse_date_compact_comma_year() {
        // Regression for da-3405: `-d oct10,2022` should resolve to 2022-10-10.
        let arg = parse_date("oct10,2022", TimeCtx::now()).expect("parse oct10,2022");
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
