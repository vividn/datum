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

use chrono::{Datelike, Duration, Months, NaiveDate, Weekday};

use crate::error::{ParseError, Result};
use crate::time::context::{DefaultDirection, TimeCtx};
use crate::time::months::month_from_word;
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
/// Supports: "+1", "-1", "0" (a day offset takes a sign), the day words
/// "today" / "yesterday" / "tomorrow", weekday names ("sunday", "sun"),
/// ISO date "2024-03-16",
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

    // Relative integer offset: the sign opts in, as it does for `-t`'s
    // minutes and did in v2 (da-6d09r). Unsigned, `2026` was 2026 days
    // ahead and `20261005` the year 57499.
    if let Some(hint) = unsigned_days_hint(primary) {
        return Err(ParseError::InvalidTime(format!(
            "cannot parse date '{s}' ({hint})"
        )));
    }
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

    Err(ParseError::InvalidTime(format!(
        "cannot parse date '{s}' — see `datum help time`"
    )))
}

/// How to write what an unsigned day count may have meant. Four digits or
/// more are likelier a year or a compact date, and a sign on `20261005`
/// is the year 57499 again, so those are offered the date alone. Zero is
/// today either way, so it needs no sign.
pub(crate) fn unsigned_days_hint(s: &str) -> Option<String> {
    let unsigned = s.bytes().all(|b| b.is_ascii_digit()) && s.bytes().any(|b| b != b'0');
    let signed = match s.len() {
        ..4 => format!("'+{s}' or '-{s}' for relative days, or "),
        _ => String::new(),
    };
    unsigned.then(|| format!("use {signed}a date like 2026-10-05"))
}

/// Match `<month><1-2 digit day>` with an optional `-YYYY`, `@YYYY` or
/// `,YYYY` tail. The month is any word [`month_from_word`] reads (`nov`,
/// `Okt`, `déc.`), case-insensitive, and one space may follow it (`nov 6`,
/// da-9rda8); an unknown word returns `None`.
fn parse_compact_month_day(s: &str) -> Option<(u32, u32, Option<i32>)> {
    let word_end = s.find(|c: char| !(c.is_alphabetic() || c == '.'))?;
    let month = month_from_word(&s[..word_end])?;

    // Day digits (1-2), then optional year tail.
    let rest = &s[word_end..];
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
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
        (None, DefaultDirection::Past | DefaultDirection::NearestDate) => {
            weekday_back(target, today, false)
        }
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
/// explicit marker and a default direction. `NearestDate`, a write's, and
/// `Nearest`, a data key's, take the nearest occurrence (da-9rda8).
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
        (None, DefaultDirection::NearestDate | DefaultDirection::Nearest) => {
            nearest_year(month, day, today)
        }
    }
}

