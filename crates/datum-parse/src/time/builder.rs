//! `TimeBuilder` — compose stacked flags into a final `DatumTime`.

use chrono::NaiveTime;

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
        if let Some(abs) = arg.absolute {
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
        use chrono::{Local, Offset, Utc};

        if let Some(tz_name) = self.timezone.as_deref() {
            // Panic-safe: resolve_timezone() already validated the name
            // before it got here. A bad name at this point means the caller
            // skipped validation — surface it loudly rather than silently
            // falling back to Local.
            let tz: chrono_tz::Tz = tz_name
                .parse()
                .expect("TimeBuilder: timezone field must be a validated IANA name");
            let mut dt = Utc::now().with_timezone(&tz);
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

        let mut dt = Local::now();

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
