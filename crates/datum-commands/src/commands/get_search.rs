//! Matcher-driven search engine for `datum get FIELD k OP v ...` (da-yhh1).
//!
//! Evaluates a slice of [`Matcher`]s against an [`Entry`] under AND
//! semantics (every matcher must hold). Lookup keys resolve in this
//! order:
//!
//! * **Reserved columns** — `field`, `verb`, `state` / `value`, `dur`
//!   (seconds), `start_time` / `end_time` (RFC3339 strings).
//! * **`extra_data` paths** — dotted notation (`weight`, `session.mood`).
//!   Unit-annotated leaves `{"v": x, "u": "kg"}` are unwrapped to `x`
//!   for numeric comparisons so `weight>75` works on either form.
//!
//! Value comparison policy:
//! * **Eq/Ne** — string-equal first; falls back to numeric equality so
//!   `count=5` matches `5` and `5.0` both.
//! * **Gt/Lt/Gte/Lte** — numeric only. A non-numeric LHS or RHS fails
//!   the match (no implicit string ordering). RHS duration literals use
//!   the same dialect as a duration argument / `--since` (`4h`, `30m`, `1:30`,
//!   `PT4H`), so `dur>4h` works everywhere the rest of the tool taught it.
//! * **Regex** — `/…/`-delimited values strip their slashes; bare
//!   values compile verbatim. Match is "contains" via `Regex::is_match`.
//!   Patterns are pre-validated by [`validate`] before filtering so a
//!   bad regex surfaces as a user error, not an empty result.

use datum_core::entry::Entry;
use datum_parse::input::{Matcher, MatcherOp};
use regex_lite::Regex;
use serde_json::Value as JsonValue;

/// Pre-validate matchers before filtering: a regex that fails to
/// compile is a USER error and must surface as one ("invalid regex
/// 'x[': …"), never as "no entries on field". Keeping validation here
/// (run once, up front) leaves [`filter`] infallible.
pub(crate) fn validate(matchers: &[Matcher]) -> anyhow::Result<()> {
    for m in matchers {
        if m.op == MatcherOp::Regex {
            let pattern = regex_pattern(&m.value);
            Regex::new(pattern).map_err(|e| anyhow::anyhow!("invalid regex '{pattern}': {e}"))?;
        }
    }
    Ok(())
}

/// Filter `entries` to only those satisfying every matcher.
pub(crate) fn filter(entries: Vec<Entry>, matchers: &[Matcher]) -> Vec<Entry> {
    entries
        .into_iter()
        .filter(|e| matches_all(e, matchers))
        .collect()
}

fn matches_all(entry: &Entry, matchers: &[Matcher]) -> bool {
    matchers.iter().all(|m| match_one(entry, m))
}

fn match_one(entry: &Entry, m: &Matcher) -> bool {
    let lhs = resolve_key(entry, &m.key);
    match m.op {
        MatcherOp::Eq => compare_eq(lhs.as_ref(), &m.value),
        MatcherOp::Ne => !compare_eq(lhs.as_ref(), &m.value),
        MatcherOp::Gt => compare_num(lhs.as_ref(), &m.value, |a, b| a > b),
        MatcherOp::Lt => compare_num(lhs.as_ref(), &m.value, |a, b| a < b),
        MatcherOp::Gte => compare_num(lhs.as_ref(), &m.value, |a, b| a >= b),
        MatcherOp::Lte => compare_num(lhs.as_ref(), &m.value, |a, b| a <= b),
        MatcherOp::Regex => compare_regex(lhs.as_ref(), &m.value),
    }
}

fn resolve_key(entry: &Entry, key: &str) -> Option<JsonValue> {
    match key {
        "field" => Some(JsonValue::String(entry.field.clone())),
        "verb" => Some(JsonValue::String(entry.verb.to_string())),
        "state" | "value" => entry.state.clone(),
        "dur" => entry
            .dur
            .as_ref()
            .and_then(|d| d.as_seconds())
            .map(JsonValue::from),
        "start_time" => entry
            .occur_time
            .as_ref()
            .map(|t| JsonValue::String(t.utc.to_rfc3339())),
        "end_time" => entry
            .computed_end_time()
            .map(|t| JsonValue::String(t.utc.to_rfc3339())),
        path => entry.data.as_ref().and_then(|d| lookup_path(d, path)),
    }
}

/// Walk a dotted path through a JSON value. Returns leaves directly;
/// unwraps `{"v": x, "u": "..."}` unit-annotation objects to `x` so
/// matchers don't have to know about the annotation shape.
fn lookup_path(root: &JsonValue, path: &str) -> Option<JsonValue> {
    let mut cur = root;
    for segment in path.split('.') {
        cur = cur.get(segment)?;
    }
    Some(unwrap_unit_annotation(cur))
}

