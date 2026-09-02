//! `TimeBuilder` — compose stacked flags into a final `DatumTime`.

use chrono::{DateTime, Duration, NaiveDateTime, NaiveTime, Utc};

#[cfg(feature = "native")]
use crate::time::datum_time::DatumTime;
use crate::time::parse_date_str::DateArg;
use crate::time::parse_duration_str::quick_precision_for_count;
use crate::time::parse_time_str::TimeArg;
#[cfg(feature = "native")]
use crate::time::precision::PrecisionUnit;
use crate::time::uncertainty::{Direction, Uncertainty};

/// Build a `DatumTime` from the combination of -y stacks, -q stacks,
/// -d arg, -t arg, and an optional base time (defaults to now).
pub struct TimeBuilder {
    pub days_back: i64,
    pub minutes_back: i64,
    /// Signed relative shift from `-t ±N` (positive = future). Held as a
    /// `Duration` rather than folded into `minutes_back` so decimal and
    /// unit offsets (`-3.5`, `+10m`, `-1.5h`) keep second precision.
    pub time_offset: Duration,
    pub date_override: Option<chrono::NaiveDate>,
    pub time_override: Option<NaiveTime>,
    /// ISO-8601 datetime with explicit zone (`...Z` / `...+02:00`) — a
    /// fully-resolved instant. Replaces "now" as the build base. The
    /// entry's `timezone` field still comes from `self.timezone` (i.e.
    /// `-z`), so the literal sets the moment, not the display zone.
    pub iso_instant_override: Option<DateTime<Utc>>,
    /// ISO-8601 datetime without zone suffix (`2024-03-16T15:30:00`).
    /// Interpreted as local-in-the-effective-zone (`self.timezone` or
    /// host local) when [`build`] resolves it.
    pub iso_naive_override: Option<NaiveDateTime>,
    /// Implicit precision produced by `-q*` stacks (None when no `-q` given).
    /// Overridden by any explicit `±N` attached to a `-d`/`-t` arg.
    pub quick_precision: Option<Uncertainty>,
    /// Explicit `±N` supplied via `-d` or `-t`. Beats `quick_precision`.
    pub explicit_precision: Option<Uncertainty>,
    /// Pre-validated IANA timezone name from `-z/--timezone`. When `Some`,
    /// `build()` anchors all date/time math in this zone instead of `Local`.
    /// Callers must resolve numeric/UTC±N forms via
    /// [`crate::time::resolve_timezone`] before passing the string in.
    pub timezone: Option<String>,
    /// Direction hint for the time portion (trailing `+`/`-` on `-t`).
    /// Only consumed when `time_override` is the resolution path —
    /// pinned ISO inputs and relative `±N` offsets ignore it.
    pub time_direction: Option<Direction>,
    /// `-D/--full-day`: collapse the entry to a date-only event. When true
    /// (or when a date is supplied without any time/quick input),
    /// `build_with_precision` strips the time-of-day to 00:00:00 in the
    /// effective zone and emits a `P1D` precision.
    pub full_day: bool,
}

#[cfg(feature = "native")]
impl TimeBuilder {
    /// Construct a builder anchored at "now" in the local timezone. Requires
    /// the `native` feature; callers targeting WASM should build via
    /// [`TimeBuilder::at`] with an explicit `DatumTime`.
    pub fn now() -> Self {
        TimeBuilder {
            days_back: 0,
            minutes_back: 0,
            time_offset: Duration::zero(),
            date_override: None,
            time_override: None,
            iso_instant_override: None,
            iso_naive_override: None,
            quick_precision: None,
            explicit_precision: None,
            timezone: None,
            time_direction: None,
            full_day: false,
        }
    }
}

impl TimeBuilder {
    pub fn with_yesterday_count(mut self, n: i64) -> Self {
        self.days_back += n;
        self
    }

    pub fn with_quick_count(mut self, n: i64) -> Self {
        // `-q` scales the time offset (5 min per flag). Precision is set
        // separately by `with_quick_precision` so callers that only want the
        // offset can opt out.
        self.minutes_back += n * 5;
        self
    }

    /// Apply the `-q*` precision scaling rule (see `quick_precision_for_count`).
    /// A zero count leaves precision unchanged.
    pub fn with_quick_precision(mut self, n: u32) -> Self {
        if n == 0 {
            return self;
        }
        self.quick_precision = Some(quick_precision_for_count(n));
        self
    }

