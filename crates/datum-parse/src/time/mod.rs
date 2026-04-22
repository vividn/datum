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
//! * [`human_time`] — display helpers (stub — populated in R2 step 2+).
//! * [`get_occur_time`] / [`get_timezone`] — higher-level helpers (stub).

pub mod builder;
pub mod datum_time;
pub mod get_occur_time;
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

#[cfg(test)]
mod tests;