fn unwrap_unit_annotation(v: &JsonValue) -> JsonValue {
    if let Some(obj) = v.as_object() {
        if let (Some(num), Some(_)) = (obj.get("v"), obj.get("u")) {
            if num.is_number() || num.is_string() {
                return num.clone();
            }
        }
    }
    v.clone()
}

fn compare_eq(lhs: Option<&JsonValue>, rhs: &str) -> bool {
    let Some(lhs) = lhs else {
        return false;
    };
    // String form: serialized scalar without surrounding quotes for
    // strings, decimal repr for numbers.
    if to_compare_string(lhs) == rhs {
        return true;
    }
    // Numeric form: 5 == 5.0 == "5".
    if let (Some(a), Some(b)) = (to_number(lhs), parse_number(rhs)) {
        if (a - b).abs() < f64::EPSILON {
            return true;
        }
    }
    false
}

fn compare_num(lhs: Option<&JsonValue>, rhs: &str, op: impl Fn(f64, f64) -> bool) -> bool {
    let Some(lhs) = lhs else {
        return false;
    };
    let (Some(a), Some(b)) = (to_number(lhs), parse_number(rhs)) else {
        return false;
    };
    op(a, b)
}

fn compare_regex(lhs: Option<&JsonValue>, rhs: &str) -> bool {
    let Some(lhs) = lhs else {
        return false;
    };
    // [`validate`] already rejected uncompilable patterns up front; the
    // no-match fallback keeps this path infallible regardless.
    let Ok(re) = Regex::new(regex_pattern(rhs)) else {
        return false;
    };
    re.is_match(&to_compare_string(lhs))
}

/// `/…/`-delimited values strip their slashes; bare values are the
/// pattern verbatim. Shared by [`validate`] and [`compare_regex`] so
/// both see the same pattern.
fn regex_pattern(rhs: &str) -> &str {
    rhs.strip_prefix('/')
        .and_then(|r| r.strip_suffix('/'))
        .unwrap_or(rhs)
}

