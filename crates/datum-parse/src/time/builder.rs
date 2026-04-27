//! `TimeBuilder` — compose stacked flags into a final `DatumTime`.

use chrono::{DateTime, NaiveDateTime, NaiveTime, Utc};

#[cfg(feature = "native")]
use crate::time::datum_time::DatumTime;
use crate::time::parse_date_str::DateArg;
use crate::time::parse_duration_str::quick_precision_for_count;
use crate::time::parse_time_str::TimeArg;
use crate::time::uncertainty::Uncertainty;

/// Build a `DatumTime` from the combination of -y stacks, -q stacks,
/// -d arg, -t arg, and an optional base time (defaults to now).
pub struct TimeBuilder {
    pub days_back: i64,
    pub minutes_back: i64,
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
            date_override: None,
            time_override: None,
            iso_instant_override: None,
            iso_naive_override: None,
            quick_precision: None,
            explicit_precision: None,
            timezone: None,
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
        if let Some(instant) = arg.iso_instant {
            self.iso_instant_override = Some(instant);
        } else if let Some(naive) = arg.iso_naive {
            self.iso_naive_override = Some(naive);
        } else if let Some(abs) = arg.absolute {
            self.time_override = Some(abs);
        } else if let Some(offset) = arg.minutes_offset {
            self.minutes_back += -offset; // -8 means 8 min ago → minutes_back += 8
        }
        if let Some(u) = arg.uncertainty {
            self.explicit_precision = Some(u);
        }
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
    #[cfg(feature = "native")]
    pub fn build_with_precision(self) -> (DatumTime, Uncertainty) {
        let precision = self.precision();
        (self.build(), precision)
    }

    #[cfg(feature = "native")]
    pub fn build(self) -> DatumTime {
        use chrono::{Local, Offset};

        if let Some(tz_name) = self.timezone.as_deref() {
            // Panic-safe: resolve_timezone() already validated the name
            // before it got here. A bad name at this point means the caller
            // skipped validation — surface it loudly rather than silently
            // falling back to Local.
            let tz: chrono_tz::Tz = tz_name
                .parse()
                .expect("TimeBuilder: timezone field must be a validated IANA name");
            // ISO-8601 overrides establish the base instant; otherwise we
            // anchor on "now" in the effective zone.
            let mut dt = if let Some(instant) = self.iso_instant_override {
                instant.with_timezone(&tz)
            } else if let Some(naive) = self.iso_naive_override {
                resolve_naive_in_tz(naive, tz)
            } else {
                Utc::now().with_timezone(&tz)
            };
            dt = dt - chrono::Duration::minutes(self.minutes_back);
            dt = dt - chrono::Duration::days(self.days_back);
            if let Some(date) = self.date_override {
                let time = self.time_override.unwrap_or_else(|| dt.time());
                if let Some(new_dt) = date.and_time(time).and_local_timezone(tz).single() {
                    dt = new_dt;
                }
            } else if let Some(time) = self.time_override {
                if let Some(new_dt) = dt.date_naive().and_time(time).and_local_timezone(tz).single() {
                    dt = new_dt;
                }
            }
            let utc = dt.with_timezone(&Utc);
            let offset_minutes = dt.offset().fix().local_minus_utc() / 60;
            return DatumTime { utc, offset_minutes, timezone: tz_name.to_string() };
        }

        // Local-zone path: same precedence — ISO overrides win over "now".
        let mut dt = if let Some(instant) = self.iso_instant_override {
            instant.with_timezone(&Local)
        } else if let Some(naive) = self.iso_naive_override {
            naive
                .and_local_timezone(Local)
                .single()
                .expect("Ambiguous/non-existent local datetime")
        } else {
            Local::now()
        };

        // Apply minute offsets first
        dt = dt - chrono::Duration::minutes(self.minutes_back);

        // Apply day offsets
        dt = dt - chrono::Duration::days(self.days_back);

        // Override date if specified
        if let Some(date) = self.date_override {
            let time = self.time_override.unwrap_or_else(|| dt.time());
            if let Some(new_dt) = date.and_time(time).and_local_timezone(Local).single() {
                dt = new_dt;
            }
        } else if let Some(time) = self.time_override {
            if let Some(new_dt) = dt.date_naive().and_time(time).and_local_timezone(Local).single() {
                dt = new_dt;
            }
        }

        let utc = dt.with_timezone(&Utc);
        let offset_minutes = dt.offset().local_minus_utc() / 60;
        let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());
        DatumTime { utc, offset_minutes, timezone }
    }
}

/// Resolve a no-zone ISO-8601 datetime into a concrete moment by reading
/// it as wall-clock in the supplied IANA zone. Picks the earliest valid
/// instant when the local time falls in a DST overlap, which matches what
/// users expect from "the first time the clock read 01:30" on fall-back
/// days. Spring-forward gaps fall through to a panic with a clear message
/// — they're rare, and silently shifting the user's input by an hour is
/// worse than failing loudly.
#[cfg(feature = "native")]
fn resolve_naive_in_tz(
    naive: NaiveDateTime,
    tz: chrono_tz::Tz,
) -> chrono::DateTime<chrono_tz::Tz> {
    use chrono::offset::LocalResult;
    match naive.and_local_timezone(tz) {
        LocalResult::Single(dt) => dt,
        LocalResult::Ambiguous(earlier, _later) => earlier,
        LocalResult::None => panic!(
            "TimeBuilder: ISO-8601 wall clock {naive} does not exist in {tz} (DST gap)"
        ),
    }
}
