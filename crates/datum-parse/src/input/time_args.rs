//! `timeArgs` — composition of `-d`/`-y*`/`-t`/`-q*`/`-z`/`-D`/`-T` into
//! a `DatumTime` + onlyDate flag + uncertainty. Port of datum's
//! `src/input/timeArgs.ts`.
//!
//! # Design
//!
//! The TS version reads the current time directly via luxon. Daydumb
//! must stay WASM-safe, so [`TimeArgs::build`] takes an explicit
//! reference time (a `DatumTime`) that the host supplies. On native
//! builds the `datum-commands` caller passes `DatumTime::now()`; on
//! web/mobile, the host passes in the equivalent.
//!
//! # Non-ports
//!
//! * Chrono-node natural-language parsing (`"yesterday at noon"`,
//!   `"june 18"`, `"2 days ago"`, …). The parity audit lists this as an
//!   explicit non-port — daydumb returns `InvalidTime` for those
//!   forms, and callers can pre-process NL inputs before handing off.
//!
//! See `docs/parity-audit/parsing.md` ("`timeArgs.ts`") for the spec.

use chrono::{Duration, Offset, TimeZone, Utc};
use chrono_tz::Tz;

use crate::error::{ParseError, Result};
use crate::time::{parse_date_arg, parse_time_arg, DatumTime, Uncertainty};

/// Parsed shape of the timing flag group.
#[derive(Debug, Clone, Default)]
pub struct TimeArgs {
    /// `-d` / `--date`: relative day offset (`+1`, `-1`) or ISO date.
    pub date: Option<String>,
    /// `-y` / `--yesterday`: days-back counter. Stackable (`-yy` → 2).
    pub yesterday: u32,
    /// `-t` / `--time`: HHMM / HH:MM / am-pm / `+N` / `-N`.
    pub time: Option<String>,
    /// `-q` / `--quick`: 5-min-back counter. Stackable (`-qqq` → 15 min).
    pub quick: u32,
    /// `-z` / `--timezone`: IANA name (`America/Chicago`) or numeric UTC
    /// offset (`"+4"`, `"-7"`, `"0"`). Empty → host locale.
    pub timezone: Option<String>,
    /// `-D` / `--full-day`: force a date-only entry.
    pub full_day: bool,
    /// `-T` / `--omit-timestamp`: no time recorded.
    pub omit_timestamp: bool,
}

/// Output of [`TimeArgs::build`] — what datum called `TimeFromArgs`.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeFromArgs {
    /// Resolved `DatumTime`. `None` when `omit_timestamp` was set.
    pub time: Option<DatumTime>,
    /// `true` when *no* timing-modifying flag was supplied (the caller
    /// can tell "no args" from "user explicitly asked for now").
    pub unmodified: bool,
    /// `true` when the resolved moment is date-only — `-D`, or a `-d` /
    /// `-y*` without any clock-level input.
    pub only_date: bool,
    /// Explicit `±N[unit]` uncertainty carried on the parsed time/date.
    /// `None` when no uncertainty suffix was supplied.
    pub uncertainty: Option<Uncertainty>,
}

impl TimeArgs {
    /// Build a `TimeFromArgs` relative to `reference` — the caller-
    /// supplied "now". The reference carries the effective timezone:
    /// if [`TimeArgs::timezone`] overrides it, we re-anchor to that
    /// zone first.
    ///
    /// Matches datum's `handleTimeArgs` for every branch except the
    /// NL-parsing paths (see crate docs).
    pub fn build(&self, reference: &DatumTime) -> Result<TimeFromArgs> {
        if self.omit_timestamp {
            return Ok(TimeFromArgs {
                time: None,
                unmodified: false,
                only_date: false,
                uncertainty: None,
            });
        }

        // Resolve the effective timezone.
        let tz_name = match self.timezone.as_deref() {
            Some(s) => resolve_timezone(s)?,
            None => reference.timezone.clone(),
        };

        // Shift the reference into the effective timezone without changing
        // the instant, so clock-level overrides (time / date) land in the
        // local-time frame the user expects.
        let reference = rebase_to_timezone(reference.clone(), &tz_name)?;

        let mut unmodified = true;
        let mut cur = reference.clone();
        let mut uncertainty: Option<Uncertainty> = None;

        if let Some(ref s) = self.time {
            cur = apply_time_arg(cur, s, &mut uncertainty)?;
            unmodified = false;
        }

        if self.quick > 0 {
            let minutes = i64::from(self.quick) * 5;
            cur = shift_minutes(cur, -minutes)?;
            unmodified = false;
        }

        if let Some(ref s) = self.date {
            cur = apply_date_arg(cur, s, &mut uncertainty, &tz_name)?;
            unmodified = false;
        }

        if self.yesterday > 0 {
            cur = shift_days(cur, -(i64::from(self.yesterday)), &tz_name)?;
            unmodified = false;
        }

        let only_date = self.full_day
            || ((self.date.is_some() || self.yesterday > 0)
                && self.time.is_none()
                && self.quick == 0);

        // Refresh `offset_minutes` in case a date shift crossed a DST
        // boundary — `cur.utc` is authoritative; the offset must re-derive
        // from the effective tz at that moment.
        cur = refresh_offset(cur)?;

        Ok(TimeFromArgs {
            time: Some(cur),
            unmodified,
            only_date,
            uncertainty,
        })
    }
}