fn to_compare_string(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn to_number(v: &JsonValue) -> Option<f64> {
    match v {
        JsonValue::Number(n) => n.as_f64(),
        JsonValue::String(s) => parse_number(s),
        _ => None,
    }
}

fn parse_number(s: &str) -> Option<f64> {
    // Bare numerics first.
    if let Ok(n) = s.parse::<f64>() {
        return Some(n);
    }
    // Duration literals → seconds, in the same dialect every other time
    // surface accepts (`dur=`, `--since`, `--window`): `4h`, `30m`,
    // `1:30`, `PT4H` all work without converting to seconds first.
    datum_core::time::parse_duration(s)
        .ok()
        .map(|d| d.num_seconds() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use datum_core::{
        dur::Dur,
        entry::{Entry, Verb},
        time::DatumTime,
    };
    use serde_json::json;

    fn t(secs: i64) -> DatumTime {
        DatumTime {
            utc: Utc.timestamp_opt(secs, 0).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        }
    }

    fn point(field: &str, state: JsonValue, data: Option<JsonValue>) -> Entry {
        Entry::new(
            field,
            Verb::Point,
            Some(state),
            data,
            Some(t(1000)),
            None,
            "d1",
        )
    }

    fn block(field: &str, state: JsonValue, dur_secs: i64) -> Entry {
        let dur = Dur::of(chrono::Duration::seconds(dur_secs));
        Entry::new(
            field,
            Verb::Block,
            Some(state),
            None,
            Some(t(1000)),
            Some(dur),
            "d1",
        )
    }

    fn m(key: &str, op: MatcherOp, value: &str) -> Matcher {
        Matcher {
            key: key.to_string(),
            op,
            value: value.to_string(),
        }
    }

    #[test]
    fn eq_matches_state_string() {
        let e = point("mood", json!("happy"), None);
        assert!(match_one(&e, &m("state", MatcherOp::Eq, "happy")));
        assert!(!match_one(&e, &m("state", MatcherOp::Eq, "sad")));
    }

    #[test]
    fn ne_inverts_eq() {
        let e = point("mood", json!("happy"), None);
        assert!(match_one(&e, &m("state", MatcherOp::Ne, "sad")));
        assert!(!match_one(&e, &m("state", MatcherOp::Ne, "happy")));
    }

    #[test]
    fn numeric_comparators_on_state() {
        let e = point("weight", json!(75.2), None);
        assert!(match_one(&e, &m("state", MatcherOp::Gt, "75")));
        assert!(!match_one(&e, &m("state", MatcherOp::Gt, "76")));
        assert!(match_one(&e, &m("state", MatcherOp::Gte, "75.2")));
        assert!(match_one(&e, &m("state", MatcherOp::Lt, "100")));
    }

    /// Quantitative comparators against `dur` accept ISO-8601 form on
    /// the RHS — `dur>PT4H` works without the user computing seconds.
    #[test]
    fn dur_compares_against_iso_8601() {
        let e = block("sleep", json!(true), 5 * 3600);
        assert!(match_one(&e, &m("dur", MatcherOp::Gt, "PT4H")));
        assert!(!match_one(&e, &m("dur", MatcherOp::Gt, "PT6H")));
        assert!(match_one(&e, &m("dur", MatcherOp::Gte, "PT5H")));
    }

    /// da-65fd: the RHS also accepts the CLI shorthand dialect every
    /// other time surface takes (`dur=`, `--since`): `4h`, `30m`,
    /// `1:30` — not just ISO-8601.
    #[test]
    fn dur_compares_against_cli_shorthand() {
        let e = block("sleep", json!(true), 5 * 3600);
        assert!(match_one(&e, &m("dur", MatcherOp::Gt, "4h")));
        assert!(!match_one(&e, &m("dur", MatcherOp::Gt, "6h")));
        assert!(match_one(&e, &m("dur", MatcherOp::Gte, "300m")));
        assert!(match_one(&e, &m("dur", MatcherOp::Gt, "4:30")));
        assert!(!match_one(&e, &m("dur", MatcherOp::Lt, "1:30")));
    }

    /// da-65fd: a regex that fails to compile is a user error surfaced
    /// by [`validate`] with the pattern named — not an empty result.
    #[test]
    fn validate_rejects_bad_regex_with_pattern_named() {
        let err = validate(&[m("state", MatcherOp::Regex, "x[")]).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid regex 'x['"),
            "pattern should be named in error: {msg}"
        );
        // Slash-delimited form strips slashes before validation, so the
        // named pattern matches what actually failed to compile.
        let err = validate(&[m("state", MatcherOp::Regex, "/y(/")]).unwrap_err();
        assert!(err.to_string().contains("invalid regex 'y('"));
        // Good patterns (and non-regex ops) pass.
        validate(&[
            m("state", MatcherOp::Regex, "^happy"),
            m("dur", MatcherOp::Gt, "4h"),
        ])
        .unwrap();
    }

    #[test]
    fn extra_data_dotted_path() {
        let e = point(
            "workout",
            json!(null),
            Some(json!({"session": {"mood": "tired"}})),
        );
        assert!(match_one(&e, &m("session.mood", MatcherOp::Eq, "tired")));
        assert!(!match_one(&e, &m("session.mood", MatcherOp::Eq, "happy")));
        assert!(!match_one(&e, &m("session.missing", MatcherOp::Eq, "x")));
    }

    /// Unit-annotated leaves (`{"v": 75.2, "u": "kg"}`) unwrap to the
    /// numeric for matcher purposes, so `weight>75` works without
    /// caring about the annotation form.
    #[test]
    fn unit_annotated_leaf_unwraps_for_numeric_compare() {
        let e = point(
            "weight",
            json!(null),
            Some(json!({"weight": {"v": 75.2, "u": "kg"}})),
        );
        assert!(match_one(&e, &m("weight", MatcherOp::Gt, "75")));
        assert!(!match_one(&e, &m("weight", MatcherOp::Gt, "80")));
    }

    #[test]
    fn regex_matches_with_slash_delimiters() {
        let e = point("mood", json!("happy"), None);
        assert!(match_one(
            &e,
            &m("state", MatcherOp::Regex, "/happy|content/")
        ));
        assert!(!match_one(&e, &m("state", MatcherOp::Regex, "/tired|sad/")));
    }

    #[test]
    fn regex_bare_pattern_also_works() {
        let e = point("mood", json!("happy_camper"), None);
        assert!(match_one(&e, &m("state", MatcherOp::Regex, "^happy")));
    }

    /// AND across multiple matchers — every predicate must hold.
    #[test]
    fn filter_applies_and_semantics() {
        let es = vec![
            point("sleep", json!(true), Some(json!({"hours": 8}))),
            point("sleep", json!(true), Some(json!({"hours": 5}))),
            point("sleep", json!(false), Some(json!({"hours": 8}))),
        ];
        let got = filter(
            es,
            &[
                m("state", MatcherOp::Eq, "true"),
                m("hours", MatcherOp::Gte, "7"),
            ],
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].data.as_ref().unwrap()["hours"], json!(8));
    }

    /// Numeric comparator against a non-numeric LHS fails the match.
    #[test]
    fn numeric_op_on_string_lhs_does_not_match() {
        let e = point("mood", json!("happy"), None);
        assert!(!match_one(&e, &m("state", MatcherOp::Gt, "5")));
    }
}
