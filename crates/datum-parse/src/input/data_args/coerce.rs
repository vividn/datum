//! Coercion phase: a raw `&str` token + classification context becomes
//! the most specific JSON [`Value`] we can.
//!
//! Layered, in evaluation order:
//! - empty / `undefined` → `None` (or `Err` when the slot is required)
//! - daydumb's `preserve_null_strings` keeps `"null"` / `"undefined"` as
//!   string values (switch verb compat)
//! - literal `null` → `Value::Null`
//! - path-suffix coercions (`*time`, `*date`, `*dur`/`*duration`) when
//!   classification flagged a temporal slot
//! - daydumb's unit-annotation extension (`75.2kg` → `{v, u}`)
//! - generic [`infer_type`] (booleans, numbers, JSON, comma arrays)

use serde_json::{Map, Value};

use crate::error::{ParseError, Result};
use crate::input::data_args::Compat;
use crate::time::{parse_date_arg, parse_duration, parse_time_arg};

use super::classify::{path_is_date, path_is_duration, path_is_time};

/// Infer the JSON shape for a value, applying the time/date/duration
/// key-pattern rules before falling back to the generic [`infer_type`].
///
/// Returns `Ok(None)` for values the TS code would turn into `undefined`
/// (empty string, literal `"undefined"`). Returns `Err(MissingRequiredKey)`
/// when the slot is required and nothing was supplied.
pub(super) fn infer_typed_value(
    path: &str,
    value: &str,
    is_required: bool,
    compat: Compat,
) -> Result<Option<Value>> {
    // Daydumb compat: keep the literal strings `"null"` / `"undefined"` as
    // string values regardless of path. Path-suffix coercion (below) is
    // bypassed too — daydumb's `extract_add_temporal_keys` already accepts
    // a string `"null"` for `dur=null`, so no signal is lost.
    if compat.preserve_null_strings
        && (value.eq_ignore_ascii_case("null") || value.eq_ignore_ascii_case("undefined"))
    {
        return Ok(Some(Value::String(value.to_string())));
    }

    // `undefined` / empty → undefined; error if required.
    if value.is_empty() || value.eq_ignore_ascii_case("undefined") {
        if is_required {
            return Err(ParseError::MissingRequiredKey(path.to_string()));
        }
        return Ok(None);
    }

    // Literal null pass-through.
    if value.eq_ignore_ascii_case("null") {
        return Ok(Some(Value::Null));
    }

    // Time-like key → parse as time. Relative forms (`+30`, `-1h`) resolve
    // against `compat.base_local` (the entry's effective occur-time in its
    // local zone) when supplied — `wake_time=+30` with `-t 10:00` becomes
    // `10:30:00`. With no base anchor (reparse / update payload) the
    // relative branches fall through to raw-string passthrough.
    if path_is_time(path) {
        if let Ok(t) = parse_time_arg(value) {
            if let Some(abs) = t.absolute {
                return Ok(Some(Value::String(abs.format("%H:%M:%S").to_string())));
            }
            // ISO datetimes (`2026-05-03T00:00:00Z`, naive `2026-05-20T08:15:00`)
            // on a `*_time` key pass through as-is — the chores kit and similar
            // plugins store full-instant due-dates on `next_time`-style keys
            // and would be broken by truncating to time-of-day. Users who want
            // just the time-of-day write `wake_time=08:15`.
            if let (Some(mins), Some(base)) = (t.minutes_offset, compat.base_local) {
                let resolved = base + chrono::Duration::minutes(mins);
                return Ok(Some(Value::String(
                    resolved.time().format("%H:%M:%S").to_string(),
                )));
            }
        }
        // Signed-duration token on a `*_time` key: `+1h`, `-30m`, `+90m`.
        // Gated to leading `+`/`-` (and non-pure-integer to avoid shadowing
        // the `minutes_offset` branch above) so bare `1h` keeps falling
        // through to the unit-annotation extension.
        if let Some(base) = compat.base_local {
            if (value.starts_with('+') || value.starts_with('-')) && !is_pure_signed_integer(value)
            {
                if let Ok(dur) = parse_duration(value) {
                    let resolved = base + dur;
                    return Ok(Some(Value::String(
                        resolved.time().format("%H:%M:%S").to_string(),
                    )));
                }
            }
        }
        // Fallthrough: if we couldn't parse as time, leave as inferType's
        // usual handling. Datum would have errored; we accept the value
        // so users can still record free-form time fields.
    }

    // Date-like key → parse as date. Relative day offsets (`nextDate=+3`)
    // resolve against `compat.base_local.date()` when supplied.
    if path_is_date(path) {
        if let Ok(d) = parse_date_arg(value) {
            if let Some(abs) = d.absolute {
                return Ok(Some(Value::String(abs.format("%Y-%m-%d").to_string())));
            }
            // Relative day-offset: `+3`, `-7`, `0`. parse_date_arg succeeds
            // for these with `absolute=None`; resolve against base date.
            if let Some(base) = compat.base_local {
                let resolved = base.date() + chrono::Duration::days(d.days_offset);
                return Ok(Some(Value::String(resolved.format("%Y-%m-%d").to_string())));
            }
        }
    }

    // Duration-like key → parse and emit ISO 8601.
    if path_is_duration(path) {
        if let Ok(dur) = parse_duration(value) {
            return Ok(Some(Value::String(format_iso_duration(dur))));
        }
    }

    // Unit-annotated number — daydumb extension over datum's `inferType`.
    // `75.2kg` / `500ml` → `{"v": <num>, "u": "<unit>"}`. Time/date/dur
    // suffix coercions above already won where they applied (e.g. `dur=5m`
    // → ISO duration, not unit-annotation).
    if let Some(v) = unit_annotated_value(value) {
        return Ok(Some(v));
    }

    Ok(infer_type(value))
}

