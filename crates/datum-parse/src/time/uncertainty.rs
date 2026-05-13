//! `±N[unit]` uncertainty suffix parsing.

use crate::error::{ParseError, Result};
use crate::time::parse_duration_str::{parse_iso_duration, split_unit};
use crate::time::precision::PrecisionUnit;

/// Direction hint from a trailing `+` / `-` suffix on a time or date arg.
///
/// `Future` forces resolution to the next occurrence; `Past` forces the most
/// recent prior occurrence. Set by [`split_direction_suffix`]; consumed when
/// composing the final instant. Already-pinned inputs (ISO instants, year-
/// pinned dates, relative `±N` offsets) ignore the hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Future,
    Past,
}

/// Peel off a trailing `+` or `-` direction marker from `s`.
///
/// Errors when both directions are stacked at the tail (`3+-`, `3-+`,
/// `3++`, `3--`) — those are explicit conflicts. Otherwise returns the
/// body without the suffix and the parsed direction (or `None` when no
/// direction marker is present).
///
/// Run this *before* [`split_uncertainty`] so a direction marker that
/// follows an uncertainty (`3±5m+`) is still recognised.
pub fn split_direction_suffix(s: &str) -> Result<(&str, Option<Direction>)> {
    let bytes = s.as_bytes();
    let n = bytes.len();
    if n == 0 {
        return Ok((s, None));
    }
    let last = bytes[n - 1];
    if last != b'+' && last != b'-' {
        return Ok((s, None));
    }
    if n >= 2 {
        let prev = bytes[n - 2];
        if prev == b'+' || prev == b'-' {
            return Err(ParseError::InvalidTime(format!(
                "Conflicting direction suffix: '{s}'"
            )));
        }
    } else {
        // Bare `+` or `-` is not a valid input.
        return Err(ParseError::InvalidTime(format!(
            "Empty input before direction suffix: '{s}'"
        )));
    }
    let dir = if last == b'+' {
        Direction::Future
    } else {
        Direction::Past
    };
    Ok((&s[..n - 1], Some(dir)))
}

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
        Uncertainty {
            seconds: 0,
            unit: PrecisionUnit::Exact,
        }
    }

    /// Render this uncertainty as the ISO-8601 duration string stored in
    /// `entries.occur_time_precision`. Returns `None` for exact (no
    /// uncertainty) so the column lands as NULL.
    pub fn to_iso_precision(&self) -> Option<String> {
        if self.seconds == 0 || matches!(self.unit, PrecisionUnit::Exact) {
            return None;
        }
        match self.unit {
            PrecisionUnit::Second => Some(format!("PT{}S", self.seconds)),
            PrecisionUnit::Minute => Some(format!("PT{}M", (self.seconds / 60).max(1))),
            PrecisionUnit::Hour => Some(format!("PT{}H", (self.seconds / 3600).max(1))),
            PrecisionUnit::Date => Some(format!("P{}D", (self.seconds / 86_400).max(1))),
            PrecisionUnit::Month => Some("P1M".to_string()),
            PrecisionUnit::Year => Some("P1Y".to_string()),
            PrecisionUnit::Exact => None,
        }
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
        return Ok(Uncertainty {
            seconds: secs,
            unit: PrecisionUnit::Second,
        });
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
    Ok(Uncertainty {
        seconds: secs,
        unit,
    })
}
