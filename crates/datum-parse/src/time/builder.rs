//! `TimeBuilder` — compose stacked flags into a final `DatumTime`.

use chrono::{DateTime, Duration, NaiveDateTime, NaiveTime, Utc};

use crate::time::date::DateArg;
#[cfg(feature = "native")]
use crate::time::datum_time::DatumTime;
use crate::time::dur::quick_uncertainty_for_count;
#[cfg(feature = "native")]
use crate::time::resolve::Zone;
use crate::time::time::TimeArg;
#[cfg(feature = "native")]
use crate::time::uncertainty::UncertaintyUnit;
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
    /// Implicit uncertainty produced by `-q*` stacks (None when no `-q`
    /// given). Overridden by any explicit `±N` attached to a `-d`/`-t` arg.
    pub quick_uncertainty: Option<Uncertainty>,
    /// Explicit `±N` supplied via `-d` or `-t`. Beats `quick_uncertainty`.
    pub explicit_uncertainty: Option<Uncertainty>,
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
    /// `build_with_uncertainty` strips the time-of-day to 00:00:00 in the
    /// effective zone and emits a `P1D` uncertainty.
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
            quick_uncertainty: None,
            explicit_uncertainty: None,
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
        // `-q` scales the time offset (5 min per flag). Uncertainty is set
        // separately by `with_quick_uncertainty` so callers that only want the
        // offset can opt out.
        self.minutes_back += n * 5;
        self
    }

    /// Apply the `-q*` scaling rule (see `quick_uncertainty_for_count`).
    /// A zero count leaves the uncertainty unchanged.
    pub fn with_quick_uncertainty(mut self, n: u32) -> Self {
        if n == 0 {
            return self;
        }
        self.quick_uncertainty = Some(quick_uncertainty_for_count(n));
        self
    }

    pub fn with_date_arg(mut self, arg: DateArg) -> Self {
        if let Some(abs) = arg.absolute {
            self.date_override = Some(abs);
        } else {
            self.days_back += -arg.days_offset; // -1 means yesterday → days_back += 1
        }
        if let Some(u) = arg.uncertainty {
            self.explicit_uncertainty = Some(u);
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
            self.explicit_uncertainty = Some(u);
        }
        self
    }

    /// Collapse the entry to a date-only event. Mirrors `-D/--full-day` on
    /// the CLI. Stays inert until `build_with_uncertainty` reads it.
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

    /// Effective uncertainty — explicit `±` beats the implicit `-q` value.
    /// Returns `Uncertainty::exact()` when neither is set.
    pub fn uncertainty(&self) -> Uncertainty {
        self.explicit_uncertainty
            .clone()
            .or_else(|| self.quick_uncertainty.clone())
            .unwrap_or_else(Uncertainty::exact)
    }

    /// Build the `DatumTime` with its effective uncertainty already on it.
    /// Most callers route through this rather than [`build`] so the
    /// uncertainty travels with the time it belongs to (da-b8rs) instead of
    /// riding alongside in a second variable.
    ///
    /// When the input describes a date-only event (`full_day` set, or a
    /// date/`-y` with no time/quick input), the uncertainty is `P1D` and the
    /// time-of-day is stripped to 00:00:00 in the effective zone — mirroring
    /// datum's `toDatumTime(_, true)`.
    ///
    /// [`build`]: Self::build
    #[cfg(feature = "native")]
    pub fn build_with_uncertainty(self) -> DatumTime {
        let only_date = self.is_only_date();
        let base = self.uncertainty();
        let uncertainty = if only_date {
            Uncertainty {
                seconds: 86_400,
                unit: UncertaintyUnit::Date,
            }
        } else {
            base
        };
        let dt = self.build();
        let mut dt = if only_date {
            strip_to_start_of_day(dt)
        } else {
            dt
        };
        dt.uncertainty = uncertainty.to_iso();
        dt
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
            || self.quick_uncertainty.is_some();
        self.full_day || (date_or_yesterday && !time_or_quick)
    }

    #[cfg(feature = "native")]
    pub fn build(self) -> DatumTime {
        // Both paths end at one `Zone`: `-z` supplies a name, and the host
        // zone is the name `DatumTime` records anyway. Resolving through a
        // zone instead of reaching for `chrono::Local` collapses what were
        // two generic instantiations into one body, so there is no second
        // place for a DST policy to drift (da-ztae).
        let (zone, name) = match self.timezone.as_deref() {
            // Panic-safe: resolve_timezone() already validated the name
            // before it got here. A bad name at this point means the caller
            // skipped validation — surface it loudly rather than silently
            // falling back to the host zone.
            Some(tz_name) => (
                Zone::iana(tz_name)
                    .expect("TimeBuilder: timezone field must be a validated IANA name"),
                Some(tz_name.to_string()),
            ),
            None => match crate::time::clock::host_iana_tz_name() {
                Some(name) => (
                    Zone::iana(&name).expect("host zone name must be a valid IANA name"),
                    Some(name),
                ),
                // A host that cannot name its zone still reports the right
                // offset, and an offset with no name is precisely what
                // `DatumTime::timezone` documents `None` for. Requiring a
                // name here is what made an unnameable host record UTC —
                // wrong instant, not merely a wrong label (da-2pgd).
                None => (Zone::Fixed(crate::time::clock::host_offset()), None),
            },
        };
        self.build_in_zone(zone, name)
    }

    /// Compose the stacked flags into a `DatumTime` anchored in `zone`.
    ///
    /// Every local→UTC step goes through [`Zone::resolve`] under the
    /// [`Resolved::for_boundary`] policy: these are composed *shifts* of a
    /// moment and one must always come out. The `.single()` guards this
    /// replaces silently discarded the entire `-d`/`-t` override whenever
    /// the resulting wall clock landed on a DST seam, which is the failure
    /// mode da-ztae exists to remove.
    #[cfg(feature = "native")]
    fn build_in_zone(self, zone: Zone, timezone_name: Option<String>) -> DatumTime {
        // ISO-8601 overrides establish the base instant; otherwise we anchor
        // on "now" in the effective zone.
        let mut utc = if let Some(instant) = self.iso_instant_override {
            instant
        } else if let Some(naive) = self.iso_naive_override {
            zone.resolve(naive).for_boundary()
        } else {
            Utc::now()
        };

        utc -= chrono::Duration::minutes(self.minutes_back);
        utc -= chrono::Duration::days(self.days_back);
        utc += self.time_offset; // signed `-t ±N` shift (future positive)

        if let Some(date) = self.date_override {
            let time = self
                .time_override
                .unwrap_or_else(|| zone.local_naive(utc).time());
            utc = zone.resolve(date.and_time(time)).for_boundary();
        } else if let Some(time) = self.time_override {
            let now_naive = zone.local_naive(utc);
            let mut local_dt = now_naive.date().and_time(time);
            if let Some(dir) = self.time_direction {
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
            utc = zone.resolve(local_dt).for_boundary();
        }

        DatumTime {
            utc,
            offset_minutes: zone.offset_at(utc).local_minus_utc() / 60,
            timezone: timezone_name,
            uncertainty: None,
        }
    }
}