/// daydumb extension: detect `75.2kg` / `500ml` style tokens and emit
/// `{"v": <num>, "u": "<unit>"}`. Returns `None` when the token isn't of
/// `<number><alpha-unit>` form. Public so the verb-specific switch-state
/// parser in `datum-commands` can apply the same coercion.
pub fn unit_annotated_value(value: &str) -> Option<Value> {
    let (num, unit) = split_numeric_unit(value)?;
    let n: f64 = num.parse().ok()?;
    let n_value = serde_json::Number::from_f64(n).map(Value::Number)?;
    let mut obj = Map::new();
    obj.insert("v".to_string(), n_value);
    obj.insert("u".to_string(), Value::String(unit.to_string()));
    Some(Value::Object(obj))
}

/// `+30` / `-8` / `7` style — leading sign optional, all-digits tail.
/// Used to guard the time-key signed-duration branch from shadowing the
/// `minutes_offset` branch above.
fn is_pure_signed_integer(s: &str) -> bool {
    let rest = s
        .strip_prefix('+')
        .or_else(|| s.strip_prefix('-'))
        .unwrap_or(s);
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

/// Split `"75.2kg"` → `Some(("75.2", "kg"))`, `"hello"` → `None`. The
/// numeric prefix may include digits, `.`, and a leading `-`; the unit
/// suffix must be all-alphabetic.
fn split_numeric_unit(s: &str) -> Option<(&str, &str)> {
    let idx = s
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == '-')
        .last()
        .map(|(i, _)| i + 1)?;
    if idx == 0 || idx == s.len() {
        return None;
    }
    let (num, unit) = s.split_at(idx);
    if unit.chars().all(|c| c.is_alphabetic()) {
        Some((num, unit))
    } else {
        None
    }
}

/// Resolve a free-form value with no path context — applies the
/// daydumb unit-annotation extension first, then datum's [`infer_type`].
/// Used by call sites (switch-state list expansion) that have a single
/// scalar token with no key path to dispatch on.
pub fn infer_value(value: &str) -> Option<Value> {
    if let Some(v) = unit_annotated_value(value) {
        return Some(v);
    }
    infer_type(value)
}

/// Format a `chrono::Duration` as an ISO 8601 string (`PT5M`, `PT1H30M`,
/// `P1DT5H`, etc.). Round-trips with [`crate::time::parse_duration`].
fn format_iso_duration(d: chrono::Duration) -> String {
    let total_secs = d.num_seconds();
    let (sign, mut secs) = if total_secs < 0 {
        ("-", -total_secs)
    } else {
        ("", total_secs)
    };
    let days = secs / 86400;
    secs %= 86400;
    let hours = secs / 3600;
    secs %= 3600;
    let minutes = secs / 60;
    secs %= 60;

    let mut out = String::new();
    if !sign.is_empty() {
        out.push('-');
    }
    out.push('P');
    let had_date = days > 0;
    if had_date {
        out.push_str(&format!("{days}D"));
    }
    let had_time = hours > 0 || minutes > 0 || secs > 0;
    if had_time || !had_date {
        out.push('T');
        if hours > 0 {
            out.push_str(&format!("{hours}H"));
        }
        if minutes > 0 {
            out.push_str(&format!("{minutes}M"));
        }
        if secs > 0 || (!had_date && hours == 0 && minutes == 0) {
            out.push_str(&format!("{secs}S"));
        }
    }
    out
}