/// The occurrence of (month, day) within six months of `today` either way:
/// the coming one while it is less than six months off, else the last one,
/// so exactly six months is the past's, as a tie is everywhere here.
fn nearest_year(month: u32, day: u32, today: NaiveDate) -> Option<NaiveDate> {
    let past = most_recent_past(month, day, today, false);
    let future = next_future(month, day, today, false);
    let six_months_on = today.checked_add_months(Months::new(6))?;
    match (past, future) {
        (Some(p), Some(f)) => Some(if f < six_months_on { f } else { p }),
        (p, f) => p.or(f),
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
    /// A write's default (da-9rda8): a year-less date the nearest.
    const NEAREST: DefaultDirection = DefaultDirection::NearestDate;

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

    /// da-9rda8: on a write, a date this year still to come within six
    /// months is that one, not last year's: `nov6` on Nov 3 is three days on.
    /// A weekday stays the past one.
    #[test]
    fn a_yearless_date_is_the_nearest_one() {
        let nov3 = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        let ymd = |y, m, d| NaiveDate::from_ymd_opt(y, m, d);
        assert_eq!(resolve_year(11, 6, nov3, None, NEAREST), ymd(2026, 11, 6));
        assert_eq!(resolve_year(10, 20, nov3, None, NEAREST), ymd(2026, 10, 20));
        assert_eq!(
            resolve_year(6, 7, ymd(2026, 5, 5).unwrap(), None, NEAREST),
            ymd(2026, 6, 7)
        );
        // A query's default keeps last year's (vividn, 2026-11-03).
        assert_eq!(resolve_year(11, 6, nov3, None, PAST), ymd(2025, 11, 6));
        // Tuesday Nov 3: Thursday is the last one, not the coming one.
        assert_eq!(
            resolve_weekday(Weekday::Thu, nov3, None, NEAREST),
            ymd(2026, 10, 29).unwrap()
        );
    }

    /// Six months from Nov 6 is May 6: on May 5 the coming Nov 6 is more
    /// than six months off and last year's is nearer; on May 7 it is less.
    /// Exactly six months either way, May 6, goes to the past.
    #[test]
    fn six_months_either_way_is_the_line() {
        let ymd = |y, m, d| NaiveDate::from_ymd_opt(y, m, d);
        let on = |d: Option<NaiveDate>| resolve_year(11, 6, d.unwrap(), None, NEAREST);
        assert_eq!(on(ymd(2026, 5, 5)), ymd(2025, 11, 6));
        assert_eq!(on(ymd(2026, 5, 6)), ymd(2025, 11, 6));
        assert_eq!(on(ymd(2026, 5, 7)), ymd(2026, 11, 6));
        let nov6 = |d| {
            parse_date(
                "nov6",
                TimeCtx::at(ymd(2026, 5, d).unwrap().and_hms_opt(12, 0, 0).unwrap())
                    .facing(NEAREST),
            )
        };
        assert_eq!(nov6(5).unwrap().absolute, ymd(2025, 11, 6));
        assert_eq!(nov6(7).unwrap().absolute, ymd(2026, 11, 6));
    }

    /// A day six months on that does not exist (Sep 31) is the month's
    /// last: chrono's `checked_add_months` clamps, so the 31st of a month
    /// before a short one still resolves, and on the right side.
    #[test]
    fn the_line_holds_on_a_months_last_days() {
        let ymd = |y, m, d| NaiveDate::from_ymd_opt(y, m, d);
        for (today, month, day, want) in [
            (ymd(2026, 3, 31), 9, 29, ymd(2026, 9, 29)),
            (ymd(2026, 3, 31), 9, 30, ymd(2025, 9, 30)),
            (ymd(2026, 8, 31), 2, 27, ymd(2027, 2, 27)),
            (ymd(2026, 8, 31), 2, 28, ymd(2026, 2, 28)),
            (ymd(2026, 12, 31), 1, 5, ymd(2027, 1, 5)),
            (ymd(2026, 10, 31), 4, 29, ymd(2027, 4, 29)),
        ] {
            assert_eq!(
                resolve_year(month, day, today.unwrap(), None, NEAREST),
                want,
                "{month}-{day} on {today:?}"
            );
        }
    }

    /// vividn, 2026-11-03: «input should definitely accept 'nov 6' with a
    /// space», and a table's localized month reads back (`Okt 3`).
    #[test]
    fn a_month_word_takes_a_space_and_any_language() {
        let ctx = TimeCtx::at(
            NaiveDate::from_ymd_opt(2026, 11, 3)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
        )
        .facing(NEAREST);
        let at = |s| parse_date(s, ctx).unwrap().absolute;
        assert_eq!(at("nov 6"), at("nov6"));
        assert_eq!(at("Nov 6"), NaiveDate::from_ymd_opt(2026, 11, 6));
        assert_eq!(at("okt 3"), NaiveDate::from_ymd_opt(2026, 10, 3));
        assert_eq!(at("Mär 6"), NaiveDate::from_ymd_opt(2027, 3, 6));
        assert_eq!(at("déc. 24"), NaiveDate::from_ymd_opt(2026, 12, 24));
        assert_eq!(at("nov 6,2027"), NaiveDate::from_ymd_opt(2027, 11, 6));
        assert!(parse_date("nov  6", ctx).is_err());
        assert!(parse_date("nov", ctx).is_err());
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
