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
use crate::input::flag_registry::{FlagAction, FlagSpec};
use crate::time::{parse_date_arg, parse_time_arg, DatumTime, Direction, Uncertainty};

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

// ─── flag registry ──────────────────────────────────────────────────────

/// Canonical flag set for the timing concern.
///
/// `parse.rs` lifts these onto a per-command target via
/// [`crate::input::flag_registry::lift`]. Commands that don't accept all
/// of these (e.g. logging verbs don't expose `-D/--full-day` today) pick
/// the relevant subset at registry-build time.
pub static FLAG_SPECS: &[FlagSpec<TimeArgs>] = &[
    FlagSpec {
        short: Some('y'),
        long: Some("yesterday"),
        action: FlagAction::Switch(|a| {
            a.yesterday = a.yesterday.saturating_add(1);
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('q'),
        long: Some("quick"),
        action: FlagAction::Switch(|a| {
            a.quick = a.quick.saturating_add(1);
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('d'),
        long: Some("date"),
        action: FlagAction::Value(|a, v| {
            a.date = Some(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('t'),
        long: Some("time"),
        action: FlagAction::Value(|a, v| {
            a.time = Some(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('z'),
        long: Some("timezone"),
        action: FlagAction::Value(|a, v| {
            a.timezone = Some(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('T'),
        long: Some("no-timestamp"),
        action: FlagAction::Switch(|a| {
            a.omit_timestamp = true;
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('D'),
        long: Some("full-day"),
        action: FlagAction::Switch(|a| {
            a.full_day = true;
            Ok(())
        }),
    },
];

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
/// * `"system"` (case-insensitive): the OS-resolved local timezone.
///   Native builds only — WASM builds error since the host has no
///   single ambient zone to resolve to.
pub fn resolve_timezone(spec: &str) -> Result<String> {
    if spec.is_empty() {
        return Err(ParseError::InvalidArg("Empty timezone".to_string()));
    }

    // `"system"` literal → OS local zone.
    if spec.eq_ignore_ascii_case("system") {
        #[cfg(feature = "native")]
        {
            let name = iana_time_zone::get_timezone().map_err(|e| {
                ParseError::InvalidArg(format!("Cannot resolve system timezone: {e}"))
            })?;
            return validate_tz_name(&name);
        }
        #[cfg(not(feature = "native"))]
        {
            return Err(ParseError::InvalidArg(
                "'system' timezone is not available in this (non-native) build".to_string(),
            ));
        }
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
            format!(
                "Etc/GMT{}{}",
                if hours > 0 { '-' } else { '+' },
                hours.abs()
            )
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
                format!(
                    "Etc/GMT{}{}",
                    if hours > 0 { '-' } else { '+' },
                    hours.abs()
                )
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

fn apply_time_arg(
    cur: DatumTime,
    s: &str,
    uncertainty: &mut Option<Uncertainty>,
) -> Result<DatumTime> {
    // Try daydumb's structured parser first.
    if let Ok(arg) = parse_time_arg(s) {
        if let Some(u) = arg.uncertainty {
            *uncertainty = Some(u);
        }
        // ISO-8601 instant (`...Z` / `...+02:00`) — the wall clock and zone
        // are fully resolved by the literal. We keep the entry's `timezone`
        // field (so `-z` still drives the IANA name) but the UTC moment is
        // the literal's instant. Direction marker is meaningless on a
        // pinned instant — silently ignored.
        if let Some(instant) = arg.iso_instant {
            return Ok(DatumTime {
                utc: instant,
                ..cur
            });
        }
        // ISO-8601 with no zone suffix (`2024-03-16T15:30:00`) — interpret
        // as local-in-the-current-effective-zone (i.e. `cur.timezone`,
        // which already reflects any `-z` override). Direction ignored:
        // the wall clock is fully specified.
        if let Some(naive) = arg.iso_naive {
            return set_naive_datetime(cur, naive);
        }
        if let Some(abs) = arg.absolute {
            return set_clock(cur, abs, arg.direction);
        }
        if let Some(mins) = arg.minutes_offset {
            // Relative offsets are already directional — ignore suffix.
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
        // Direction marker on year-pinned ISO dates and on relative
        // offsets has no effect — the moment is fully specified. The
        // hint becomes load-bearing once the parser produces year-less
        // forms (compact `apr29`, `jan5`, …).
        if let Some(abs) = arg.absolute {
            let _ = arg.direction;
            return set_date(cur, abs, tz_name);
        }
        let _ = arg.direction;
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

fn set_naive_datetime(cur: DatumTime, naive: chrono::NaiveDateTime) -> Result<DatumTime> {
    let tz: Tz = cur
        .timezone
        .parse()
        .map_err(|_| ParseError::InvalidArg(format!("Unknown timezone: {}", cur.timezone)))?;
    let utc = tz
        .from_local_datetime(&naive)
        .single()
        .ok_or_else(|| {
            ParseError::InvalidTime(format!("Ambiguous/non-existent local datetime: {naive}"))
        })?
        .with_timezone(&Utc);
    Ok(DatumTime { utc, ..cur })
}

fn set_clock(
    cur: DatumTime,
    abs: chrono::NaiveTime,
    direction: Option<Direction>,
) -> Result<DatumTime> {
    let tz: Tz = cur
        .timezone
        .parse()
        .map_err(|_| ParseError::InvalidArg(format!("Unknown timezone: {}", cur.timezone)))?;
    let now_local = cur.utc.with_timezone(&tz);
    let mut local_dt = now_local.date_naive().and_time(abs);
    // Direction-suffix disambiguation: shift by ±1 day when the candidate
    // sits on the wrong side of "now". Tie-break (candidate == now) goes
    // *with* the requested direction — `+` even at exact equality picks
    // the future, `-` picks the past.
    if let Some(dir) = direction {
        let now_naive = now_local.naive_local();
        match dir {
            Direction::Future if local_dt <= now_naive => {
                local_dt += chrono::Duration::days(1);
            }
            Direction::Past if local_dt >= now_naive => {
                local_dt -= chrono::Duration::days(1);
            }
            _ => {}
        }
    }
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

    #[test]
    fn iso_datetime_offset_suffix_resolves_to_utc() {
        let mut a = TimeArgs::default();
        a.time = Some("2024-03-16T15:30:00+02:00".into());
        // 15:30 +02:00 ↔ 13:30 UTC.
        assert_utc(&build(a), "2024-03-16T13:30:00Z");
    }

    #[test]
    fn iso_datetime_high_precision_round_trips() {
        let mut a = TimeArgs::default();
        a.time = Some("2024-03-16T15:30:00.123456789Z".into());
        let got = build(a);
        let utc = got.time.unwrap().utc;
        assert_eq!(utc.timestamp(), 1_710_603_000);
        assert_eq!(utc.timestamp_subsec_nanos(), 123_456_789);
    }

    #[test]
    fn iso_datetime_no_tz_uses_effective_zone() {
        // No-tz datetime + `-z America/New_York`: the wall clock is read in
        // NY; 2024-03-16 15:30 EDT ↔ 19:30 UTC (DST in effect by 16 March).
        let mut a = TimeArgs::default();
        a.timezone = Some("America/New_York".into());
        a.time = Some("2024-03-16T15:30:00".into());
        let got = build(a);
        assert_utc(&got, "2024-03-16T19:30:00Z");
        assert_eq!(got.time.unwrap().offset_minutes, -4 * 60);
    }

    #[test]
    fn iso_instant_with_explicit_z_overrides_tz_for_instant() {
        // `-t 2024-03-16T15:30:00Z -z America/New_York`: the literal's `Z`
        // wins for the actual instant; `-z` still drives the `timezone`
        // field so the entry stores its display zone.
        let mut a = TimeArgs::default();
        a.timezone = Some("America/New_York".into());
        a.time = Some("2024-03-16T15:30:00Z".into());
        let got = build(a);
        assert_utc(&got, "2024-03-16T15:30:00Z");
        assert_eq!(got.time.as_ref().unwrap().timezone, "America/New_York");
        // Offset reflects NY at that instant (EDT = -4h on 2024-03-16).
        assert_eq!(got.time.unwrap().offset_minutes, -4 * 60);
    }

    // ── `-z system` literal ────────────────────────────────────────────
    #[test]
    #[cfg(feature = "native")]
    fn z_system_resolves_to_os_local_zone() {
        let expected = iana_time_zone::get_timezone().expect("system tz available");
        for spec in ["system", "SYSTEM", "System", "sYsTeM"] {
            let resolved = resolve_timezone(spec).expect("resolve");
            assert_eq!(
                resolved, expected,
                "spec {spec:?} should resolve to OS local zone"
            );
        }
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

    // ── direction suffix `+` / `-` (frozen-clock harness) ──────────────
    //
    // Reference clock: 2020-05-10T15:25:30Z. Tests cover all three
    // boundaries called out in the spec:
    //   * candidate strictly before "now" (e.g. `3+` → tomorrow)
    //   * candidate strictly after  "now" (e.g. `18-` → yesterday)
    //   * candidate at exact tie         (use a tailored reference time)
    // and verify that the hint is ignored for already-pinned forms.

    fn build_at(args: TimeArgs, reference: &DatumTime) -> TimeFromArgs {
        args.build(reference).expect("build")
    }

    #[test]
    fn direction_future_past_now_picks_tomorrow() {
        // `-t 3+` at 15:25 → today's 03:00 already passed → tomorrow 03:00.
        let mut a = TimeArgs::default();
        a.time = Some("3+".into());
        assert_utc(&build(a), "2020-05-11T03:00:00Z");
    }

    #[test]
    fn direction_future_before_now_picks_today() {
        // `-t 18+` at 15:25 → today's 18:00 still ahead → today 18:00.
        let mut a = TimeArgs::default();
        a.time = Some("18+".into());
        assert_utc(&build(a), "2020-05-10T18:00:00Z");
    }

    #[test]
    fn direction_past_past_now_picks_today() {
        // `-t 3-` at 15:25 → today's 03:00 already passed → today 03:00.
        let mut a = TimeArgs::default();
        a.time = Some("3-".into());
        assert_utc(&build(a), "2020-05-10T03:00:00Z");
    }

    #[test]
    fn direction_past_before_now_picks_yesterday() {
        // `-t 18-` at 15:25 → today's 18:00 still ahead → yesterday 18:00.
        let mut a = TimeArgs::default();
        a.time = Some("18-".into());
        assert_utc(&build(a), "2020-05-09T18:00:00Z");
    }

    #[test]
    fn direction_future_at_tie_picks_tomorrow() {
        // Reference at exactly 03:00:00 → `-t 3+` resolves into the future,
        // i.e. tomorrow 03:00. Documented tie-break.
        let utc: DateTime<Utc> = "2020-05-10T03:00:00Z".parse().unwrap();
        let reference = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        let mut a = TimeArgs::default();
        a.time = Some("3+".into());
        assert_utc(&build_at(a, &reference), "2020-05-11T03:00:00Z");
    }

    #[test]
    fn direction_past_at_tie_picks_yesterday() {
        // Symmetric tie-break for `-`: at exactly 03:00:00, `-t 3-`
        // resolves into the past (yesterday 03:00).
        let utc: DateTime<Utc> = "2020-05-10T03:00:00Z".parse().unwrap();
        let reference = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        let mut a = TimeArgs::default();
        a.time = Some("3-".into());
        assert_utc(&build_at(a, &reference), "2020-05-09T03:00:00Z");
    }

    #[test]
    fn direction_one_second_before_candidate_treated_as_past() {
        // 02:59:59 — candidate 03:00 is one second in the future.
        // `-t 3+` keeps today; `-t 3-` rolls back to yesterday.
        let utc: DateTime<Utc> = "2020-05-10T02:59:59Z".parse().unwrap();
        let reference = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        let mut a = TimeArgs::default();
        a.time = Some("3+".into());
        assert_utc(&build_at(a, &reference), "2020-05-10T03:00:00Z");
        let mut a = TimeArgs::default();
        a.time = Some("3-".into());
        assert_utc(&build_at(a, &reference), "2020-05-09T03:00:00Z");
    }

    #[test]
    fn direction_one_second_after_candidate_treated_as_future() {
        // 03:00:01 — candidate 03:00 is one second in the past.
        // `-t 3+` rolls forward to tomorrow; `-t 3-` keeps today.
        let utc: DateTime<Utc> = "2020-05-10T03:00:01Z".parse().unwrap();
        let reference = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        let mut a = TimeArgs::default();
        a.time = Some("3+".into());
        assert_utc(&build_at(a, &reference), "2020-05-11T03:00:00Z");
        let mut a = TimeArgs::default();
        a.time = Some("3-".into());
        assert_utc(&build_at(a, &reference), "2020-05-10T03:00:00Z");
    }

    #[test]
    fn direction_meridian_form_resolves_correctly() {
        // `-t 9pm-` at 15:25 → today's 21:00 is still ahead → yesterday 21:00.
        let mut a = TimeArgs::default();
        a.time = Some("9pm-".into());
        assert_utc(&build(a), "2020-05-09T21:00:00Z");
        // `-t 9am+` at 15:25 → today's 09:00 already passed → tomorrow 09:00.
        let mut a = TimeArgs::default();
        a.time = Some("9am+".into());
        assert_utc(&build(a), "2020-05-11T09:00:00Z");
    }

    #[test]
    fn direction_hh_mm_form_resolves_correctly() {
        // `-t 18:30+` at 15:25 → today's 18:30 still ahead → today 18:30.
        let mut a = TimeArgs::default();
        a.time = Some("18:30+".into());
        assert_utc(&build(a), "2020-05-10T18:30:00Z");
        // `-t 14:00+` at 15:25 → 14:00 already passed → tomorrow 14:00.
        let mut a = TimeArgs::default();
        a.time = Some("14:00+".into());
        assert_utc(&build(a), "2020-05-11T14:00:00Z");
    }

    #[test]
    fn direction_ignored_on_relative_offset() {
        // `-t +5+` is `+5` (relative future) with redundant `+` hint —
        // the hint is meaningless on a fully-resolved offset.
        let mut a = TimeArgs::default();
        a.time = Some("+5+".into());
        assert_utc(&build(a), "2020-05-10T15:30:30Z");
        let mut a = TimeArgs::default();
        a.time = Some("-30-".into());
        assert_utc(&build(a), "2020-05-10T14:55:30Z");
    }

    #[test]
    fn direction_ignored_on_iso_instant() {
        // ISO instant is fully pinned — direction hint cannot change it.
        let mut a = TimeArgs::default();
        a.time = Some("2024-03-16T15:30:00Z+".into());
        assert_utc(&build(a), "2024-03-16T15:30:00Z");
    }

    #[test]
    fn direction_ignored_on_iso_naive() {
        // Naive ISO datetime is wall-clock-pinned — direction hint cannot
        // change which day it lands on.
        let mut a = TimeArgs::default();
        a.time = Some("2024-03-16T15:30:00-".into());
        assert_utc(&build(a), "2024-03-16T15:30:00Z");
    }

    #[test]
    fn direction_with_uncertainty_carries_both() {
        // `-t 3+5m+` → primary 3am with future direction, ±5m uncertainty.
        // 03:00 already past at 15:25 → tomorrow 03:00.
        let mut a = TimeArgs::default();
        a.time = Some("3+-5m+".into());
        let got = build(a);
        assert_utc(&got, "2020-05-11T03:00:00Z");
        let u = got.uncertainty.expect("uncertainty");
        assert_eq!(u.seconds, 300);
    }

    #[test]
    fn direction_double_marker_errors() {
        let mut a = TimeArgs::default();
        a.time = Some("3+-".into());
        assert!(matches!(
            a.build(&reference_now()),
            Err(ParseError::InvalidTime(_))
        ));
        let mut a = TimeArgs::default();
        a.time = Some("3-+".into());
        assert!(matches!(
            a.build(&reference_now()),
            Err(ParseError::InvalidTime(_))
        ));
    }

    #[test]
    fn direction_in_iana_zone_uses_local_clock() {
        // 15:25 UTC = 11:25 EDT. With `-z America/New_York -t 9am+`, the
        // candidate 09:00 EDT has already passed → tomorrow's 09:00 EDT.
        // 2020-05-11 09:00 EDT = 2020-05-11 13:00 UTC.
        let mut a = TimeArgs::default();
        a.timezone = Some("America/New_York".into());
        a.time = Some("9am+".into());
        assert_utc(&build(a), "2020-05-11T13:00:00Z");
    }

    #[test]
    fn date_direction_suffix_on_iso_date_is_no_op() {
        // Year-pinned ISO date: direction marker has nothing to disambiguate.
        let mut a = TimeArgs::default();
        a.date = Some("2018-06-30+".into());
        assert_utc(&build(a), "2018-06-30T15:25:30Z");
        let mut a = TimeArgs::default();
        a.date = Some("2018-06-30-".into());
        assert_utc(&build(a), "2018-06-30T15:25:30Z");
    }

    #[test]
    fn date_direction_suffix_on_relative_offset_is_no_op() {
        // Relative offsets are already directional.
        let mut a = TimeArgs::default();
        a.date = Some("+1+".into());
        assert_utc(&build(a), "2020-05-11T15:25:30Z");
        let mut a = TimeArgs::default();
        a.date = Some("-1-".into());
        assert_utc(&build(a), "2020-05-09T15:25:30Z");
    }

    #[test]
    fn date_direction_double_marker_errors() {
        let mut a = TimeArgs::default();
        a.date = Some("2018-06-30+-".into());
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
