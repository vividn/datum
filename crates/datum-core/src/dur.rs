//! Parsed view over the `dur` text column.
//!
//! Storage stays as `Option<String>` on `Entry::dur` (da-ju71.2 narrows
//! scope: the type-cascade is parked under da-ju71.6). This enum is the
//! semantic interpretation that callers reach for when they want to
//! reason about a row's duration intent without re-implementing the
//! grammar each time.
//!
//! Storage forms:
//! - `"MOMENT"` → [`Dur::Moment`] — explicit point-in-time (occur).
//! - ISO-8601 duration (`"PT5M"`, `"P1D"`, …) → [`Dur::Of`] — a real span.
//! - `"UNKNOWN"` → [`Dur::Unknown`] — open-ended / not-yet-known.
//!
//! `parse` returns `None` for anything else; the verb-derivation path
//! preserves today's "malformed ⇒ error" behavior by falling through to
//! the existing ISO-8601 classifier rather than treating an unparseable
//! string as `Unknown`.

use chrono::Duration;
use datum_parse::time::{format_iso_duration, parse_iso_duration};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dur {
    Moment,
    Of(Duration),
    Unknown,
}

impl Dur {
    /// Parse a stored `dur` value into its semantic form. Returns `None`
    /// for malformed ISO-8601 spans so the caller can decide whether to
    /// surface a parse error or fall back.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "MOMENT" => Some(Dur::Moment),
            "UNKNOWN" => Some(Dur::Unknown),
            iso => parse_iso_duration(iso).ok().map(Dur::Of),
        }
    }

    /// Render to the canonical storage string.
    pub fn to_storage_string(&self) -> String {
        match self {
            Dur::Moment => "MOMENT".to_string(),
            Dur::Unknown => "UNKNOWN".to_string(),
            Dur::Of(d) => format_iso_duration(*d),
        }
    }

    /// Span as integer seconds. `Moment` and `Unknown` return `None` —
    /// they have no measurable duration. `Of(d)` returns the chrono
    /// span, including zero-spans like `PT0S`.
    pub fn as_seconds(&self) -> Option<i64> {
        match self {
            Dur::Of(d) => Some(d.num_seconds()),
            Dur::Moment | Dur::Unknown => None,
        }
    }

    /// True for `Dur::Moment`. Replaces the legacy `dur == "MOMENT"`
    /// string compare from the pre-da-ju71.3 era.
    pub fn is_moment(&self) -> bool {
        matches!(self, Dur::Moment)
    }

    /// True for `Dur::Unknown`.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Dur::Unknown)
    }

    /// True for `Dur::Of(d)` where the chrono span is exactly zero.
    /// Replaces compares against `"PT0S"` / `"P0D"` etc. — semantically
    /// they all collapse to a zero span after parse.
    pub fn is_zero_span(&self) -> bool {
        matches!(self, Dur::Of(d) if d.is_zero())
    }
}

impl Serialize for Dur {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_storage_string())
    }
}

impl<'de> Deserialize<'de> for Dur {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Dur::parse(&s).ok_or_else(|| D::Error::custom(format!("invalid dur value: {s:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moment_round_trips() {
        let d = Dur::parse("MOMENT").unwrap();
        assert_eq!(d, Dur::Moment);
        assert_eq!(d.to_storage_string(), "MOMENT");
    }

    #[test]
    fn unknown_round_trips() {
        let d = Dur::parse("UNKNOWN").unwrap();
        assert_eq!(d, Dur::Unknown);
        assert_eq!(d.to_storage_string(), "UNKNOWN");
    }

    #[test]
    fn iso_duration_round_trips() {
        let d = Dur::parse("PT5M").unwrap();
        assert_eq!(d, Dur::Of(Duration::minutes(5)));
        assert_eq!(d.to_storage_string(), "PT5M");
    }

    #[test]
    fn iso_day_duration_round_trips() {
        let d = Dur::parse("P1D").unwrap();
        assert_eq!(d, Dur::Of(Duration::days(1)));
        assert_eq!(d.to_storage_string(), "P1D");
    }

    #[test]
    fn malformed_returns_none() {
        for bad in ["", "moment", "P", "PXX", "5", "PT5"] {
            assert!(Dur::parse(bad).is_none(), "expected None for {bad:?}");
        }
    }
}