// ─── timezone resolution ────────────────────────────────────────────────

/// Resolve the `-z` / `--timezone` input into an IANA name.
///
/// Accepted forms (matching `src/time/getTimezone.ts`):
///
/// * IANA name (`"America/Chicago"`, `"UTC"`, …).
/// * `"UTC±N"` / `"UTC+N"`.
/// * A bare numeric offset in hours: `"-7"`, `"+4"`, `"0"`, `"2"`. This
///   maps onto an `Etc/GMT∓N` name because IANA's `Etc/GMT*` zones use
///   the POSIX sign convention (UTC+4 → `Etc/GMT-4`).
pub fn resolve_timezone(spec: &str) -> Result<String> {
    if spec.is_empty() {
        return Err(ParseError::InvalidArg("Empty timezone".to_string()));
    }

    // Numeric offset (hours).
    if let Ok(hours) = spec.parse::<i32>() {
        if !(-24..=24).contains(&hours) {
            return Err(ParseError::InvalidArg(format!(
                "Timezone offset out of range: {spec}"
            )));
        }
        // POSIX-style naming: `Etc/GMT-N` is N hours ahead of UTC.
        let name = if hours == 0 {
            "UTC".to_string()
        } else {
            format!("Etc/GMT{}{}", if hours > 0 { '-' } else { '+' }, hours.abs())
        };
        return validate_tz_name(&name);
    }

    // `UTC±N` form.
    if let Some(rest) = spec.strip_prefix("UTC") {
        if rest.is_empty() {
            return Ok("UTC".to_string());
        }
        if let Ok(hours) = rest.parse::<i32>() {
            let name = if hours == 0 {
                "UTC".to_string()
            } else {
                format!("Etc/GMT{}{}", if hours > 0 { '-' } else { '+' }, hours.abs())
            };
            return validate_tz_name(&name);
        }
    }

    // Fall through: treat as an IANA name.
    validate_tz_name(spec)
}

fn validate_tz_name(name: &str) -> Result<String> {
    let _tz: Tz = name
        .parse()
        .map_err(|_| ParseError::InvalidArg(format!("Unknown timezone: {name}")))?;
    Ok(name.to_string())
}

fn refresh_offset(mut t: DatumTime) -> Result<DatumTime> {
    let tz: Tz = t
        .timezone
        .parse()
        .map_err(|_| ParseError::InvalidArg(format!("Unknown timezone: {}", t.timezone)))?;
    let local = t.utc.with_timezone(&tz);
    t.offset_minutes = local.offset().fix().local_minus_utc() / 60;
    Ok(t)
}

fn rebase_to_timezone(mut t: DatumTime, tz_name: &str) -> Result<DatumTime> {
    let tz: Tz = tz_name
        .parse()
        .map_err(|_| ParseError::InvalidArg(format!("Unknown timezone: {tz_name}")))?;
    let local = t.utc.with_timezone(&tz);
    t.offset_minutes = local.offset().fix().local_minus_utc() / 60;
    t.timezone = tz_name.to_string();
    Ok(t)
}

// ─── time/date application ──────────────────────────────────────────────

