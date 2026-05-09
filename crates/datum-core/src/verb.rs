//! Verb derivation: pure (state, dur, following_state) → Verb.
//!
//! da-5ns sub-bead 2. Truth table is normative — see the bead description
//! for the matrix. Insert/update wiring lives in sub-bead 3; the
//! `dm check --verbs` drift detector lives in sub-bead 7.

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
/// Moment rule (da-ju71.2): when `dur` parses as `Dur::Moment`, the
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
    dur: Option<&str>,
    following_state: Option<JsonValue>,
) -> Result<(Verb, Option<JsonValue>)> {
    if let Some(s) = dur {
        if matches!(Dur::parse(s), Some(Dur::Moment)) {
            return Ok((Verb::Point, state));
        }
    }

    let dur_kind = match dur {
        None => DurKind::None,
        Some(s) => classify_dur(s)?,
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

fn classify_dur(s: &str) -> Result<DurKind> {
    let secs = parse_iso_duration_secs(s)
        .ok_or_else(|| DatumError::Other(format!("malformed ISO-8601 duration: {s}")))?;
    if secs < 0 {
        return Err(DatumError::Other(format!(
            "negative duration not allowed: {s}"
        )));
    }
    Ok(if secs == 0 {
        DurKind::Zero
    } else {
        DurKind::Positive
    })
}

/// Parse an ISO-8601 duration to total seconds. Years and months are
/// approximated (365d / 30d) — fine here because this helper is only
/// used for zero/positive/negative classification; the canonical `dur`
/// string is stored verbatim, never recomputed from this number.
///
/// Returns `None` for malformed input. Negative results (leading `-`)
/// flow through; the caller decides whether to reject.
fn parse_iso_duration_secs(input: &str) -> Option<i64> {
    let (sign, body) = match input.strip_prefix('-') {
        Some(rest) => (-1f64, rest),
        None => (1.0, input),
    };
    let rest = body.strip_prefix('P')?;
    if rest.is_empty() {
        return None;
    }

    // Week form (PnW) is mutually exclusive with the rest of the grammar.
    if rest.ends_with('W') && !rest.contains('T') {
        let n_str = &rest[..rest.len() - 1];
        if n_str.is_empty() {
            return None;
        }
        let n: f64 = parse_decimal(n_str)?;
        return Some((sign * n * 7.0 * 86400.0) as i64);
    }

    let mut total: f64 = 0.0;
    let mut buf = String::new();
    let mut seen_t = false;
    let mut had_any = false;

    for c in rest.chars() {
        if c == 'T' {
            if seen_t || !buf.is_empty() {
                return None;
            }
            seen_t = true;
        } else if c.is_ascii_digit() || c == '.' || c == ',' {
            buf.push(if c == ',' { '.' } else { c });
        } else {
            if buf.is_empty() {
                return None;
            }
            let n: f64 = parse_decimal(&buf)?;
            buf.clear();
            let secs = match (seen_t, c) {
                (false, 'Y') => n * 365.0 * 86400.0,
                (false, 'M') => n * 30.0 * 86400.0,
                (false, 'D') => n * 86400.0,
                (true, 'H') => n * 3600.0,
                (true, 'M') => n * 60.0,
                (true, 'S') => n,
                _ => return None,
            };
            total += secs;
            had_any = true;
        }
    }

    if !buf.is_empty() {
        return None;
    }
    if !had_any {
        return None;
    }

    Some((sign * total) as i64)
}

/// Parse a decimal that must contain at least one digit. Rejects the
/// empty string, a lone `.`, and trailing-only decimals like `1.`.
fn parse_decimal(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    let digit_count = s.chars().filter(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return None;
    }
    s.parse::<f64>().ok()
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
        derive_verb(state, dur, fs).expect("derive_verb should succeed")
    }

    fn assert_err(state: Option<JsonValue>, dur: Option<&str>, fs: Option<JsonValue>) {
        derive_verb(state, dur, fs).expect_err("derive_verb should reject");
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
            assert!(
                derive_verb(Some(json!("on")), Some(d), None).is_err(),
                "expected reject for dur={d}",
            );
        }
    }

    #[test]
    fn malformed_durations_rejected() {
        for bad in [
            "", "P", "PT", "PXX", "PT5", "5", "P1D2H", "T5S",
            "PMT", "PT.S", "P-",
        ] {
            assert!(
                derive_verb(Some(json!(1)), Some(bad), None).is_err(),
                "expected reject for dur={bad:?}",
            );
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

    // ─── Dur::Moment locks Verb::Point (da-ju71.2) ───────────────────
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
