//! `timeArgs` — the `-d`/`-y*`/`-t`/`-q*`/`-z`/`-D`/`-T` flag group, as
//! datum's `src/input/timeArgs.ts` had it.
//!
//! # Design
//!
//! The flags are composed in one place, [`TimeBuilder`]: [`TimeArgs::builder`]
//! reads them into one, and [`TimeArgs::build`] builds it at a reference
//! time the host supplies (the TS version read the clock through luxon;
//! daydumb must stay WASM-safe). `crates/datum-parse/tests/time_flag_grid.rs`
//! holds the two ways in to the same instant.
//!
//! # Non-ports
//!
//! * Chrono-node natural-language parsing (`"yesterday at noon"`,
//!   `"june 18"`, `"2 days ago"`, …). The parity audit lists this as an
//!   explicit non-port — daydumb returns `InvalidTime` for those
//!   forms, and callers can pre-process NL inputs before handing off.
//!   Single-word day forms (`yesterday`, `sunday`) are *not* part of the
//!   non-port: `parse_date` handles them from a bounded vocabulary,
//!   and `-t` reaches them through its date fallback (da-0gs4).
//!
//! See `docs/parity-audit/parsing.md` ("`timeArgs.ts`") for the spec.

use chrono_tz::TZ_VARIANTS;

use crate::error::{ParseError, Result};
use crate::input::flag_registry::{FlagAction, FlagSpec};
use crate::time::{
    fixed_offset_minutes, parse_date, parse_time, DatumTime, TimeBuilder, TimeCtx, Uncertainty,
    Zone,
};

/// Parsed shape of the timing flag group.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimeArgs {
    /// `-d` / `--date`: relative day offset (`+1`, `-1`) or ISO date.
    pub date: Option<String>,
    /// `-y` / `--yesterday`: days-back counter. Stackable (`-yy` → 2).
    pub yesterday: u32,
    /// `-t` / `--time`: HHMM / HH:MM / am-pm / `+N` / `-N`.
    pub time: Option<String>,
    /// Further relative `-t` values (`+N` / `-N`), applied after `time`.
    /// A relative `-t` stacks the way `-q` does, and an absolute one
    /// replaces only the absolute, so an alias-bound `-t +5` composes
    /// with a call-site `-t +10` to `+15` (Rule 15, da-snlq1).
    pub time_offsets: Vec<String>,
    /// `-q` / `--quick`: 5-min-back counter. Stackable (`-qqq` → 15 min).
    pub quick: u32,
    /// `-z` / `--timezone`: IANA name (`America/Chicago`) or numeric UTC
    /// offset (`"+4"`, `"-7"`, `"0"`). Empty → host locale.
    pub timezone: Option<String>,
    /// `-D` / `--full-day`: force a date-only entry.
    pub full_day: bool,
    /// `-T` / `--omit-timestamp`: no time recorded.
    pub omit_timestamp: bool,
    /// Read a wall time a fall-back night repeats as its later instant
    /// ([`TimeBuilder::later`]): no flag, the answer to datum's question.
    pub later: bool,
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
    /// `-y*` / `-t <day>` without any clock-level input.
    pub only_date: bool,
    /// The `±N[unit]` typed on the time or date, else `-q`'s own.
    /// `None` when the moment is exact.
    pub uncertainty: Option<Uncertainty>,
}

impl TimeArgs {
    /// Whether a clock- or date-moving flag (`-t`/`-d`/`-y*`/`-q*`/`-D`) is
    /// set: what places a timeless verb's entry in time. `-z` only re-zones.
    pub fn places(&self) -> bool {
        self.time.is_some()
            || self.date.is_some()
            || self.yesterday > 0
            || self.quick > 0
            || self.full_day
    }

    /// Every `-t` value, in the order it applies: `time`, then the offsets.
    pub fn time_values(&self) -> impl Iterator<Item = &String> {
        self.time.iter().chain(&self.time_offsets)
    }

    /// Take one more `-t` value: a relative one stacks, an absolute one
    /// replaces only the absolute (see [`TimeArgs::time_offsets`]).
    pub fn push_time(&mut self, v: &str) {
        let relative = |s: &str| s.starts_with(['+', '-']);
        match self.time.take() {
            Some(prev) if relative(v) => {
                self.time = Some(prev);
                self.time_offsets.push(v.to_string());
            }
            Some(prev) if relative(&prev) => {
                self.time_offsets.insert(0, prev);
                self.time = Some(v.to_string());
            }
            _ => self.time = Some(v.to_string()),
        }
    }

