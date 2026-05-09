//! Classification phase: shape decisions about a token or path before any
//! value parsing happens. Pure helpers — no allocation of payload state,
//! no error returns.
//!
//! Owners:
//! - path normalization (`state` / `.x` → `state.id` / `state.x`)
//! - dotted-path segmentation
//! - key-suffix predicates (`time` / `date` / `dur`uration) that drive
//!   coercion in [`super::coerce`]
//! - `-k`/`-K` declared-key default lookup
//! - dot-scalar / dot-default heuristics

use serde_json::Value;

/// Port of datum's `datumPath`. Maps `state` → `state.id` and any
/// leading-dot path (e.g. `.title`) → `state.title`.
pub(super) fn datum_path(path: &str) -> String {
    if path == "state" {
        "state.id".to_string()
    } else if let Some(rest) = path.strip_prefix('.') {
        format!("state.{rest}")
    } else {
        path.to_string()
    }
}

/// Split a string at the first `=`; the tail is `Some("")` for `k=` and
/// `None` when no `=` appears. Mirrors the TS helper `splitFirst("=", s)`.
pub(super) fn split_first_eq(s: &str) -> (&str, Option<&str>) {
    match s.split_once('=') {
        Some((h, t)) => (h, Some(t)),
        None => (s, None),
    }
}

/// Split a path into dotted segments.
pub(super) fn path_segs(path: &str) -> Vec<&str> {
    path.split('.').collect()
}

/// Does the value at `existing` equal the literal string `"."`? Used to
/// treat a user-placed `"."` as "hasn't filled this key yet" when the
/// trailing key-default loop runs.
pub(super) fn is_dot_scalar(v: &Value) -> bool {
    matches!(v, Value::String(s) if s == ".")
}

/// A default-from-existing-value: when an explicit `path=.` happens and
/// the value already exists in `datumData`, the TS code treats the
/// existing value as the default so `"."` round-trips to it.
pub(super) fn inferred_value_for_default(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

/// Find the default value declared for `path` in a keys list.
pub(super) fn find_default_for_path(keys: &[String], path: &str) -> Option<String> {
    keys.iter().find_map(|spec| {
        let (raw, default) = split_first_eq(spec);
        if datum_path(raw) == path {
            default.map(|s| s.to_string())
        } else {
            None
        }
    })
}

pub(super) fn path_is_time(path: &str) -> bool {
    matches_suffix(path, "time")
}

pub(super) fn path_is_date(path: &str) -> bool {
    matches_suffix(path, "date")
}

pub(super) fn path_is_duration(path: &str) -> bool {
    matches_suffix(path, "dur") || matches_suffix(path, "duration")
}

/// Case-insensitive suffix match following datum's TS regex semantics:
///
/// * `/(?:\b|_)<suffix>\d*$/i` — word boundary or underscore before
///   the suffix, optional trailing digits;
/// * `/[a-z0-9]<suffix-camel>\d*$/` — or a lowercase-letter/digit
///   immediately before a capitalised form.
///
/// We fold `path` to lowercase and strip trailing digits, then check the
/// boundary against `_` or the first char of the chunk.
fn matches_suffix(path: &str, suffix: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let trimmed = lower.trim_end_matches(|c: char| c.is_ascii_digit());
    if !trimmed.ends_with(suffix) {
        return false;
    }
    let head_len = trimmed.len() - suffix.len();
    if head_len == 0 {
        return true;
    }
    // The char just before the suffix must be (a) a word boundary
    // (underscore/non-alnum), (b) uppercase in the original (camelCase),
    // or (c) a digit — matching the union of the two JS regexes.
    let orig_chars: Vec<char> = path.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();
    let boundary = lower_chars[head_len - 1];
    let orig_at = orig_chars.get(head_len).copied().unwrap_or('\0');
    if boundary == '_' {
        return true;
    }
    if orig_at.is_ascii_uppercase() {
        return true;
    }
    // Word-boundary split at non-alnum: digits count, letters don't.
    !boundary.is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datum_path_rewrites() {
        assert_eq!(datum_path("plain"), "plain");
        assert_eq!(datum_path("state"), "state.id");
        assert_eq!(datum_path(".key"), "state.key");
        assert_eq!(datum_path(".deep.sub"), "state.deep.sub");
    }

    #[test]
    fn matches_suffix_handles_word_boundary_and_camel_case() {
        assert!(matches_suffix("time", "time"));
        assert!(matches_suffix("startTime", "time"));
        assert!(matches_suffix("start_time", "time"));
        assert!(matches_suffix("taskDuration", "duration"));
        assert!(matches_suffix("duration3", "duration"));
        assert!(matches_suffix("start_time2", "time"));
        // No word boundary inside "sometime" — the TS regex wouldn't match
        // either. Added to guard against an over-eager port.
        assert!(!matches_suffix("sometime", "time"));
        assert!(!matches_suffix("datetime", "time"));
        assert!(!matches_suffix("sometime", "date"));
    }
}