/// Rewind a built `DatumTime` to 00:00:00 of the same calendar day in its
/// own zone. Preserves the IANA name and refreshes `offset_minutes`, because
/// midnight may sit on the far side of a DST transition from the original
/// time.
///
/// Midnight is a *day boundary*, so [`Resolved::for_boundary`] is exactly
/// the policy wanted: a fall-back overlap takes the earlier instant and a
/// spring-forward gap clamps to the transition, which is what makes
/// consecutive days abut with no hole and no double-count. Zones that shift
/// across midnight exist (Chile, Cuba, Lord Howe), so the old `panic!` on
/// that branch was reachable.
#[cfg(feature = "native")]
fn strip_to_start_of_day(dt: DatumTime) -> DatumTime {
    // A `DatumTime` with no zone name still has its stored offset, which has
    // no transitions — `Zone::of` yields that, so midnight resolves either way.
    let zone = Zone::of(&dt)
        .expect("strip_to_start_of_day: DatumTime.timezone must be a validated IANA name");
    let midnight = zone
        .local_naive(dt.utc)
        .date()
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is always a valid time");
    let utc = zone.resolve(midnight).for_boundary();
    DatumTime {
        utc,
        offset_minutes: zone.offset_at(utc).local_minus_utc() / 60,
        timezone: dt.timezone,
        uncertainty: dt.uncertainty,
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;
    use crate::time::date::DateArg;
    use crate::time::time::TimeArg;
    use chrono::{FixedOffset, NaiveDate, NaiveTime};

    /// The ISO form a date-only entry lands on `DatumTime::uncertainty`.
    fn p1d() -> Option<String> {
        Some("P1D".to_string())
    }

    /// da-2pgd: a host whose zone cannot be *named* still has an offset, and
    /// that offset must survive into the row. Before this, the nameless case
    /// resolved through `Zone::iana("UTC")`, so the stored instant moved by
    /// the host's whole distance from Greenwich and the row then claimed UTC
    /// as though it had been told.
    #[test]
    fn a_nameless_zone_keeps_its_offset_and_claims_no_name() {
        let zone = Zone::Fixed(FixedOffset::east_opt(2 * 3600).unwrap());
        let dt = TimeBuilder::now().build_in_zone(zone, None);
        assert_eq!(dt.timezone, None);
        assert_eq!(dt.offset_minutes, 120);
    }

    /// `with_full_day(true)` collapses the base instant to midnight in the
    /// configured zone and reports `P1D` uncertainty. No anchor is set so the
    /// builder falls through to "now" — the assertions only depend on the
    /// stripping, not on the wall clock at test time.
    #[test]
    fn full_day_strips_time_and_emits_p1d() {
        let b = TimeBuilder::now()
            .with_timezone(Some("America/Chicago".into()))
            .with_full_day(true);

        let dt = b.build_with_uncertainty();
        assert_eq!(dt.uncertainty, p1d());
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
        let dt = b.build_with_uncertainty();
        assert_eq!(dt.uncertainty, p1d());
        let local = dt.utc.with_timezone(&chrono_tz::America::Chicago);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        assert_eq!(
            local.date_naive(),
            NaiveDate::from_ymd_opt(2024, 3, 16).unwrap()
        );
    }

    /// `-y` (yesterday) alone is also a date-only event. Uses an implicit
    /// "now" anchor so we assert only the stripping + uncertainty, not the
    /// resulting calendar date.
    #[test]
    fn yesterday_alone_strips_time_and_emits_p1d() {
        let b = TimeBuilder::now()
            .with_timezone(Some("America/Chicago".into()))
            .with_yesterday_count(1);

        let dt = b.build_with_uncertainty();
        assert_eq!(dt.uncertainty, p1d());
        let local = dt.utc.with_timezone(&chrono_tz::America::Chicago);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    }

    /// `--full-day` overrides any explicit clock input — the time is still
    /// stripped and uncertainty is still `P1D`.
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

        let dt = b.build_with_uncertainty();
        assert_eq!(dt.uncertainty, p1d());
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
    /// must be preserved and the uncertainty falls back to exact.
    #[test]
    fn date_plus_time_keeps_clock_and_exact_uncertainty() {
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
        let dt = b.build_with_uncertainty();
        // Exact is rendered as absent: `None` on the field, NULL in the
        // column (see `Uncertainty::to_iso`).
        assert_eq!(dt.uncertainty, None);
        let local = dt.utc.with_timezone(&chrono_tz::America::Chicago);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(14, 37, 0).unwrap());
    }

    /// The bug `.single()` used to cause: when the requested wall clock
    /// lands in a spring-forward gap, the *entire* `-d`/`-t` override was
    /// silently discarded and the entry kept "now". Under the boundary
    /// policy it clamps to the transition instead — the user's date is
    /// honoured, and it is a movement of minutes, not of months (da-ztae).
    #[test]
    fn date_and_time_landing_in_a_dst_gap_is_no_longer_discarded() {
        // 2024-03-10 02:30 does not exist in Chicago; the clock jumps
        // 01:59:59 CST → 03:00:00 CDT.
        let b = TimeBuilder::now()
            .with_timezone(Some("America/Chicago".into()))
            .with_date_arg(DateArg {
                days_offset: 0,
                absolute: Some(NaiveDate::from_ymd_opt(2024, 3, 10).unwrap()),
                uncertainty: None,
                direction: None,
            })
            .with_time_arg(TimeArg {
                date: None,
                absolute: Some(NaiveTime::from_hms_opt(2, 30, 0).unwrap()),
                iso_instant: None,
                iso_naive: None,
                offset: None,
                uncertainty: None,
                direction: None,
            });
        let dt = b.build();
        assert_eq!(
            dt.utc,
            "2024-03-10T08:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            "clamped to the transition, not left at `now`"
        );
    }

    /// The other half: a wall clock the fall-back overlap repeats resolves
    /// to the earlier of the two instants rather than being dropped.
    #[test]
    fn date_and_time_in_a_dst_overlap_takes_the_earlier_instant() {
        let b = TimeBuilder::now()
            .with_timezone(Some("America/Chicago".into()))
            .with_date_arg(DateArg {
                days_offset: 0,
                absolute: Some(NaiveDate::from_ymd_opt(2024, 11, 3).unwrap()),
                uncertainty: None,
                direction: None,
            })
            .with_time_arg(TimeArg {
                date: None,
                absolute: Some(NaiveTime::from_hms_opt(1, 30, 0).unwrap()),
                iso_instant: None,
                iso_naive: None,
                offset: None,
                uncertainty: None,
                direction: None,
            });
        assert_eq!(
            b.build().utc,
            "2024-11-03T06:30:00Z".parse::<DateTime<Utc>>().unwrap()
        );
    }

    /// `strip_to_start_of_day` used to `panic!` when midnight itself was in
    /// the gap. Santiago springs forward *at* midnight, so that branch was
    /// reachable by anyone logging `-D` from Chile on the day.
    #[test]
    fn start_of_day_survives_a_zone_whose_midnight_does_not_exist() {
        // 2024-09-08 00:00 does not exist in Santiago: 23:59:59 -04 on the
        // 7th is followed by 01:00:00 -03 on the 8th.
        let noon = DatumTime::from_utc_and_tz(
            "2024-09-08T16:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            "America/Santiago",
        )
        .unwrap();
        let start = strip_to_start_of_day(noon);
        assert_eq!(
            start.utc,
            "2024-09-08T04:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            "clamped to the transition — the first instant of the local day"
        );
        assert_eq!(start.offset_minutes, -180, "post-transition offset (-03)");
    }
}
