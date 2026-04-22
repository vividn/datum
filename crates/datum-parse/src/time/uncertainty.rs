//! `±N[unit]` uncertainty suffix parsing.

use crate::error::{ParseError, Result};
use crate::time::parse_duration_str::{parse_iso_duration, split_unit};
use crate::time::precision::PrecisionUnit;

/// Uncertainty attached to a time reference — `±N[unit]` on the input.
///
/// `seconds` is the absolute radius of the interval. `unit` names the
/// granularity for display/rollup (e.g. a `±5m` input stores 300s + minute).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uncertainty {
    pub seconds: u32,
    pub unit: PrecisionUnit,
}

impl Uncertainty {
    pub fn exact() -> Self {
        Uncertainty { seconds: 0, unit: PrecisionUnit::Exact }
    }
}

/// Split an input string into its primary-time portion and an optional
/// trailing uncertainty suffix (`±N[unit]` or `+-N[unit]`).
///
/// The uncertainty marker may only appear once and must sit at the end of
/// the string. Returns `(primary, Some(uncertainty_body))` when found, or
/// `(primary, None)` otherwise.
pub fn split_uncertainty(s: &str) -> (&str, Option<&str>) {
    if let Some(idx) = s.rfind('±') {
        return (&s[..idx], Some(&s[idx + '±'.len_utf8()..]));
    }
    // ASCII form `+-` — only split when it's preceded by a non-sign char so
    // that a leading-sign relative offset like `-t +5` isn't misparsed.
    if let Some(idx) = s.rfind("+-") {
        if idx > 0 {
            return (&s[..idx], Some(&s[idx + 2..]));
        }
    }
    (s, None)
}

/// Parse the body of an uncertainty suffix (the portion after `±` or `+-`).
///
/// Accepts the same unit suffixes as `parse_duration`; bare numbers default
/// to minutes. The returned `PrecisionUnit` mirrors the input unit so a
/// `±5m` round-trips as `minute` rather than being collapsed to `second`.
pub fn parse_uncertainty(body: &str) -> Result<Uncertainty> {
    if body.is_empty() {
        return Err(ParseError::InvalidTime(
            "empty uncertainty body (expected N or N<unit>)".to_string(),
        ));
    }
    // ISO 8601 duration form
    if body.starts_with('P') {
        let d = parse_iso_duration(body)?;
        let secs = d.num_seconds().unsigned_abs() as u32;
        return Ok(Uncertainty { seconds: secs, unit: PrecisionUnit::Second });
    }

    let (num_str, unit_str) = split_unit(body);
    let n: f64 = num_str
        .parse()
        .map_err(|_| ParseError::InvalidTime(format!("Cannot parse uncertainty: '{body}'")))?;
    let (secs, unit) = match unit_str {
        "s" | "sec" | "second" | "seconds" => (n as u32, PrecisionUnit::Second),
        "m" | "min" | "minute" | "minutes" => ((n * 60.0) as u32, PrecisionUnit::Minute),
        "h" | "hr" | "hour" | "hours" => ((n * 3600.0) as u32, PrecisionUnit::Hour),
        "d" | "day" | "days" => ((n * 86400.0) as u32, PrecisionUnit::Date),
        "" => ((n * 60.0) as u32, PrecisionUnit::Minute), // default: minutes
        other => {
            return Err(ParseError::InvalidTime(format!(
                "Unknown uncertainty unit: '{other}'"
            )))
        }
    };
    Ok(Uncertainty { seconds: secs, unit })
}