    pub fn with_date_arg(mut self, arg: DateArg) -> Self {
        if let Some(abs) = arg.absolute {
            self.date_override = Some(abs);
        } else {
            self.days_back += -arg.days_offset; // -1 means yesterday → days_back += 1
        }
        if let Some(u) = arg.uncertainty {
            self.explicit_precision = Some(u);
        }
        self
    }

    pub fn with_time_arg(mut self, arg: TimeArg) -> Self {
        // `-t sunday` / `-t 2024-03-16`: the value named a day, not a
        // clock, so it takes the date path and lands as a date-only event.
        if let Some(date) = arg.date {
            return self.with_date_arg(date);
        }
        if let Some(instant) = arg.iso_instant {
            self.iso_instant_override = Some(instant);
        } else if let Some(naive) = arg.iso_naive {
            self.iso_naive_override = Some(naive);
        } else if let Some(abs) = arg.absolute {
            self.time_override = Some(abs);
            // Direction is only meaningful when we resolve a bare clock
            // against "today" — that's the time_override path.
            self.time_direction = arg.direction;
        } else if let Some(offset) = arg.offset {
            // Signed relative shift (`+5` future, `-8` past, `-3.5`/`-10m`
            // sub-minute). Stored as a Duration so decimal/unit offsets keep
            // their second-precision instead of truncating to whole minutes.
            self.time_offset += offset;
        }
        if let Some(u) = arg.uncertainty {
            self.explicit_precision = Some(u);
        }
        self
    }

    /// Collapse the entry to a date-only event. Mirrors `-D/--full-day` on
    /// the CLI. Stays inert until `build_with_precision` reads it.
    pub fn with_full_day(mut self, full_day: bool) -> Self {
        self.full_day = full_day;
        self
    }

    /// Anchor all date/time math in a specific IANA timezone. The string
    /// must already be a valid IANA name (`America/New_York`, `Etc/GMT-5`,
    /// `UTC`, …); resolve numeric/`UTC±N` forms via
    /// [`crate::time::resolve_timezone`] at the call site first.
    /// Passing `None` keeps the default `Local` behavior.
    pub fn with_timezone(mut self, tz: Option<String>) -> Self {
        self.timezone = tz;
        self
    }

    /// Effective precision — explicit `±` beats implicit `-q` precision.
    /// Returns `Uncertainty::exact()` when neither is set.
    pub fn precision(&self) -> Uncertainty {
        self.explicit_precision
            .clone()
            .or_else(|| self.quick_precision.clone())
            .unwrap_or_else(Uncertainty::exact)
    }

    /// Build the DatumTime together with the effective precision. Most
    /// callers route through this so precision fields land on the entry.
    ///
    /// When the input describes a date-only event (`full_day` set, or a
    /// date/`-y` with no time/quick input), the returned precision is
    /// `P1D` and the time-of-day is stripped to 00:00:00 in the effective
    /// zone — mirroring datum's `toDatumTime(_, true)`.
    #[cfg(feature = "native")]
    pub fn build_with_precision(self) -> (DatumTime, Uncertainty) {
        let only_date = self.is_only_date();
        let base_precision = self.precision();
        let precision = if only_date {
            Uncertainty {
                seconds: 86_400,
                unit: PrecisionUnit::Date,
            }
        } else {
            base_precision
        };
        let dt = self.build();
        let dt = if only_date {
            strip_to_start_of_day(dt)
        } else {
            dt
        };
        (dt, precision)
    }

    /// True when the inputs describe a date-only event: explicit `full_day`,
    /// or a date/`-y` shift with no time-of-day input. Mirrors the
    /// `only_date` flag computed in `TimeArgs::apply_to`.
    fn is_only_date(&self) -> bool {
        let date_or_yesterday = self.date_override.is_some() || self.days_back != 0;
        let time_or_quick = self.time_override.is_some()
            || self.iso_instant_override.is_some()
            || self.iso_naive_override.is_some()
            || self.minutes_back != 0
            || self.time_offset != Duration::zero()
            || self.quick_precision.is_some();
        self.full_day || (date_or_yesterday && !time_or_quick)
    }

    #[cfg(feature = "native")]
    pub fn build(self) -> DatumTime {
        if let Some(tz_name) = self.timezone.as_deref() {
            // Panic-safe: resolve_timezone() already validated the name
            // before it got here. A bad name at this point means the caller
            // skipped validation — surface it loudly rather than silently
            // falling back to Local.
            let tz: chrono_tz::Tz = tz_name
                .parse()
                .expect("TimeBuilder: timezone field must be a validated IANA name");
            let name = tz_name.to_string();
            self.build_in_tz(tz, name)
        } else {
            let name = crate::time::clock::host_iana_tz_name();
            self.build_in_tz(chrono::Local, name)
        }
    }

