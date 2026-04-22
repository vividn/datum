//! Parse `-d` / `--date` arguments.
//!
//! Supports relative day offsets (`+1`, `-1`, `0`) and absolute ISO dates
//! (`2024-03-16`), with an optional `±N[unit]` uncertainty suffix.

use crate::error::{ParseError, Result};
use crate::time::uncertainty::{parse_uncertainty, split_uncertainty, Uncertainty};

/// Parsed result of a date argument (-d flag).
pub struct DateArg {
    pub days_offset: i64,
    pub absolute: Option<chrono::NaiveDate>,
    pub uncertainty: Option<Uncertainty>,
}

/// Parse a -d / --date argument.
/// Supports: "+1", "-1", "0", or ISO date "2024-03-16"
/// plus optional `±N[unit]` uncertainty suffix.
pub fn parse_date_arg(s: &str) -> Result<DateArg> {
    let (primary, uncert_body) = split_uncertainty(s);
    let uncertainty = uncert_body.map(parse_uncertainty).transpose()?;

    // Relative integer offset
    if let Ok(n) = primary.parse::<i64>() {
        return Ok(DateArg { days_offset: n, absolute: None, uncertainty });
    }

    // Absolute ISO date
    if let Ok(d) = chrono::NaiveDate::parse_from_str(primary, "%Y-%m-%d") {
        return Ok(DateArg { days_offset: 0, absolute: Some(d), uncertainty });
    }

    Err(ParseError::InvalidTime(format!("Cannot parse date: '{s}'")))
}
