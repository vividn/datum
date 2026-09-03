//! Uncertainty — how precisely a time or a duration is known.
//!
//! One concept, one name. This module owns both the `±N[unit]` suffix
//! grammar and [`UncertaintyUnit`], the granularity label that used to be
//! called `PrecisionUnit` and live in its own module (and, before that, in
//! `datum-core::entry`). The split name was why nobody could tell whether
//! uncertainty reached storage: it does, as the `occur_time_uncertainty` /
//! `dur_uncertainty` columns (da-b8rs).

use serde::{Deserialize, Serialize};

use crate::error::{ParseError, Result};
use crate::time::parse_duration_str::{parse_iso_duration, split_unit};

/// Display granularity of an uncertainty. An input of `±5m` is stored as
/// `seconds = 300` + `unit = Minute` so round-tripping preserves the
/// user-visible unit rather than collapsing to seconds.
///
/// The serde representation (`rename_all = "lowercase"`) and the
/// case-insensitive `FromStr` are preserved verbatim from the days when this
/// was `PrecisionUnit`: the *type* was renamed, the *values* were not, so
/// rows and payloads written before da-b8rs still deserialize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum UncertaintyUnit {
    #[default]
    Exact,
    Second,
    Minute,
    Hour,
    Date,
    Month,
    Year,
}

impl std::fmt::Display for UncertaintyUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UncertaintyUnit::Exact => write!(f, "exact"),
            UncertaintyUnit::Second => write!(f, "second"),
            UncertaintyUnit::Minute => write!(f, "minute"),
            UncertaintyUnit::Hour => write!(f, "hour"),
            UncertaintyUnit::Date => write!(f, "date"),
            UncertaintyUnit::Month => write!(f, "month"),
            UncertaintyUnit::Year => write!(f, "year"),
        }
    }
}

impl std::str::FromStr for UncertaintyUnit {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "exact" => Ok(UncertaintyUnit::Exact),
            "second" => Ok(UncertaintyUnit::Second),
            "minute" => Ok(UncertaintyUnit::Minute),
            "hour" => Ok(UncertaintyUnit::Hour),
            "date" => Ok(UncertaintyUnit::Date),
            "month" => Ok(UncertaintyUnit::Month),
            "year" => Ok(UncertaintyUnit::Year),
            _ => Err(format!("Unknown uncertainty unit: '{s}'")),
        }
    }
}

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
    pub unit: UncertaintyUnit,
}

impl Uncertainty {
    pub fn exact() -> Self {
        Uncertainty {
            seconds: 0,
            unit: UncertaintyUnit::Exact,
        }
    }

    /// Render this uncertainty as the ISO-8601 duration string carried on
    /// [`DatumTime::uncertainty`] / [`Dur::uncertainty`] and stored in the
    /// `occur_time_uncertainty` / `dur_uncertainty` columns. Returns `None`
    /// for exact (no uncertainty) so the column lands as NULL.
    ///
    /// [`DatumTime::uncertainty`]: crate::time::DatumTime::uncertainty
    pub fn to_iso(&self) -> Option<String> {
        if self.seconds == 0 || matches!(self.unit, UncertaintyUnit::Exact) {
            return None;
        }
        match self.unit {
            UncertaintyUnit::Second => Some(format!("PT{}S", self.seconds)),
            UncertaintyUnit::Minute => Some(format!("PT{}M", (self.seconds / 60).max(1))),
            UncertaintyUnit::Hour => Some(format!("PT{}H", (self.seconds / 3600).max(1))),
            UncertaintyUnit::Date => Some(format!("P{}D", (self.seconds / 86_400).max(1))),
            UncertaintyUnit::Month => Some("P1M".to_string()),
            UncertaintyUnit::Year => Some("P1Y".to_string()),
            UncertaintyUnit::Exact => None,
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
/// to minutes. The returned `UncertaintyUnit` mirrors the input unit so a
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
            unit: UncertaintyUnit::Second,
        });
    }

    let (num_str, unit_str) = split_unit(body);
    let n: f64 = num_str
        .parse()
        .map_err(|_| ParseError::InvalidTime(format!("Cannot parse uncertainty: '{body}'")))?;
    let (secs, unit) = match unit_str {
        "s" | "sec" | "second" | "seconds" => (n as u32, UncertaintyUnit::Second),
        "m" | "min" | "minute" | "minutes" => ((n * 60.0) as u32, UncertaintyUnit::Minute),
        "h" | "hr" | "hour" | "hours" => ((n * 3600.0) as u32, UncertaintyUnit::Hour),
        "d" | "day" | "days" => ((n * 86400.0) as u32, UncertaintyUnit::Date),
        "" => ((n * 60.0) as u32, UncertaintyUnit::Minute), // default: minutes
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

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[UncertaintyUnit] = &[
        UncertaintyUnit::Exact,
        UncertaintyUnit::Second,
        UncertaintyUnit::Minute,
        UncertaintyUnit::Hour,
        UncertaintyUnit::Date,
        UncertaintyUnit::Month,
        UncertaintyUnit::Year,
    ];

    #[test]
    fn display_and_fromstr_round_trip() {
        for v in ALL {
            let s = v.to_string();
            let parsed: UncertaintyUnit = s.parse().unwrap();
            assert_eq!(*v, parsed, "round-trip failed for {v}");
        }
    }

    #[test]
    fn display_matches_serde_json_string() {
        for v in ALL {
            let displayed = v.to_string();
            let json = serde_json::to_value(v).unwrap();
            assert_eq!(
                serde_json::Value::String(displayed.clone()),
                json,
                "Display vs serde drift for {v}",
            );
        }
    }

    /// The da-b8rs rename touched the *type* name only. Rows and sync
    /// payloads written as `PrecisionUnit` carry these same lowercase
    /// strings, so the wire values must not drift with the type.
    #[test]
    fn serde_values_survive_the_precision_rename() {
        for (v, expected) in [
            (UncertaintyUnit::Exact, "exact"),
            (UncertaintyUnit::Minute, "minute"),
            (UncertaintyUnit::Date, "date"),
        ] {
            assert_eq!(serde_json::to_value(v).unwrap(), expected);
            let back: UncertaintyUnit =
                serde_json::from_value(serde_json::json!(expected)).unwrap();
            assert_eq!(v, back);
        }
    }

    #[test]
    fn serde_round_trips_through_json() {
        for v in ALL {
            let json = serde_json::to_value(v).unwrap();
            let back: UncertaintyUnit = serde_json::from_value(json).unwrap();
            assert_eq!(*v, back, "serde round-trip failed for {v}");
        }
    }

    #[test]
    fn fromstr_is_case_insensitive() {
        assert_eq!(
            "HOUR".parse::<UncertaintyUnit>().unwrap(),
            UncertaintyUnit::Hour
        );
        assert_eq!(
            "Hour".parse::<UncertaintyUnit>().unwrap(),
            UncertaintyUnit::Hour
        );
    }

    #[test]
    fn fromstr_rejects_unknown() {
        assert!("fortnight".parse::<UncertaintyUnit>().is_err());
    }
}