    /// Compose the stacked flags into a `DatumTime` anchored in an arbitrary
    /// [`chrono::TimeZone`]. Both the explicit-`-z` (`chrono_tz::Tz`) and the
    /// default-host (`chrono::Local`) paths run through this one body, so the
    /// six resolution steps — base, minute/day offsets, date override, time
    /// override with direction, and the final UTC/offset computation — stay
    /// in lockstep. In particular, DST-overlap and DST-gap handling is
    /// identical regardless of whether `--timezone` was supplied.
    #[cfg(feature = "native")]
    fn build_in_tz<T: chrono::TimeZone>(self, tz: T, timezone_name: String) -> DatumTime {
        use chrono::Offset;

        // ISO-8601 overrides establish the base instant; otherwise we anchor
        // on "now" in the effective zone.
        let mut dt = if let Some(instant) = self.iso_instant_override {
            instant.with_timezone(&tz)
        } else if let Some(naive) = self.iso_naive_override {
            resolve_naive_in_tz(naive, tz.clone(), &timezone_name)
        } else {
            Utc::now().with_timezone(&tz)
        };

        dt -= chrono::Duration::minutes(self.minutes_back);
        dt -= chrono::Duration::days(self.days_back);
        dt += self.time_offset; // signed `-t ±N` shift (future positive)

        if let Some(date) = self.date_override {
            let time = self.time_override.unwrap_or_else(|| dt.time());
            if let Some(new_dt) = date.and_time(time).and_local_timezone(tz.clone()).single() {
                dt = new_dt;
            }
        } else if let Some(time) = self.time_override {
            let mut local_dt = dt.date_naive().and_time(time);
            if let Some(dir) = self.time_direction {
                let now_naive = dt.naive_local();
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
            if let Some(new_dt) = local_dt.and_local_timezone(tz.clone()).single() {
                dt = new_dt;
            }
        }

        let utc = dt.with_timezone(&Utc);
        let offset_minutes = dt.offset().fix().local_minus_utc() / 60;
        DatumTime {
            utc,
            offset_minutes,
            timezone: Some(timezone_name),
        }
    }
}

/// Rewind a built `DatumTime` to 00:00:00 of the same calendar day in its
/// own zone. Preserves the IANA name and refreshes `offset_minutes` because
/// midnight may sit on the far side of a DST transition from the original
/// time. Ambiguous midnights (DST fall-back) pick the earlier instant; the
/// spring-forward gap at midnight is essentially nonexistent in real zones
/// but is panicked on for surface area parity with `resolve_naive_in_tz`.
#[cfg(feature = "native")]
fn strip_to_start_of_day(dt: DatumTime) -> DatumTime {
    match dt.timezone.as_deref() {
        Some(name) => {
            let tz: chrono_tz::Tz = name
                .parse()
                .expect("strip_to_start_of_day: DatumTime.timezone must be a validated IANA name");
            strip_in_zone(dt, tz)
        }
        // No stored zone: the fixed offset is all we know about local time,
        // and it has no DST transition for midnight to land on the far side
        // of, so the day boundary is unambiguous.
        None => {
            let offset = *dt.local_time().offset();
            strip_in_zone(dt, offset)
        }
    }
}

#[cfg(feature = "native")]
fn strip_in_zone<Z>(dt: DatumTime, tz: Z) -> DatumTime
where
    Z: chrono::TimeZone + std::fmt::Display,
{
    use chrono::offset::LocalResult;
    use chrono::Offset;
    let local = dt.utc.with_timezone(&tz);
    let midnight = local
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is always a valid time");
    let local_midnight = match midnight.and_local_timezone(tz.clone()) {
        LocalResult::Single(t) => t,
        // Earlier instant, selected by UTC like `resolve_naive_in_tz` rather
        // than by tuple position (see its note on impl-dependent ordering).
        LocalResult::Ambiguous(a, b) => a.min(b),
        LocalResult::None => {
            panic!("strip_to_start_of_day: midnight {midnight} does not exist in {tz} (DST gap)")
        }
    };
    DatumTime {
        utc: local_midnight.with_timezone(&Utc),
        offset_minutes: local_midnight.offset().fix().local_minus_utc() / 60,
        timezone: dt.timezone,
    }
}

/// Resolve a no-zone ISO-8601 datetime into a concrete moment by reading
/// it as wall-clock in the supplied zone. Picks the earliest valid instant
/// when the local time falls in a DST overlap, which matches what users
/// expect from "the first time the clock read 01:30" on fall-back days.
/// Spring-forward gaps fall through to a panic with a clear message — they're
/// rare, and silently shifting the user's input by an hour is worse than
/// failing loudly. Generic over the zone so the explicit-`-z` and host-local
/// paths share one DST contract; `zone_name` only feeds the gap panic since
/// [`chrono::Local`] has no useful `Display`.
///
/// The two `Ambiguous` instants are selected by UTC instant, not by tuple
/// position: `chrono_tz::Tz` and `chrono::Local` order the pair *oppositely*
/// (the former yields the earlier instant first, the latter the later), so
/// keying on position would silently pick the wrong side on the host-local
/// path. `DateTime` orders by instant, so `min` is the genuinely-earlier one.
#[cfg(feature = "native")]
fn resolve_naive_in_tz<T: chrono::TimeZone>(
    naive: NaiveDateTime,
    tz: T,
    zone_name: &str,
) -> chrono::DateTime<T> {
    use chrono::offset::LocalResult;
    match naive.and_local_timezone(tz) {
        LocalResult::Single(dt) => dt,
        LocalResult::Ambiguous(a, b) => a.min(b),
        LocalResult::None => {
            panic!(
                "TimeBuilder: ISO-8601 wall clock {naive} does not exist in {zone_name} (DST gap)"
            )
        }
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;
    use crate::time::parse_date_str::DateArg;
    use crate::time::parse_time_str::TimeArg;
    use chrono::{NaiveDate, NaiveTime};

    fn p1d() -> Uncertainty {
        Uncertainty {
            seconds: 86_400,
            unit: PrecisionUnit::Date,
        }
    }

    /// `with_full_day(true)` collapses the base instant to midnight in the
    /// configured zone and reports `P1D` precision. No anchor is set so the
    /// builder falls through to "now" — the assertions only depend on the
    /// stripping, not on the wall clock at test time.
    #[test]
    fn full_day_strips_time_and_emits_p1d() {
        let b = TimeBuilder::now()
            .with_timezone(Some("America/Chicago".into()))
            .with_full_day(true);

        let (dt, precision) = b.build_with_precision();
        assert_eq!(precision, p1d());
        let local = dt.utc.with_timezone(&chrono_tz::America::Chicago);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        assert_eq!(dt.timezone.as_deref(), Some("America/Chicago"));
    }

    /// An absolute `-d` with no `-t`/`-q` is a date-only event.
    #[test]
    fn date_alone_strips_time_and_emits_p1d() {
        let b = TimeBuilder::now()
            .with_timezone(Some("America/Chicago".into()))
            .with_date_arg(DateArg {
                days_offset: 0,
                absolute: Some(NaiveDate::from_ymd_opt(2024, 3, 16).unwrap()),
                uncertainty: None,
                direction: None,
            });
        let (dt, precision) = b.build_with_precision();
        assert_eq!(precision, p1d());
        let local = dt.utc.with_timezone(&chrono_tz::America::Chicago);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        assert_eq!(
            local.date_naive(),
            NaiveDate::from_ymd_opt(2024, 3, 16).unwrap()
        );
    }

    /// `-y` (yesterday) alone is also a date-only event. Uses an implicit
    /// "now" anchor so we assert only the stripping + precision, not the
    /// resulting calendar date.
    #[test]
    fn yesterday_alone_strips_time_and_emits_p1d() {
        let b = TimeBuilder::now()
            .with_timezone(Some("America/Chicago".into()))
            .with_yesterday_count(1);

        let (dt, precision) = b.build_with_precision();
        assert_eq!(precision, p1d());
        let local = dt.utc.with_timezone(&chrono_tz::America::Chicago);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    }

    /// `--full-day` overrides any explicit clock input — the time is still
    /// stripped and precision is still `P1D`.
    #[test]
    fn full_day_wins_over_clock() {
        let b = TimeBuilder::now()
            .with_timezone(Some("America/Chicago".into()))
            .with_date_arg(DateArg {
                days_offset: 0,
                absolute: Some(NaiveDate::from_ymd_opt(2024, 3, 16).unwrap()),
                uncertainty: None,
                direction: None,
            })
            .with_time_arg(TimeArg {
                absolute: Some(NaiveTime::from_hms_opt(5, 0, 0).unwrap()),
                iso_instant: None,
                iso_naive: None,
                date: None,
                offset: None,
                uncertainty: None,
                direction: None,
            })
            .with_full_day(true);

        let (dt, precision) = b.build_with_precision();
        assert_eq!(precision, p1d());
        let local = dt.utc.with_timezone(&chrono_tz::America::Chicago);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        assert_eq!(
            local.date_naive(),
            NaiveDate::from_ymd_opt(2024, 3, 16).unwrap()
        );
    }

    /// A fall-back-hour ISO-naive wall clock (`01:30` exists twice on
    /// 2024-11-03 in America/New_York) resolves to the *earlier* instant
    /// (EDT, −04:00 → 05:30 UTC) rather than panicking. This is the DST
    /// contract that `build_in_tz` now shares between the explicit-`-z` and
    /// host-local paths.
    #[test]
    fn iso_naive_dst_overlap_picks_earlier_instant() {
        let b = TimeBuilder::now()
            .with_timezone(Some("America/New_York".into()))
            .with_time_arg(TimeArg {
                absolute: None,
                iso_instant: None,
                iso_naive: Some(
                    NaiveDate::from_ymd_opt(2024, 11, 3)
                        .unwrap()
                        .and_hms_opt(1, 30, 0)
                        .unwrap(),
                ),
                date: None,
                offset: None,
                uncertainty: None,
                direction: None,
            });
        let dt = b.build();
        // Earlier of the two 01:30s is EDT (−04:00) → 05:30 UTC.
        assert_eq!(dt.offset_minutes, -240);
        assert_eq!(
            dt.utc,
            NaiveDate::from_ymd_opt(2024, 11, 3)
                .unwrap()
                .and_hms_opt(5, 30, 0)
                .unwrap()
                .and_utc()
        );
        assert_eq!(dt.timezone.as_deref(), Some("America/New_York"));
    }

    /// Same fall-back hour, but with no `-z` so the build runs the
    /// host-local (`chrono::Local`) arm. This is the regression that the
    /// explicit-`-z` test above cannot catch: `chrono::Local` orders the
    /// `LocalResult::Ambiguous` pair *opposite* to `chrono_tz::Tz`, so the
    /// resolver must pick by UTC instant, not tuple position. The earlier
    /// instant is still EDT (−04:00 → 05:30 UTC). Before the unification this
    /// path panicked outright ("Ambiguous/non-existent local datetime").
    #[test]
    fn iso_naive_dst_overlap_local_arm_picks_earlier_instant() {
        let _tz = crate::time::clock::test_tz::TzGuard::set(Some("America/New_York"));
        let b = TimeBuilder::now().with_time_arg(TimeArg {
            absolute: None,
            iso_instant: None,
            iso_naive: Some(
                NaiveDate::from_ymd_opt(2024, 11, 3)
                    .unwrap()
                    .and_hms_opt(1, 30, 0)
                    .unwrap(),
            ),
            date: None,
            offset: None,
            uncertainty: None,
            direction: None,
        });
        let dt = b.build();
        assert_eq!(dt.offset_minutes, -240);
        assert_eq!(
            dt.utc,
            NaiveDate::from_ymd_opt(2024, 11, 3)
                .unwrap()
                .and_hms_opt(5, 30, 0)
                .unwrap()
                .and_utc()
        );
        assert_eq!(dt.timezone.as_deref(), Some("America/New_York"));
    }

    /// `-d` paired with `-t` is NOT a date-only event — the explicit time
    /// must be preserved and the precision falls back to exact.
    #[test]
    fn date_plus_time_keeps_clock_and_exact_precision() {
        let b = TimeBuilder::now()
            .with_timezone(Some("America/Chicago".into()))
            .with_date_arg(DateArg {
                days_offset: 0,
                absolute: Some(NaiveDate::from_ymd_opt(2024, 3, 16).unwrap()),
                uncertainty: None,
                direction: None,
            })
            .with_time_arg(TimeArg {
                absolute: Some(NaiveTime::from_hms_opt(14, 37, 0).unwrap()),
                iso_instant: None,
                iso_naive: None,
                date: None,
                offset: None,
                uncertainty: None,
                direction: None,
            });
        let (dt, precision) = b.build_with_precision();
        assert_eq!(precision, Uncertainty::exact());
        let local = dt.utc.with_timezone(&chrono_tz::America::Chicago);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(14, 37, 0).unwrap());
    }
}
