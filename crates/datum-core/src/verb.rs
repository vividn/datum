//! Verb derivation: pure (state, dur, following_state) → Verb.
//!
//! da-5ns sub-bead 2. Truth table is normative — see the bead description
//! for the matrix. Insert/update wiring lives in sub-bead 3; the
//! `dm check verbs` drift detector lives in sub-bead 7.

use serde_json::Value as JsonValue;

use crate::dur::Dur;
use crate::entry::Verb;
use crate::error::{DatumError, Result};

/// Classify (state, dur, following_state) and return the input state
/// unchanged.
///
/// The return tuple is `(verb, state)`. The state is passed through
/// verbatim — there is no longer a null→true promotion. Verb::Point
/// entries with `state=None` are valid (the canonical "occur with no
/// state" shape, locked in by `dur == "MOMENT"`).
///
/// Moment rule (da-ju71.2): when `dur` parses as `Dur::moment()`, the
/// verb is `Verb::Point` regardless of state shape. This locks occur
/// entries to Point even when they carry structured state data.
///
/// Rejects:
/// - `state = None` + `following_state = Some` — fs implies a leading state.
/// - `state = Some` + `dur = None` + `following_state = Some` — fs requires
///   a duration to delimit the trailing edge.
/// - malformed ISO-8601 duration string.
/// - negative duration.
pub fn derive_verb(
    state: Option<JsonValue>,
    dur: Option<&Dur>,
    following_state: Option<JsonValue>,
) -> Result<(Verb, Option<JsonValue>)> {
    if dur.is_some_and(Dur::is_moment) {
        return Ok((Verb::Point, state));
    }

    let dur_kind = match dur {
        None => DurKind::None,
        Some(d) if d.is_unknown() => DurKind::None,
        Some(d) => classify_dur(d)?,
    };

    let state_set = state.is_some();
    let fs_set = following_state.is_some();

    let verb = match (state_set, dur_kind, fs_set) {
        // state=null rows (the "set" dur cases collapse 0 and >0; the
        // truth table does not distinguish them when state is absent).
        (false, DurKind::None, false) => Verb::Point,
        (false, DurKind::None, true) => {
            return Err(DatumError::Other(
                "following_state with no state and no duration is not a valid shape".to_string(),
            ));
        }
        (false, DurKind::Zero | DurKind::Positive, false) => Verb::Block,
        (false, DurKind::Zero | DurKind::Positive, true) => Verb::BlockSwitch,

        // state=set rows.
        (true, DurKind::None, false) => Verb::Switch,
        (true, DurKind::None, true) => {
            return Err(DatumError::Other(
                "following_state requires a duration when state is set".to_string(),
            ));
        }
        (true, DurKind::Zero, false) => Verb::Point,
        (true, DurKind::Zero, true) => Verb::PointSwitch,
        (true, DurKind::Positive, false) => Verb::Block,
        (true, DurKind::Positive, true) => Verb::BlockSwitch,
    };

    Ok((verb, state))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DurKind {
    None,
    Zero,
    Positive,
}

fn classify_dur(d: &Dur) -> Result<DurKind> {
    let Some(secs) = d.as_seconds() else {
        // Moment and Unknown are not measurements.
        return Ok(DurKind::None);
    };
    if secs < 0 {
        return Err(DatumError::Other(format!(
            "negative duration not allowed: {}",
            d.to_storage_string()
        )));
    }
    Ok(if secs == 0 {
        DurKind::Zero
    } else {
        DurKind::Positive
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ok(
        state: Option<JsonValue>,
        dur: Option<&str>,
        fs: Option<JsonValue>,
    ) -> (Verb, Option<JsonValue>) {
        let parsed = dur.map(|s| Dur::parse(s).expect("test dur must parse"));
        derive_verb(state, parsed.as_ref(), fs).expect("derive_verb should succeed")
    }

    /// Either the dur string is malformed (Dur::parse returns None — no
    /// `Dur` value to feed in) or `derive_verb` itself rejects the
    /// (state, dur, fs) shape. Both are "rejected" from the test's POV.
    fn assert_err(state: Option<JsonValue>, dur: Option<&str>, fs: Option<JsonValue>) {
        let parsed = match dur {
            Some(s) => match Dur::parse(s) {
                Some(d) => Some(d),
                None => return, // malformed at the Dur boundary — already rejected.
            },
            None => None,
        };
        derive_verb(state, parsed.as_ref(), fs).expect_err("derive_verb should reject");
    }

    // ─── Truth table (one test per row) ──────────────────────────────

    #[test]
    fn row_null_null_null_is_point_state_null() {
        let (v, s) = ok(None, None, None);
        assert_eq!(v, Verb::Point);
        assert_eq!(s, None);
    }

    #[test]
    fn row_null_null_set_rejects() {
        assert_err(None, None, Some(json!("end")));
    }

    #[test]
    fn row_null_set_null_is_block_state_null() {
        let (v, s) = ok(None, Some("PT30M"), None);
        assert_eq!(v, Verb::Block);
        assert_eq!(s, None);
    }

    #[test]
    fn row_null_set_set_is_block_switch_state_null() {
        let (v, s) = ok(None, Some("PT30M"), Some(json!("done")));
        assert_eq!(v, Verb::BlockSwitch);
        assert_eq!(s, None);
    }

    #[test]
    fn row_set_null_null_is_switch_state_as_given() {
        let (v, s) = ok(Some(json!("happy")), None, None);
        assert_eq!(v, Verb::Switch);
        assert_eq!(s, Some(json!("happy")));
    }

    #[test]
    fn row_set_null_set_rejects() {
        assert_err(Some(json!("happy")), None, Some(json!("sad")));
    }

    #[test]
    fn row_set_zero_null_is_point_state_as_given() {
        let (v, s) = ok(Some(json!(2)), Some("PT0S"), None);
        assert_eq!(v, Verb::Point);
        assert_eq!(s, Some(json!(2)));
    }

    #[test]
    fn row_set_zero_set_is_point_switch_state_as_given() {
        let (v, s) = ok(Some(json!("a")), Some("PT0S"), Some(json!("b")));
        assert_eq!(v, Verb::PointSwitch);
        assert_eq!(s, Some(json!("a")));
    }

    #[test]
    fn row_set_positive_null_is_block_state_as_given() {
        let (v, s) = ok(Some(json!("on")), Some("PT5M"), None);
        assert_eq!(v, Verb::Block);
        assert_eq!(s, Some(json!("on")));
    }

    #[test]
    fn row_set_positive_set_is_block_switch_state_as_given() {
        let (v, s) = ok(Some(json!("on")), Some("PT5M"), Some(json!("off")));
        assert_eq!(v, Verb::BlockSwitch);
        assert_eq!(s, Some(json!("on")));
    }

    // ─── Canonical zero forms ────────────────────────────────────────

    #[test]
    fn canonical_zero_forms_classify_as_zero() {
        for d in ["PT0S", "PT0M", "PT0H", "P0D", "P0M", "P0Y", "P0W"] {
            let (v, _) = ok(Some(json!(1)), Some(d), None);
            assert_eq!(v, Verb::Point, "expected Point for dur={d}");
        }
    }

    #[test]
    fn composite_zero_classifies_as_zero() {
        let (v, _) = ok(Some(json!(1)), Some("P0DT0H0M0S"), None);
        assert_eq!(v, Verb::Point);
    }

    // ─── Negative & malformed ────────────────────────────────────────

    #[test]
    fn negative_duration_rejected() {
        for d in ["-PT5M", "-PT1S", "-P1D"] {
            let parsed = Dur::parse(d).expect("negative dur must parse");
            assert!(
                derive_verb(Some(json!("on")), Some(&parsed), None).is_err(),
                "expected reject for dur={d}",
            );
        }
    }

    #[test]
    fn malformed_durations_reject_at_dur_boundary() {
        // After da-ju71.3, malformed strings are rejected by `Dur::parse`
        // before reaching `derive_verb`. The parser returns `None`; the
        // boundary (CLI / row_mapper) is responsible for surfacing it as
        // a parse error.
        for bad in [
            "", "P", "PT", "PXX", "PT5", "5", "P1D2H", "T5S", "PMT", "PT.S", "P-",
        ] {
            assert!(Dur::parse(bad).is_none(), "expected None for dur={bad:?}",);
        }
    }

    // ─── Positive durations correctly classified ─────────────────────

    #[test]
    fn positive_durations_classify_as_positive() {
        for d in ["PT1S", "PT1M", "PT1H", "P1D", "P1W", "PT1.5H", "P1Y", "P1M"] {
            let (v, _) = ok(Some(json!(1)), Some(d), None);
            assert_eq!(v, Verb::Block, "expected Block for dur={d}");
        }
    }

    #[test]
    fn comma_decimal_separator_accepted() {
        let (v, _) = ok(Some(json!(1)), Some("PT1,5H"), None);
        assert_eq!(v, Verb::Block);
    }

    // ─── Some(JsonValue::Null) is treated as set ──────────────────
    // The Option<JsonValue> distinction is the contract; an explicit
    // Value::Null is a real value, not absence. Pinning behavior so
    // sub-bead 3 callers can rely on it.

    #[test]
    fn explicit_json_null_is_treated_as_set() {
        let (v, s) = ok(Some(JsonValue::Null), None, None);
        assert_eq!(v, Verb::Switch);
        assert_eq!(s, Some(JsonValue::Null));
    }

    // ─── Dur::moment() locks Verb::Point (da-ju71.2) ───────────────────
    // `dur == "MOMENT"` overrides every state-driven path: a Moment
    // entry is always Point, whether state is null, a bare value, or a
    // structured object. State passes through verbatim — no promotion.

    #[test]
    fn moment_with_null_state_is_point() {
        let (v, s) = ok(None, Some("MOMENT"), None);
        assert_eq!(v, Verb::Point);
        assert_eq!(s, None);
    }

    #[test]
    fn moment_with_bare_state_is_point() {
        let (v, s) = ok(Some(json!("happy")), Some("MOMENT"), None);
        assert_eq!(v, Verb::Point);
        assert_eq!(s, Some(json!("happy")));
    }

    #[test]
    fn moment_with_structured_state_is_point() {
        let state = json!({"id": "car", "role": "driver"});
        let (v, s) = ok(Some(state.clone()), Some("MOMENT"), None);
        assert_eq!(v, Verb::Point);
        assert_eq!(s, Some(state));
    }
}