fn apply_time_arg(cur: DatumTime, s: &str, uncertainty: &mut Option<Uncertainty>) -> Result<DatumTime> {
    // Try daydumb's structured parser first.
    if let Ok(arg) = parse_time_arg(s) {
        if let Some(u) = arg.uncertainty {
            *uncertainty = Some(u);
        }
        if let Some(abs) = arg.absolute {
            return set_clock(cur, abs);
        }
        if let Some(mins) = arg.minutes_offset {
            return shift_minutes(cur, mins);
        }
    }

    // Relative-with-unit forms: "+10m", "-3hrs", "+87.3", "+3h", "+100s".
    if let Some(rest) = s.strip_prefix('+').or_else(|| s.strip_prefix('-')) {
        let sign = if s.starts_with('-') { -1.0f64 } else { 1.0f64 };
        if let Some(secs) = parse_signed_duration_seconds(rest) {
            let nanos = (sign * secs * 1_000_000_000.0) as i64;
            let shifted = cur.utc + Duration::nanoseconds(nanos);
            return Ok(DatumTime {
                utc: shifted,
                ..cur
            });
        }
    }

    // ISO-8601 datetime (`2010-05-20T10:00:00Z`) — datum's parseTimeStr
    // accepts this. Delegate to chrono.
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(DatumTime {
            utc: dt.with_timezone(&Utc),
            ..cur
        });
    }

    Err(ParseError::InvalidTime(format!(
        "Cannot parse time (NL-parsing not ported — see parity audit): '{s}'"
    )))
}

fn apply_date_arg(
    cur: DatumTime,
    s: &str,
    uncertainty: &mut Option<Uncertainty>,
    tz_name: &str,
) -> Result<DatumTime> {
    if let Ok(arg) = parse_date_arg(s) {
        if let Some(u) = arg.uncertainty {
            *uncertainty = Some(u);
        }
        if let Some(abs) = arg.absolute {
            return set_date(cur, abs, tz_name);
        }
        return shift_days(cur, arg.days_offset, tz_name);
    }

    Err(ParseError::InvalidTime(format!(
        "Cannot parse date (NL-parsing not ported — see parity audit): '{s}'"
    )))
}

/// Parse a signed duration body (`"10m"`, `"3hrs"`, `"87.3"`, `"100s"`)
/// into seconds. Returns `None` for anything our duration parser
/// doesn't recognise (leaves the decision to the caller).
fn parse_signed_duration_seconds(body: &str) -> Option<f64> {
    // Strip trailing space-and-unit words ("minutes", "hours").
    let body = body.trim();
    // If the remainder is a bare number, minutes are the default unit.
    if let Ok(n) = body.parse::<f64>() {
        return Some(n * 60.0);
    }
    // Split number / unit, treating the number part as f64.
    let (num_str, unit) = split_number_unit(body);
    let n: f64 = num_str.parse().ok()?;
    let unit = unit.trim_start_matches(' ').to_ascii_lowercase();
    let multiplier: f64 = match unit.as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
        "m" | "min" | "mins" | "minute" | "minutes" | "" => 60.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
        "d" | "day" | "days" => 86_400.0,
        _ => return None,
    };
    Some(n * multiplier)
}

fn split_number_unit(s: &str) -> (&str, &str) {
    let idx = s
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit() && *c != '.' && *c != '-')
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    (&s[..idx], &s[idx..])
}

fn set_clock(cur: DatumTime, abs: chrono::NaiveTime) -> Result<DatumTime> {
    let tz: Tz = cur
        .timezone
        .parse()
        .map_err(|_| ParseError::InvalidArg(format!("Unknown timezone: {}", cur.timezone)))?;
    let local = cur.utc.with_timezone(&tz).date_naive();
    let local_dt = local.and_time(abs);
    let utc = tz
        .from_local_datetime(&local_dt)
        .single()
        .ok_or_else(|| {
            ParseError::InvalidTime(format!("Ambiguous/non-existent local time: {abs}"))
        })?
        .with_timezone(&Utc);
    Ok(DatumTime { utc, ..cur })
}

fn set_date(cur: DatumTime, abs: chrono::NaiveDate, tz_name: &str) -> Result<DatumTime> {
    let tz: Tz = tz_name
        .parse()
        .map_err(|_| ParseError::InvalidArg(format!("Unknown timezone: {tz_name}")))?;
    let local = cur.utc.with_timezone(&tz).time();
    let local_dt = abs.and_time(local);
    let utc = tz
        .from_local_datetime(&local_dt)
        .single()
        .ok_or_else(|| {
            ParseError::InvalidTime(format!("Ambiguous/non-existent local datetime: {local_dt}"))
        })?
        .with_timezone(&Utc);
    Ok(DatumTime { utc, ..cur })
}