/// Port of datum's `inferType`: turn a raw string into the most specific
/// JSON shape we can. Unlike datum we don't carry relaxed-JSON parsing
/// (that would pull in a fat dep for a rare branch); values that aren't
/// strict JSON or one of the recognised literals fall through unchanged.
pub fn infer_type(value: &str) -> Option<Value> {
    if value.is_empty() {
        return None;
    }

    // The `\.` escape hatch: two literal characters `\` + `.` resolve to
    // the single-char string ".". Matches the TS regex `/^\\.$/`.
    if value == r"\." {
        return Some(Value::String(".".to_string()));
    }

    // `","` alone → empty array.
    if value == "," {
        return Some(Value::Array(vec![]));
    }

    // Leading/trailing comma → array shorthand. Edge empties are tolerated
    // (transitional compat with old datum aliases that wrote `,tag1,`,
    // `,,`, etc.); interior empty splits keep their existing null behavior.
    if value.starts_with(',') || value.ends_with(',') {
        let parts: Vec<&str> = value.split(',').collect();
        let start = parts
            .iter()
            .position(|s| !s.is_empty())
            .unwrap_or(parts.len());
        let end = parts
            .iter()
            .rposition(|s| !s.is_empty())
            .map(|i| i + 1)
            .unwrap_or(start);
        return Some(Value::Array(
            parts[start..end]
                .iter()
                .map(|v| infer_type(v).unwrap_or(Value::Null))
                .collect(),
        ));
    }

    if value.eq_ignore_ascii_case("true") {
        return Some(Value::Bool(true));
    }
    if value.eq_ignore_ascii_case("false") {
        return Some(Value::Bool(false));
    }
    if value == "''" || value == "\"\"" {
        return Some(Value::String(String::new()));
    }
    if value.eq_ignore_ascii_case("undefined") {
        return None;
    }
    if value.eq_ignore_ascii_case("null") {
        return Some(Value::Null);
    }
    if value.eq_ignore_ascii_case("nan") {
        return Some(Value::String("NaN".to_string()));
    }
    if value.eq_ignore_ascii_case("infinity") || value.eq_ignore_ascii_case("inf") {
        return Some(Value::String("Infinity".to_string()));
    }
    if value.eq_ignore_ascii_case("-infinity") || value.eq_ignore_ascii_case("-inf") {
        return Some(Value::String("-Infinity".to_string()));
    }

    // Strict JSON — numbers, arrays, objects, quoted strings.
    if let Ok(v) = serde_json::from_str::<Value>(value) {
        return Some(v);
    }

    Some(Value::String(value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn infer_type_empty_is_undefined() {
        assert_eq!(infer_type(""), None);
    }

    #[test]
    fn infer_type_dot_escape_returns_dot_string() {
        assert_eq!(infer_type(r"\."), Some(Value::String(".".to_string())));
    }

    #[test]
    fn infer_type_booleans() {
        assert_eq!(infer_type("true"), Some(Value::Bool(true)));
        assert_eq!(infer_type("False"), Some(Value::Bool(false)));
    }

    #[test]
    fn infer_type_number() {
        assert_eq!(infer_type("42"), Some(json!(42)));
        assert_eq!(infer_type("3.5"), Some(json!(3.5)));
    }

    #[test]
    fn infer_type_arrays_and_objects() {
        assert_eq!(infer_type("[1,2,3]"), Some(json!([1, 2, 3])));
        assert_eq!(infer_type(r#"{"a":1}"#), Some(json!({"a": 1})));
    }

    #[test]
    fn infer_type_comma_shorthands() {
        assert_eq!(infer_type(","), Some(json!([])));
        assert_eq!(infer_type(",a,b"), Some(json!(["a", "b"])));
        assert_eq!(infer_type("a,b,"), Some(json!(["a", "b"])));
    }

    #[test]
    fn infer_type_comma_edge_tolerance() {
        // Leading + trailing edges may both be empty (da-lvvp).
        assert_eq!(infer_type(",tag1,tag2"), Some(json!(["tag1", "tag2"])));
        assert_eq!(infer_type("tag1,tag2,"), Some(json!(["tag1", "tag2"])));
        assert_eq!(infer_type(",tag1,"), Some(json!(["tag1"])));
        assert_eq!(infer_type(",,"), Some(json!([])));
        // Interior empty splits keep their existing null behavior.
        assert_eq!(
            infer_type(",tag1,,tag2,"),
            Some(json!(["tag1", null, "tag2"]))
        );
    }

    #[test]
    fn infer_type_falls_back_to_string() {
        assert_eq!(infer_type("hello"), Some(Value::String("hello".into())));
    }

    #[test]
    fn infer_type_quoted_empty_string() {
        assert_eq!(infer_type("''"), Some(Value::String(String::new())));
        assert_eq!(infer_type("\"\""), Some(Value::String(String::new())));
    }
}
