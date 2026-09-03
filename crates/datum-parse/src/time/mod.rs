//! Time parsing and representation.
//!
//! Submodules mirror the layout the R2 bead calls for (and the original
//! datum `src/time/` TS module):
//!
//! * [`context`] — `TimeCtx` / `DefaultDirection`: what ambiguous input
//!   resolves against.
//! * [`uncertainty`] — `Uncertainty` + `UncertaintyUnit` (the granularity
//!   label formerly called `PrecisionUnit`) + `±N[unit]` suffix parsing.
//! * [`datum_time`] — `DatumTime` (UTC + offset + IANA name).
//! * [`resolve`] — the one local→UTC resolution utility (`Zone`, `Resolved`,
//!   `ZoneTimeline`). Every wall-clock→instant conversion goes through it;
//!   `clippy.toml` disallows the chrono calls it wraps.
//! * [`date`] — `-d` / `--date` argument parsing (`DateArg`).
//! * [`time`] — `-t` / `--time` argument parsing (`TimeArg`).
//! * [`dur`] — `--dur` argument parsing.
//! * [`builder`] — `TimeBuilder` composes stacked flags into a `DatumTime`.
//! * [`human_time`] — doc-anchor stub; canonical impl is
//!   `datum_commands::render::action::format_timestamp`.
//! * [`get_timezone`] — doc-anchor stub; canonical impl is
//!   `datum_parse::input::time_args::resolve_timezone`.

pub mod builder;
#[cfg(feature = "native")]
pub mod clock;
pub mod context;
pub mod date;
pub mod datum_time;
pub mod dur;
pub mod get_timezone;
pub mod human_time;
pub mod resolve;
// `time::time` stutters, but the module is named for its content like its
// siblings `date` and `dur`, and `pub use` below flattens it — no caller
// ever writes the doubled path.
#[allow(clippy::module_inception)]
pub mod time;
pub mod uncertainty;

pub use self::builder::TimeBuilder;
pub use self::context::{DefaultDirection, TimeCtx};
pub use self::date::{parse_date, DateArg};
pub use self::datum_time::DatumTime;
pub use self::dur::{
    format_human_duration, format_iso_duration, parse_duration, parse_iso_duration,
    quick_uncertainty_for_count, DurationPrecision,
};
pub use self::resolve::{Resolved, Zone, ZoneSegment, ZoneTimeline};
pub use self::time::{count_stacked_flag, parse_time, TimeArg};
pub use self::uncertainty::{
    parse_uncertainty, split_direction_suffix, split_uncertainty, Direction, Uncertainty,
    UncertaintyUnit,
};

pub use crate::input::time_args::resolve_timezone;

#[cfg(test)]
mod tests;