fn shift_minutes(cur: DatumTime, minutes: i64) -> Result<DatumTime> {
    Ok(DatumTime {
        utc: cur.utc + Duration::minutes(minutes),
        ..cur
    })
}

fn shift_days(cur: DatumTime, days: i64, tz_name: &str) -> Result<DatumTime> {
    // Shifting by calendar days requires re-anchoring in the local zone so
    // DST transitions land on the intended wall-clock moment.
    let tz: Tz = tz_name
        .parse()
        .map_err(|_| ParseError::InvalidArg(format!("Unknown timezone: {tz_name}")))?;
    let local = cur.utc.with_timezone(&tz);
    let shifted = local + Duration::days(days);
    let utc = shifted.with_timezone(&Utc);
    Ok(DatumTime { utc, ..cur })
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    /// Fixed "now" matching datum's test harness: 2020-05-10 15:25:30Z.
    fn reference_now() -> DatumTime {
        let utc: DateTime<Utc> = "2020-05-10T15:25:30Z".parse().unwrap();
        DatumTime::from_utc_and_tz(utc, "UTC").unwrap()
    }

    fn build(args: TimeArgs) -> TimeFromArgs {
        args.build(&reference_now()).expect("build")
    }

    fn assert_utc(got: &TimeFromArgs, expected: &str) {
        let expected: DateTime<Utc> = expected.parse().unwrap();
        assert_eq!(got.time.as_ref().unwrap().utc, expected);
    }

    // ── returns "now" unchanged ─────────────────────────────────────────
    #[test]
    fn no_args_returns_reference_as_is() {
        let got = build(TimeArgs::default());
        assert_utc(&got, "2020-05-10T15:25:30Z");
        assert!(got.unmodified);
        assert!(!got.only_date);
    }

    // ── absolute time strings (structured subset) ───────────────────────
    #[test]
    fn absolute_time_hh_mm() {
        let mut a = TimeArgs::default();
        a.time = Some("3:15".into());
        assert_utc(&build(a), "2020-05-10T03:15:00Z");
    }

    #[test]
    fn absolute_time_hhmm() {
        let mut a = TimeArgs::default();
        a.time = Some("315".into());
        assert_utc(&build(a), "2020-05-10T03:15:00Z");
        let mut a = TimeArgs::default();
        a.time = Some("0315".into());
        assert_utc(&build(a), "2020-05-10T03:15:00Z");
        let mut a = TimeArgs::default();
        a.time = Some("2315".into());
        assert_utc(&build(a), "2020-05-10T23:15:00Z");
    }

    #[test]
    fn absolute_time_am_pm() {
        let mut a = TimeArgs::default();
        a.time = Some("3:15am".into());
        assert_utc(&build(a), "2020-05-10T03:15:00Z");
        let mut a = TimeArgs::default();
        a.time = Some("3:15pm".into());
        assert_utc(&build(a), "2020-05-10T15:15:00Z");
    }

    #[test]
    fn iso_datetime_passes_through() {
        let mut a = TimeArgs::default();
        a.time = Some("2010-05-20T10:00:00Z".into());
        assert_utc(&build(a), "2010-05-20T10:00:00Z");
    }

    // ── relative time strings ──────────────────────────────────────────
    #[test]
    fn relative_minutes_default_unit() {
        let mut a = TimeArgs::default();
        a.time = Some("+1".into());
        assert_utc(&build(a), "2020-05-10T15:26:30Z");
        let mut a = TimeArgs::default();
        a.time = Some("-10".into());
        assert_utc(&build(a), "2020-05-10T15:15:30Z");
    }

    #[test]
    fn relative_time_with_unit_suffix() {
        let mut a = TimeArgs::default();
        a.time = Some("+10m".into());
        assert_utc(&build(a), "2020-05-10T15:35:30Z");
        let mut a = TimeArgs::default();
        a.time = Some("+3h".into());
        assert_utc(&build(a), "2020-05-10T18:25:30Z");
        let mut a = TimeArgs::default();
        a.time = Some("-1hr".into());
        assert_utc(&build(a), "2020-05-10T14:25:30Z");
        let mut a = TimeArgs::default();
        a.time = Some("-3hrs".into());
        assert_utc(&build(a), "2020-05-10T12:25:30Z");
        let mut a = TimeArgs::default();
        a.time = Some("+100s".into());
        assert_utc(&build(a), "2020-05-10T15:27:10Z");
    }

    // ── quick flags ────────────────────────────────────────────────────
    #[test]
    fn quick_subtracts_five_minutes_per_unit() {
        let mut a = TimeArgs::default();
        a.quick = 1;
        assert_utc(&build(a), "2020-05-10T15:20:30Z");
        let mut a = TimeArgs::default();
        a.quick = 2;
        assert_utc(&build(a), "2020-05-10T15:15:30Z");
        let mut a = TimeArgs::default();
        a.quick = 6;
        assert_utc(&build(a), "2020-05-10T14:55:30Z");
    }

    #[test]
    fn time_and_quick_compose_in_order() {
        let mut a = TimeArgs::default();
        a.time = Some("10:15".into());
        a.quick = 3;
        // 10:15 minus 15 minutes = 10:00
        assert_utc(&build(a), "2020-05-10T10:00:00Z");
    }

    // ── absolute date (ISO subset) ──────────────────────────────────────
    #[test]
    fn absolute_iso_date_preserves_clock() {
        let mut a = TimeArgs::default();
        a.date = Some("2018-06-30".into());
        // clock preserved at 15:25:30
        assert_utc(&build(a), "2018-06-30T15:25:30Z");
    }

    // ── relative date ───────────────────────────────────────────────────
    #[test]
    fn relative_date_shifts_by_days() {
        let mut a = TimeArgs::default();
        a.date = Some("-1".into());
        assert_utc(&build(a), "2020-05-09T15:25:30Z");
        let mut a = TimeArgs::default();
        a.date = Some("+1".into());
        assert_utc(&build(a), "2020-05-11T15:25:30Z");
        let mut a = TimeArgs::default();
        a.date = Some("-2".into());
        assert_utc(&build(a), "2020-05-08T15:25:30Z");
    }

    // ── yesterday flag ──────────────────────────────────────────────────
    #[test]
    fn yesterday_shifts_by_n_days() {
        let mut a = TimeArgs::default();
        a.yesterday = 1;
        assert_utc(&build(a), "2020-05-09T15:25:30Z");
        let mut a = TimeArgs::default();
        a.yesterday = 5;
        assert_utc(&build(a), "2020-05-05T15:25:30Z");
    }

    #[test]
    fn yesterday_composes_with_date() {
        // TS test: { date: "2010-09-01", yesterday: 1 } → "2010-08-31"
        let mut a = TimeArgs::default();
        a.date = Some("2010-09-01".into());
        a.yesterday = 1;
        assert_utc(&build(a), "2010-08-31T15:25:30Z");
    }

    // ── date + time compose ─────────────────────────────────────────────
    #[test]
    fn date_and_time_compose() {
        let mut a = TimeArgs::default();
        a.date = Some("2018-06-30".into());
        a.time = Some("15:18".into());
        assert_utc(&build(a), "2018-06-30T15:18:00Z");
    }

    // ── omit_timestamp / full_day / onlyDate ───────────────────────────
    #[test]
    fn omit_timestamp_returns_none() {
        let mut a = TimeArgs::default();
        a.omit_timestamp = true;
        let got = build(a);
        assert_eq!(got.time, None);
        assert!(!got.unmodified);
        assert!(!got.only_date);
    }

    #[test]
    fn full_day_sets_only_date() {
        let mut a = TimeArgs::default();
        a.full_day = true;
        assert!(build(a).only_date);
    }

    #[test]
    fn date_alone_sets_only_date() {
        let mut a = TimeArgs::default();
        a.date = Some("2023-10-23".into());
        assert!(build(a).only_date);
    }

    #[test]
    fn yesterday_alone_sets_only_date() {
        let mut a = TimeArgs::default();
        a.yesterday = 1;
        assert!(build(a).only_date);
    }

    #[test]
    fn date_plus_time_is_not_only_date() {
        let mut a = TimeArgs::default();
        a.date = Some("2023-10-23".into());
        a.time = Some("14:37".into());
        assert!(!build(a).only_date);
    }

    #[test]
    fn full_day_wins_over_clock_input() {
        let mut a = TimeArgs::default();
        a.date = Some("-1".into());
        a.time = Some("5am".into());
        a.full_day = true;
        assert!(build(a).only_date);
    }

    // ── unmodified flag ─────────────────────────────────────────────────
    #[test]
    fn any_modifier_clears_unmodified() {
        assert!(build(TimeArgs::default()).unmodified);

        let mut a = TimeArgs::default();
        a.date = Some("-1".into());
        assert!(!build(a).unmodified);

        let mut a = TimeArgs::default();
        a.time = Some("3:45".into());
        assert!(!build(a).unmodified);

        let mut a = TimeArgs::default();
        a.yesterday = 1;
        assert!(!build(a).unmodified);

        let mut a = TimeArgs::default();
        a.quick = 2;
        assert!(!build(a).unmodified);

        let mut a = TimeArgs::default();
        a.omit_timestamp = true;
        assert!(!build(a).unmodified);

        // "-0" counts as a modifier even though the instant is the same.
        let mut a = TimeArgs::default();
        a.time = Some("-0".into());
        assert!(!build(a).unmodified);
    }

    // ── timezone handling ───────────────────────────────────────────────
    #[test]
    fn numeric_offset_plus_four() {
        // TS: { timezone: "+4", time: "10:00" } → utc: 06:00, o: 4
        let mut a = TimeArgs::default();
        a.timezone = Some("+4".into());
        a.time = Some("10:00".into());
        let got = build(a);
        assert_utc(&got, "2020-05-10T06:00:00Z");
        assert_eq!(got.time.unwrap().offset_minutes, 4 * 60);
    }

    #[test]
    fn numeric_offset_minus_four() {
        let mut a = TimeArgs::default();
        a.timezone = Some("-4".into());
        a.time = Some("10:00".into());
        let got = build(a);
        assert_utc(&got, "2020-05-10T14:00:00Z");
        assert_eq!(got.time.unwrap().offset_minutes, -4 * 60);
    }

    #[test]
    fn zero_offset_is_utc() {
        let mut a = TimeArgs::default();
        a.timezone = Some("0".into());
        a.time = Some("10:00".into());
        let got = build(a);
        assert_utc(&got, "2020-05-10T10:00:00Z");
        assert_eq!(got.time.unwrap().offset_minutes, 0);
    }

    #[test]
    fn iana_tz_before_and_after_dst() {
        // Chicago DST in 2018 began 2018-03-11. Before: UTC-6; after: UTC-5.
        let utc: DateTime<Utc> = "2020-05-10T15:25:30Z".parse().unwrap();
        let reference = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();

        let mut a = TimeArgs::default();
        a.timezone = Some("America/Chicago".into());
        a.date = Some("2018-03-10".into());
        a.time = Some("10:00".into());
        let got = a.build(&reference).unwrap();
        assert_utc(&got, "2018-03-10T16:00:00Z");
        assert_eq!(got.time.as_ref().unwrap().offset_minutes, -6 * 60);

        let mut a = TimeArgs::default();
        a.timezone = Some("America/Chicago".into());
        a.date = Some("2018-03-12".into());
        a.time = Some("10:00".into());
        let got = a.build(&reference).unwrap();
        assert_utc(&got, "2018-03-12T15:00:00Z");
        assert_eq!(got.time.as_ref().unwrap().offset_minutes, -5 * 60);
    }

    #[test]
    fn unknown_timezone_errors() {
        let mut a = TimeArgs::default();
        a.timezone = Some("rubbish/timezone".into());
        assert!(matches!(
            a.build(&reference_now()),
            Err(ParseError::InvalidArg(_))
        ));
    }

    #[test]
    fn non_ported_nl_time_errors() {
        let mut a = TimeArgs::default();
        a.time = Some("half past nothing".into());
        assert!(matches!(
            a.build(&reference_now()),
            Err(ParseError::InvalidTime(_))
        ));

        let mut a = TimeArgs::default();
        a.date = Some("the end of the universe".into());
        assert!(matches!(
            a.build(&reference_now()),
            Err(ParseError::InvalidTime(_))
        ));
    }

    #[test]
    fn uncertainty_surfaces_from_time_arg() {
        let mut a = TimeArgs::default();
        a.time = Some("10:00±5m".into());
        let got = build(a);
        let u = got.uncertainty.unwrap();
        assert_eq!(u.seconds, 300);
    }
}
