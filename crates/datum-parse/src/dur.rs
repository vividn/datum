//! Parsed view over the `dur` / `dur_uncertainty` column pair.
//!
//! Storage stays as text on `Entry::dur` (da-ju71.2 narrows scope: the
//! type-cascade is parked under da-ju71.6). This type is the semantic
//! interpretation that callers reach for when they want to reason about a
//! row's duration intent without re-implementing the grammar each time.
//!
//! Storage forms for the span:
//! - `"MOMENT"` → [`DurSpan::Moment`] — explicit point-in-time (occur).
//! - ISO-8601 duration (`"PT5M"`, `"P1D"`, …) → [`DurSpan::Of`] — a real span.
//! - `"UNKNOWN"` → [`DurSpan::Unknown`] — open-ended / not-yet-known.
//!
//! `parse` returns `None` for anything else; the verb-derivation path
//! preserves today's "malformed ⇒ error" behavior by falling through to
//! the existing ISO-8601 classifier rather than treating an unparseable
//! string as `Unknown`.
//!
//! da-b8rs: the duration's uncertainty rides on [`Dur`] itself rather than
//! in a loose sibling column-field on `Entry`, matching
//! `DatumTime::uncertainty` on the occur side. One concept, one name, one
//! home, on both halves of the time model.

use crate::time::{format_iso_duration, parse_iso_duration};
use chrono::Duration;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

/// The duration itself, without its uncertainty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurSpan {
    Moment,
    Of(Duration),
    Unknown,
}

/// A duration together with how precisely it is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dur {
    pub span: DurSpan,
    /// How precisely `span` is known, as the ISO-8601 duration of the ±
    /// radius (`"PT1M"` for minute-resolution). Stored in
    /// `entries.dur_uncertainty`.
    ///
    /// `None` means *unknown* — we were not told how precisely the duration
    /// is known — which is **not** the claim "this duration is exact". Same
    /// rule, and the same reason, as `DatumTime::uncertainty` and the
    /// `timezone` field it follows: naming a precision we were not given
    /// invents history. Renderers that need a floor fall back to the full
    /// `span`, and say so where they do it.
    ///
    /// Deliberately **absent from the `content_hash` preimage** — see the
    /// note on [`crate::entry_id::content_hash`].
    pub uncertainty: Option<String>,
}

impl Dur {
    /// A `Dur` with the given span and no stated uncertainty.
    pub fn new(span: DurSpan) -> Self {
        Dur {
            span,
            uncertainty: None,
        }
    }

    /// Explicit point-in-time, uncertainty unstated.
    pub fn moment() -> Self {
        Dur::new(DurSpan::Moment)
    }

    /// Open-ended / not-yet-known duration, uncertainty unstated.
    pub fn unknown() -> Self {
        Dur::new(DurSpan::Unknown)
    }

    /// A real span, uncertainty unstated.
    pub fn of(d: Duration) -> Self {
        Dur::new(DurSpan::Of(d))
    }

    /// Attach (or clear, with `None`) the uncertainty.
    pub fn with_uncertainty(mut self, uncertainty: Option<String>) -> Self {
        self.uncertainty = uncertainty;
        self
    }

    /// Parse a stored `dur` value into its semantic form. Returns `None`
    /// for malformed ISO-8601 spans so the caller can decide whether to
    /// surface a parse error or fall back.
    ///
    /// Accepts an optional `±<iso>` uncertainty suffix — the form
    /// [`to_wire_string`] emits and the one a user can type as
    /// `dur=PT1H30M±PT5M`. The bare form (what the `dur` column holds)
    /// parses to an unstated uncertainty.
    ///
    /// [`to_wire_string`]: Self::to_wire_string
    pub fn parse(s: &str) -> Option<Self> {
        let (span_str, uncertainty) = match s.split_once('±') {
            Some((span, unc)) if !unc.is_empty() => {
                // Validate the suffix rather than storing any old token.
                parse_iso_duration(unc).ok()?;
                (span, Some(unc.to_string()))
            }
            Some(_) => return None,
            None => (s, None),
        };
        let span = match span_str {
            "MOMENT" => DurSpan::Moment,
            "UNKNOWN" => DurSpan::Unknown,
            iso => DurSpan::Of(parse_iso_duration(iso).ok()?),
        };
        Some(Dur { span, uncertainty })
    }

    /// Render the span to the canonical `dur` column value.
    ///
    /// Uncertainty is **not** included: it has its own column, and this is
    /// also the string `compute_hash` feeds into the `content_hash`
    /// preimage, which uncertainty is deliberately kept out of.
    pub fn to_storage_string(&self) -> String {
        match &self.span {
            DurSpan::Moment => "MOMENT".to_string(),
            DurSpan::Unknown => "UNKNOWN".to_string(),
            DurSpan::Of(d) => format_iso_duration(*d),
        }
    }