    /// The flags read into a [`TimeBuilder`], in the order it composes
    /// them: `-t`, `-q`, `-d`, `-y`. `ctx` is what a weekday or a year-less
    /// date is measured from. `-z` is the caller's: it names the zone the
    /// builder is built in. A relative time beside a named day needs a
    /// clock ([`TimeBuilder::shift_needs_clock`]).
    pub fn builder(&self, ctx: TimeCtx) -> Result<TimeBuilder> {
        self.builder_anchored(ctx)?.shift_needs_clock()
    }

    /// [`builder`](Self::builder) for an anchor that is a clock of its own,
    /// an entry's time (retime): a shift beside a day has one to move.
    pub fn builder_anchored(&self, ctx: TimeCtx) -> Result<TimeBuilder> {
        let mut builder = TimeBuilder {
            timezone: self.timezone.as_deref().map(resolve_timezone).transpose()?,
            later: self.later,
            ..TimeBuilder::default()
        }
        .with_quick_count(i64::from(self.quick))
        .with_quick_uncertainty(self.quick)
        .with_full_day(self.full_day);
        for t in self.time_values() {
            builder = builder.with_time_arg(parse_time(t, ctx)?);
        }
        if let Some(d) = &self.date {
            builder = builder.with_date_arg(parse_date(d, ctx)?);
        }
        Ok(builder.with_yesterday_count(i64::from(self.yesterday)))
    }

    /// Build a `TimeFromArgs` relative to `reference` — the caller-
    /// supplied "now". The reference carries the effective timezone, and
    /// [`TimeArgs::timezone`] overrides it. The moment keeps its clock when
    /// it is date-only: `only_date` says so, and the caller picks the day.
    pub fn build(&self, reference: &DatumTime) -> Result<TimeFromArgs> {
        if self.omit_timestamp {
            return Ok(TimeFromArgs {
                time: None,
                unmodified: false,
                only_date: false,
                uncertainty: None,
            });
        }

        // `None` means the reference carries no zone name of its own and
        // none was requested — its fixed offset stands in for one.
        let name = match self.timezone.as_deref() {
            Some(s) => Some(resolve_timezone(s)?),
            None => reference.timezone.clone(),
        };
        let zone = match &name {
            Some(name) => Zone::named(name)?,
            None => Zone::of(reference)?,
        };
        let builder = self.builder(TimeCtx::at(zone.local_naive(reference.utc)))?;

        Ok(TimeFromArgs {
            unmodified: self.time_values().next().is_none()
                && self.quick == 0
                && self.date.is_none()
                && self.yesterday == 0,
            only_date: builder.is_only_date(),
            uncertainty: Some(builder.uncertainty()).filter(|u| *u != Uncertainty::exact()),
            time: Some(DatumTime {
                uncertainty: reference.uncertainty.clone(),
                ..builder.build_at(reference.utc, zone, name)
            }),
        })
    }
}

// ─── flag registry ──────────────────────────────────────────────────────

/// Canonical flag set for the timing concern.
///
/// `parse.rs` lifts these onto a per-command target via
/// [`crate::input::flag_registry::lift`]. Commands that don't accept all
/// of these pick the relevant subset at registry-build time.
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
            a.push_time(v);
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

/// Resolve the `-z` / `--timezone` input into a name [`Zone::named`] reads.
///
/// Accepted forms (matching `src/time/getTimezone.ts`):
///
/// * IANA name (`"America/Chicago"`, `"UTC"`, …), in any case: `"utc"` and
///   `"europe/berlin"` are spelled back `"UTC"` and `"Europe/Berlin"`.
/// * A fixed offset: `"+4"`, `"-7"`, `"0"`, `"+5:30"`, `"+0530"`, `"UTC-7"`,
///   `"UTC+5:30"`, spelled back as `UTC±H[:MM]`. An entry written in one
///   stores the offset and no zone name (da-kvosn).
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
            return iana_zone(&name);
        }
        #[cfg(not(feature = "native"))]
        {
            return Err(ParseError::InvalidArg(
                "'system' timezone is not available in this (non-native) build".to_string(),
            ));
        }
    }

    if let Some(minutes) = fixed_offset_minutes(spec) {
        if minutes.abs() >= 24 * 60 {
            return Err(ParseError::InvalidArg(format!(
                "Timezone offset out of range: {spec}"
            )));
        }
        let (h, m) = (minutes.abs() / 60, minutes.abs() % 60);
        let sign = if minutes < 0 { '-' } else { '+' };
        return Ok(match m {
            0 => format!("UTC{sign}{h}"),
            _ => format!("UTC{sign}{h}:{m:02}"),
        });
    }

    // Fall through: treat as an IANA name.
    iana_zone(spec)
}

