//! Time parsing and representation.
//!
//! Submodules mirror the layout the R2 bead calls for (and the original
//! datum `src/time/` TS module):
//!
//! * [`precision`] — `PrecisionUnit` (formerly in `datum-core::entry`).
//! * [`uncertainty`] — `Uncertainty` struct + `±N[unit]` suffix parsing.
//! * [`datum_time`] — `DatumTime` (UTC + offset + IANA name).
//! * [`parse_date_str`] — `-d` / `--date` argument parsing (`DateArg`).
//! * [`parse_time_str`] — `-t` / `--time` argument parsing (`TimeArg`).
//! * [`parse_duration_str`] — `--dur` argument parsing.
//! * [`builder`] — `TimeBuilder` composes stacked flags into a `DatumTime`.
//! * [`human_time`] — doc-anchor stub; canonical impl is
//!   `datum_commands::render::action::format_timestamp`.
//! * [`get_timezone`] — doc-anchor stub; canonical impl is
//!   `datum_parse::input::time_args::resolve_timezone`.

pub mod builder;
#[cfg(feature = "native")]
pub mod clock;
pub mod datum_time;
pub mod get_timezone;
pub mod human_time;
pub mod parse_date_str;
pub mod parse_duration_str;
pub mod parse_time_str;
pub mod precision;
pub mod uncertainty;

pub use builder::TimeBuilder;
pub use datum_time::DatumTime;
pub use parse_date_str::{parse_date_arg, DateArg};
pub use parse_duration_str::{parse_duration, quick_precision_for_count};
pub use parse_time_str::{count_stacked_flag, parse_time_arg, TimeArg};
pub use precision::PrecisionUnit;
pub use uncertainty::{parse_uncertainty, split_uncertainty, Uncertainty};

pub use crate::input::time_args::resolve_timezone;

#[cfg(test)]
mod tests;