    /// Render span *and* uncertainty into the single string serde carries.
    ///
    /// Byte-identical to [`to_storage_string`] when no uncertainty is
    /// stated, which is the overwhelmingly common case — so a peer running
    /// a pre-da-b8rs build reads new payloads unchanged unless the entry
    /// genuinely carries a duration uncertainty.
    ///
    /// [`to_storage_string`]: Self::to_storage_string
    pub fn to_wire_string(&self) -> String {
        match &self.uncertainty {
            Some(u) => format!("{}±{u}", self.to_storage_string()),
            None => self.to_storage_string(),
        }
    }

    /// Span as integer seconds. `Moment` and `Unknown` return `None` —
    /// they have no measurable duration. `Of(d)` returns the chrono
    /// span, including zero-spans like `PT0S`.
    pub fn as_seconds(&self) -> Option<i64> {
        match &self.span {
            DurSpan::Of(d) => Some(d.num_seconds()),
            DurSpan::Moment | DurSpan::Unknown => None,
        }
    }

    /// True for [`DurSpan::Moment`]. Replaces the legacy `dur == "MOMENT"`
    /// string compare from the pre-da-ju71.3 era.
    pub fn is_moment(&self) -> bool {
        matches!(self.span, DurSpan::Moment)
    }

    /// True for [`DurSpan::Unknown`].
    pub fn is_unknown(&self) -> bool {
        matches!(self.span, DurSpan::Unknown)
    }

    /// True for `DurSpan::Of(d)` where the chrono span is exactly zero.
    /// Replaces compares against `"PT0S"` / `"P0D"` etc. — semantically
    /// they all collapse to a zero span after parse.
    pub fn is_zero_span(&self) -> bool {
        matches!(&self.span, DurSpan::Of(d) if d.is_zero())
    }
}

impl Serialize for Dur {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_wire_string())
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
        assert_eq!(d, Dur::moment());
        assert_eq!(d.to_storage_string(), "MOMENT");
    }

    #[test]
    fn unknown_round_trips() {
        let d = Dur::parse("UNKNOWN").unwrap();
        assert_eq!(d, Dur::unknown());
        assert_eq!(d.to_storage_string(), "UNKNOWN");
    }

    #[test]
    fn iso_duration_round_trips() {
        let d = Dur::parse("PT5M").unwrap();
        assert_eq!(d, Dur::of(Duration::minutes(5)));
        assert_eq!(d.to_storage_string(), "PT5M");
    }

    #[test]
    fn iso_day_duration_round_trips() {
        let d = Dur::parse("P1D").unwrap();
        assert_eq!(d, Dur::of(Duration::days(1)));
        assert_eq!(d.to_storage_string(), "P1D");
    }

    #[test]
    fn malformed_returns_none() {
        for bad in ["", "moment", "P", "PXX", "5", "PT5"] {
            assert!(Dur::parse(bad).is_none(), "expected None for {bad:?}");
        }
    }

    /// da-b8rs: uncertainty rides on the type but stays out of the `dur`
    /// column, which is also the hash preimage.
    #[test]
    fn uncertainty_round_trips_but_stays_out_of_storage_string() {
        let d = Dur::parse("PT1H30M±PT5M").unwrap();
        assert_eq!(d.span, DurSpan::Of(Duration::minutes(90)));
        assert_eq!(d.uncertainty.as_deref(), Some("PT5M"));
        assert_eq!(d.to_storage_string(), "PT1H30M");
        assert_eq!(d.to_wire_string(), "PT1H30M±PT5M");
    }

    /// Uncertainty is uniform across spans: `MOMENT` and `UNKNOWN` rows can
    /// carry one too, so renaming `dur_precision` -> `dur_uncertainty` needs
    /// no row-level reshaping (and drops nothing).
    #[test]
    fn uncertainty_is_allowed_on_every_span() {
        for span in ["MOMENT", "UNKNOWN", "PT30M"] {
            let d = Dur::parse(&format!("{span}±PT1M")).unwrap();
            assert_eq!(d.uncertainty.as_deref(), Some("PT1M"));
            assert_eq!(d.to_storage_string(), span);
        }
    }

    #[test]
    fn malformed_uncertainty_suffix_is_rejected() {
        for bad in ["PT30M±", "PT30M±5m", "PT30M±PXX"] {
            assert!(Dur::parse(bad).is_none(), "expected None for {bad:?}");
        }
    }

    /// The wire form is unchanged for an entry with no stated duration
    /// uncertainty — a pre-da-b8rs peer reads those bytes as it always did.
    #[test]
    fn wire_form_is_unchanged_without_uncertainty() {
        for s in ["MOMENT", "UNKNOWN", "PT30M"] {
            let d = Dur::parse(s).unwrap();
            assert_eq!(serde_json::to_value(&d).unwrap(), s);
        }
    }

    #[test]
    fn serde_round_trips_uncertainty() {
        let d = Dur::of(Duration::minutes(90)).with_uncertainty(Some("PT5M".into()));
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json, "PT1H30M±PT5M");
        assert_eq!(serde_json::from_value::<Dur>(json).unwrap(), d);
    }
}