/// The zone's canonical spelling, matched without regard to case: a zone
/// has one spelling, so `utc` and `europe/berlin` can only mean `UTC` and
/// `Europe/Berlin`, which is what is stored however it was typed (da-a32ze).
/// A name that is no zone is refused offering the nearest that are.
pub fn iana_zone(name: &str) -> Result<String> {
    TZ_VARIANTS
        .iter()
        .find(|tz| tz.name().eq_ignore_ascii_case(name))
        .map(|tz| tz.name().to_string())
        .ok_or_else(|| {
            let nearest = nearest_zones(name).join(", ");
            ParseError::InvalidArg(format!("Unknown timezone: {name} (nearest: {nearest})"))
        })
}

/// The three zones nearest `name` by edit distance, to the whole name or to
/// its last part, so `Tokyo` and `Asia/Tokio` both find `Asia/Tokyo`.
fn nearest_zones(name: &str) -> Vec<&'static str> {
    let name = name.to_ascii_lowercase();
    let mut zones: Vec<(usize, &'static str)> = TZ_VARIANTS
        .iter()
        .map(|tz| {
            let zone = tz.name().to_ascii_lowercase();
            let city = zone.rsplit('/').next().unwrap_or(&zone);
            let far = edit_distance(&name, &zone).min(edit_distance(&name, city));
            (far, tz.name())
        })
        .collect();
    zones.sort();
    zones.into_iter().take(3).map(|(_, zone)| zone).collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let cost = diagonal + usize::from(x != *y);
            diagonal = row[j + 1];
            row[j + 1] = cost.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
// Test setups in this module use `let mut a = TimeArgs::default(); a.x = ...;`
// per-field assignment for readability — the rows mirror datum's CLI flag
// layout. Struct-literal form would require enumerating every field of
// TimeArgs at every call site, which obscures what each test is exercising.
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

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
        assert_eq!(
            got.time.as_ref().unwrap().timezone.as_deref(),
            Some("America/New_York")
        );
        // Offset reflects NY at that instant (EDT = -4h on 2024-03-16).
        assert_eq!(got.time.unwrap().offset_minutes, -4 * 60);
    }

    // ── `-z system` literal ────────────────────────────────────────────
    #[test]
    #[cfg(feature = "native")]
    fn z_system_resolves_to_os_local_zone() {
        // `resolve_timezone("system")` is a thin wrapper around
        // `iana_time_zone::get_timezone()`, so the two must agree in lockstep:
        // both succeed with the same value, or both fail. This makes the test
        // deterministic on every host — including stripped sandboxes with no
        // `/etc/localtime` — without skipping or env manipulation.
        let expected = iana_time_zone::get_timezone();
        for spec in ["system", "SYSTEM", "System", "sYsTeM"] {
            let resolved = resolve_timezone(spec);
            match (&expected, &resolved) {
                (Ok(e), Ok(r)) => assert_eq!(
                    r, e,
                    "spec {spec:?} should resolve to the OS local zone"
                ),
                (Err(_), Err(_)) => {
                    // Host has no configured IANA zone; resolver correctly errors.
                }
                (Ok(e), Err(r)) => panic!(
                    "spec {spec:?}: iana_time_zone returned Ok({e:?}) but resolver returned Err({r:?})"
                ),
                (Err(e), Ok(r)) => panic!(
                    "spec {spec:?}: iana_time_zone returned Err({e:?}) but resolver returned Ok({r:?})"
                ),
            }
        }
    }

    /// da-kvosn: a fixed offset resolves to `UTC±H[:MM]`, the name v2
    /// writes, in ISO's sign; never to an `Etc/GMT` name, which reads
    /// backwards and has whole hours only.
    #[test]
    fn z_fixed_offsets_resolve_to_utc_names() {
        for (spec, name) in [
            ("+2", "UTC+2"),
            ("2", "UTC+2"),
            ("-7", "UTC-7"),
            ("0", "UTC+0"),
            ("UTC+2", "UTC+2"),
            ("UTC-9:30", "UTC-9:30"),
            ("+5:30", "UTC+5:30"),
            ("+0530", "UTC+5:30"),
            ("+05:45", "UTC+5:45"),
            ("UTC", "UTC"),
            ("Etc/GMT-2", "Etc/GMT-2"),
        ] {
            assert_eq!(resolve_timezone(spec).unwrap(), name, "-z {spec}");
        }
        for spec in ["+24", "-25:00", "+5:60", "+5:3", "+123", "UTC+x"] {
            assert!(resolve_timezone(spec).is_err(), "-z {spec} is refused");
        }
        // Each name reads back as the offset it says, `Etc/GMT`'s POSIX sign
        // included, so a row holding either resolves (no migration).
        let at = DatumTime::from_utc_and_tz;
        let utc = "2024-06-18T20:42:00Z".parse().unwrap();
        assert_eq!(at(utc, "UTC+2").unwrap().offset_minutes, 120);
        assert_eq!(at(utc, "UTC-9:30").unwrap().offset_minutes, -570);
        assert_eq!(at(utc, "Etc/GMT-2").unwrap().offset_minutes, 120);
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

    /// da-e93nr: a `-q` or a `-t ±N` beside a named day reads two ways,
    /// shift then day or day then shift, so with no clock it is refused.
    /// An entry's own time is a clock, and retime composes them as before.
    #[test]
    fn a_relative_time_beside_a_named_day_needs_a_clock() {
        let args = |date: Option<&str>, yesterday, quick, times: &[&str]| {
            let mut a = TimeArgs::default();
            (a.date, a.yesterday, a.quick) = (date.map(Into::into), yesterday, quick);
            times.iter().for_each(|t| a.push_time(t));
            a
        };
        for a in [
            args(Some("2020-05-05"), 0, 0, &["-6"]),
            args(Some("2020-05-05"), 0, 1, &[]),
            args(Some("today"), 0, 1, &[]),
            args(None, 1, 0, &["-6"]),
            args(None, 1, 1, &[]),
            args(None, 0, 0, &["-6", "yesterday"]),
            args(None, 0, 1, &["sunday"]),
        ] {
            let err = a.build(&reference_now()).expect_err("no clock");
            assert!(err.to_string().contains("needs a clock"), "{a:?}: {err}");
            let ctx = TimeCtx::at(reference_now().utc.naive_utc());
            assert!(a.builder_anchored(ctx).is_ok(), "{a:?}");
        }
        // A clock, by any spelling, or no day.
        for (a, at) in [
            (
                args(Some("2020-05-05"), 0, 0, &["1830", "-6"]),
                "05T18:24:00",
            ),
            (args(None, 1, 1, &["1830"]), "09T18:25:00"),
            (args(None, 1, 1, &["2020-05-05T18:30"]), "04T18:25:00"),
            (args(None, 0, 1, &["-6"]), "10T15:14:30"),
        ] {
            assert_utc(&build(a), &format!("2020-05-{at}Z"));
        }
    }

    /// da-xpvdi: a day shift is a calendar day, not 24 h. Berlin's fall-back
    /// day (2026-10-25) is 25 h long and its spring-forward day (2026-03-29)
    /// 23 h, so 24 h from the hour beside either change was the same day,
    /// or two days off.
    #[test]
    fn a_day_shift_is_a_calendar_day_beside_a_dst_change() {
        let berlin = |now: &str, a: TimeArgs| {
            let now = DatumTime::from_utc_and_tz(now.parse().unwrap(), "Europe/Berlin").unwrap();
            let t = a.build(&now).unwrap().time.unwrap();
            t.local_time().naive_local().to_string()
        };
        let args = |f: fn(&mut TimeArgs)| {
            let mut a = TimeArgs::default();
            f(&mut a);
            a
        };
        // 23:30 CET on the fall-back day, by each spelling of a day back.
        for back in [
            args(|a| a.yesterday = 1),
            args(|a| a.date = Some("-1".into())),
            args(|a| a.date = Some("yesterday".into())),
            args(|a| a.time = Some("yesterday".into())),
        ] {
            assert_eq!(
                berlin("2026-10-25T22:30:00Z", back.clone()),
                "2026-10-24 23:30:00",
                "{back:?}"
            );
        }
        // 00:30 CEST on the day after spring-forward.
        assert_eq!(
            berlin("2026-03-29T22:30:00Z", args(|a| a.yesterday = 1)),
            "2026-03-29 00:30:00"
        );
        // Forward, into the long day and out of the short one.
        let tomorrow = || args(|a| a.date = Some("+1".into()));
        assert_eq!(
            berlin("2026-10-24T21:30:00Z", tomorrow()),
            "2026-10-25 23:30:00"
        );
        assert_eq!(
            berlin("2026-03-28T23:30:00Z", tomorrow()),
            "2026-03-30 00:30:00"
        );
        // A clock the day before never read (02:30 in the gap) still comes
        // out, at the change: nobody typed it, so it is no error.
        assert_eq!(
            berlin("2026-03-30T00:30:00Z", args(|a| a.yesterday = 1)),
            "2026-03-29 03:00:00"
        );
        // No shift leaves the instant alone: the second 02:30 of the
        // fall-back night is not re-read as the first.
        assert_eq!(
            berlin(
                "2026-10-25T01:30:00Z",
                args(|a| a.date = Some("today".into()))
            ),
            "2026-10-25 02:30:00"
        );
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
    fn a_zone_name_is_read_in_any_case_and_spelled_canonically() {
        for (typed, canonical) in [
            ("utc", "UTC"),
            ("europe/berlin", "Europe/Berlin"),
            ("EUROPE/BERLIN", "Europe/Berlin"),
            ("America/Chicago", "America/Chicago"),
        ] {
            assert_eq!(resolve_timezone(typed).unwrap(), canonical, "-z {typed}");
        }
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

    // ── `<date> <time>` and ISO offsets (da-6yqu9) ─────────────────────
    //
    // Read in America/Chicago, where the reference is Sunday 2020-05-10
    // 10:25:30 CDT (UTC-5), so a wall clock that were read as UTC would be
    // five hours off.

    fn time_in_chicago(t: &str) -> Result<TimeFromArgs> {
        let a = TimeArgs {
            time: Some(t.into()),
            timezone: Some("America/Chicago".into()),
            ..TimeArgs::default()
        };
        a.build(&reference_now())
    }

    #[test]
    fn a_date_then_a_time_parses_in_the_local_zone() {
        for (t, want) in [
            // What failed before, each at the instant it names.
            ("2026-09-28 10:00", "2026-09-28T15:00:00Z"),
            ("2026-09-28 10:00:00", "2026-09-28T15:00:00Z"),
            ("2026-09-28 10am", "2026-09-28T15:00:00Z"),
            ("2026-09-28T10", "2026-09-28T15:00:00Z"),
            ("sep28 10:00", "2019-09-28T15:00:00Z"),
            ("sep28 22:30", "2019-09-29T03:30:00Z"),
            ("sep28+ 22:30", "2020-09-29T03:30:00Z"),
            ("yesterday 10:00", "2020-05-09T15:00:00Z"),
            ("yesterday 10am", "2020-05-09T15:00:00Z"),
            ("sunday 9:15 am", "2020-05-10T14:15:00Z"),
            ("2026-09-28T10:00+02:00", "2026-09-28T08:00:00Z"),
            ("2026-09-28T10:00+0200", "2026-09-28T08:00:00Z"),
            ("2026-09-28 10:00+02:00", "2026-09-28T08:00:00Z"),
            ("2026-09-28 10:00-0200", "2026-09-28T12:00:00Z"),
            ("2026-09-28T10:00:30+02:00", "2026-09-28T08:00:30Z"),
            ("2026-09-28T10:00:30+0200", "2026-09-28T08:00:30Z"),
            // `Z` without seconds, which `%z` and RFC 3339 both refuse.
            ("2026-09-28T10:00Z", "2026-09-28T10:00:00Z"),
            ("2026-09-28 10:00z", "2026-09-28T10:00:00Z"),
            // What parsed before, unchanged.
            ("2026-09-28T10:00", "2026-09-28T15:00:00Z"),
            ("2026-09-28T10:00:00", "2026-09-28T15:00:00Z"),
            ("2026-09-28T10:00:00Z", "2026-09-28T10:00:00Z"),
            ("2026-09-28", "2026-09-28T15:25:30Z"),
            ("sep28", "2019-09-28T15:25:30Z"),
            ("yesterday", "2020-05-09T15:25:30Z"),
            ("10:00", "2020-05-10T15:00:00Z"),
            ("10am", "2020-05-10T15:00:00Z"),
            ("-3d", "2020-05-07T15:25:30Z"),
        ] {
            let got = time_in_chicago(t).unwrap_or_else(|e| panic!("{t:?}: {e}"));
            let want: DateTime<Utc> = want.parse().unwrap();
            assert_eq!(got.time.unwrap().utc, want, "-t {t:?}");
        }
    }

    #[test]
    fn a_date_then_a_time_keeps_a_trailing_uncertainty() {
        let got = time_in_chicago("yesterday 10:00+-5m").unwrap();
        assert_eq!(got.uncertainty.unwrap().seconds, 300);
    }

    #[test]
    fn a_failure_points_at_the_time_help_page() {
        // `09-28` is no date `-t` takes (it has no month-day form without
        // a year), so its join does not parse either.
        for t in [
            "sep31 10:00",
            "2026-09-28 25:00",
            "09-28 10:00",
            "10:00 sep28",
        ] {
            let err = time_in_chicago(t)
                .err()
                .unwrap_or_else(|| panic!("{t:?} parsed"));
            assert_eq!(
                err.to_string(),
                format!("Invalid time: cannot parse time '{t}' — see `datum help time`"),
            );
        }
    }
}
