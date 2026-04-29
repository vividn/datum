//! `dataArgs` — the core key/value/positional reconciliation from datum's
//! `src/input/dataArgs.ts`.
//!
//! Given a `DataArgs` bag (positional `data`, declared `keys`, comment(s),
//! remainder disposition, leniency) this module produces the final
//! [`DatumData`] object the command layer writes to an entry.
//!
//! # Non-ports
//!
//! * `-b`/`--base-data` (JSON seed). Low demand, explicitly dropped by the
//!   parity audit; add back later once the typed API lands.
//! * Command-change mid-parse escape (`dur=5 start` → switch verb).
//!   Daydumb dispatches verb-first; conflicts with the field/verb swap.
//! * Chrono-node natural-language parsing inside `alter_datum_data`'s
//!   time/date inference paths. Values that would require NL parsing
//!   stay as strings; structured forms (ISO date, `±N[unit]`, `+/-N` day
//!   offsets, `HH:MM`, `HHMM`, ISO duration, colon/named-unit duration)
//!   round-trip through the existing [`crate::time`] parsers.
//!
//! See `docs/parity-audit/parsing.md` ("`dataArgs.ts`") for the spec.

use serde_json::{Map, Value};

use crate::error::{ParseError, Result};
use crate::input::consolidate_keys::consolidate_keys;
use crate::input::flag_registry::{FlagAction, FlagSpec};
use crate::input::flexible_positional::FlexibleTarget;
use crate::time::{parse_date_arg, parse_duration, parse_time_arg};

/// Canonical flag set for the data concern.
///
/// `-c`/`--comment` is repeatable per datum's argparse shape (the TS code
/// joins multiple comments into a `comment: [...]` array); each occurrence
/// pushes onto `comment`. `-k` and `-K` likewise repeat — `-K` also tracks
/// the bare key name in `id_parts` so callers can render the slug template.
///
/// Positional `data` tokens (`key=value`, bare values) are NOT in this
/// registry — they're consumed by the parser's positional path because
/// they don't have a flag form.
pub static FLAG_SPECS: &[FlagSpec<DataArgs>] = &[
    FlagSpec {
        short: Some('k'),
        long: Some("key"),
        action: FlagAction::Value(|a, v| {
            a.keys.push(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('K'),
        long: Some("slug-key"),
        action: FlagAction::Value(|a, v| {
            a.push_id_key(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('c'),
        long: Some("comment"),
        action: FlagAction::Value(|a, v| {
            a.comment.push(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('R'),
        long: Some("remainder"),
        action: FlagAction::Value(|a, v| {
            a.remainder = Some(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('S'),
        long: Some("string-remainder"),
        action: FlagAction::Switch(|a| {
            a.string_remainder = true;
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('C'),
        long: Some("comment-remainder"),
        action: FlagAction::Switch(|a| {
            a.comment_remainder = true;
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('l'),
        long: Some("lenient"),
        action: FlagAction::Switch(|a| {
            a.lenient = true;
            Ok(())
        }),
    },
];

/// The untyped data payload produced by [`handle_data_args`] — the direct
/// analogue of datum's `DatumData` (a plain object).
pub type DatumData = Map<String, Value>;

/// Input to [`handle_data_args`]. Field names mirror the datum TS shape.
#[derive(Debug, Clone, Default)]
pub struct DataArgs {
    /// Positional data values, in user-supplied order. May contain
    /// interleaved `key=value` pairs.
    pub data: Vec<String>,
    /// `-k` / `-K` specs. Non-consolidated input is fine — the handler
    /// calls [`consolidate_keys`] internally.
    pub keys: Vec<String>,
    /// `-c`/`--comment`. Joined into the final doc's `comment` key in
    /// the order supplied (string → `comment: "x"`, many → `comment: [...]`).
    pub comment: Vec<String>,
    /// `-K` additions — when the user supplies `-K key`, the key name is
    /// pushed here in addition to being appended to `keys`. Callers that
    /// want the `%key%` slug token can read this list directly.
    pub id_parts: Vec<String>,
    /// `-R` / `--remainder` override for the remainder bucket key.
    pub remainder: Option<String>,
    /// `-S` / `--string-remainder`: join remainder as one space-separated
    /// string instead of an array.
    pub string_remainder: bool,
    /// `-C` / `--comment-remainder`: shorthand for `-SR comment`.
    pub comment_remainder: bool,
    /// `-l` / `--lenient`: when no `-R` is supplied, default the
    /// remainder bucket to `extraData` instead of raising.
    pub lenient: bool,
}

impl FlexibleTarget for DataArgs {
    fn data(&mut self) -> &mut Vec<String> {
        &mut self.data
    }
    fn keys(&mut self) -> &mut Vec<String> {
        &mut self.keys
    }
}

impl DataArgs {
    /// Record a `-K key` flag: append to `keys` and to `id_parts`. Mirrors
    /// the custom `IdKeyAction` in the TS argparse definition.
    pub fn push_id_key(&mut self, spec: String) {
        let key_name = spec.split_once('=').map(|(n, _)| n).unwrap_or(&spec);
        self.id_parts.push(format!("%{key_name}"));
        self.keys.push(spec);
    }
}

/// Resolve `DataArgs` into the final `DatumData` object.
///
/// See the TS `handleDataArgs` for the reference algorithm; the Rust
/// version differs only in that the excluded non-ports (`baseData`,
/// command-change escape) never surface.
pub fn handle_data_args(args: &DataArgs) -> Result<DatumData> {
    // Clone the bits we mutate; the input bag is treated as read-only.
    let mut data_queue: Vec<String> = args.data.clone();
    let mut keys: Vec<String> = consolidate_keys(&args.keys);

    let remainder_key: Option<String> = args.remainder.clone().or_else(|| {
        if args.comment_remainder {
            Some("comment".to_string())
        } else if args.lenient {
            Some("extraData".to_string())
        } else {
            None
        }
    });
    let remainder_as_string = args.string_remainder || args.comment_remainder;

    let mut datum_data: DatumData = Map::new();
    let mut remainder_data: Vec<String> = Vec::new();

    // ── positional loop ────────────────────────────────────────────────
    while !data_queue.is_empty() {
        let arg = data_queue.remove(0);
        let (before_eq, after_eq) = split_first_eq(&arg);

        if let Some(value) = after_eq {
            // Explicit key=value in the positional stream.
            let path = datum_path(before_eq);
            let existing = get_path(&datum_data, &path).cloned();
            // Find the first declared key matching this path so an
            // explicit `key=.` can fall back to its default.
            let default_value = if let Some(v) = existing {
                inferred_value_for_default(&v)
            } else {
                find_default_for_path(&keys, &path)
            };
            alter_datum_data(
                &mut datum_data,
                &path,
                Some(value),
                default_value.as_deref(),
                false, // append
                false, // isRequired — explicit key=... is never required
            )?;
            continue;
        }

        // Positional slot — try to consume the next unfilled key.
        let data_value = before_eq;
        let mut consumed = false;
        while !keys.is_empty() {
            let spec = keys.remove(0);
            let (raw_path, default_value) = split_first_eq(&spec);
            let is_required = default_value.is_none();
            let path = datum_path(raw_path);

            if get_path(&datum_data, &path).is_some() {
                // Already filled manually — skip to the next declared key.
                continue;
            }

            alter_datum_data(
                &mut datum_data,
                &path,
                Some(data_value),
                default_value,
                false,
                is_required,
            )?;
            consumed = true;
            break;
        }

        if !consumed {
            remainder_data.push(data_value.to_string());
        }
    }

    // ── unfilled keys: apply defaults; unfilled required → error ───────
    for spec in keys.drain(..) {
        let (raw_path, default_value) = split_first_eq(&spec);
        let path = datum_path(raw_path);
        let is_required = default_value.is_none();

        match get_path(&datum_data, &path) {
            Some(existing) if !is_dot_scalar(existing) => continue,
            _ => {}
        }
        alter_datum_data(
            &mut datum_data,
            &path,
            default_value,
            None,
            false,
            is_required,
        )?;
    }

    // ── comments ───────────────────────────────────────────────────────
    for comment in &args.comment {
        alter_datum_data(
            &mut datum_data,
            "comment",
            Some(comment),
            None,
            true,
            false,
        )?;
    }

    // ── remainder ──────────────────────────────────────────────────────
    if !remainder_data.is_empty() {
        let Some(ref rkey) = remainder_key else {
            return Err(ParseError::ExtraData(remainder_data));
        };

        if remainder_as_string {
            let joined = remainder_data.join(" ");
            alter_datum_data(
                &mut datum_data,
                rkey,
                Some(&joined),
                None,
                true,
                false,
            )?;
        } else {
            for rem in &remainder_data {
                alter_datum_data(
                    &mut datum_data,
                    rkey,
                    Some(rem),
                    None,
                    true,
                    false,
                )?;
            }
        }
    }

    Ok(datum_data)
}

// ─── path primitives ─────────────────────────────────────────────────────

/// Port of datum's `datumPath`. Maps `state` → `state.id` and any
/// leading-dot path (e.g. `.title`) → `state.title`.
fn datum_path(path: &str) -> String {
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
fn split_first_eq(s: &str) -> (&str, Option<&str>) {
    match s.split_once('=') {
        Some((h, t)) => (h, Some(t)),
        None => (s, None),
    }
}

/// Walk a dotted path through a `Map` and return a reference to the leaf.
fn get_path<'a>(obj: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let segs: Vec<&str> = path.split('.').collect();
    if segs.is_empty() {
        return None;
    }
    let mut cur = obj.get(segs[0])?;
    for seg in &segs[1..] {
        match cur {
            Value::Object(m) => {
                cur = m.get(*seg)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

/// Remove a dotted path from `obj`. No-op if the path doesn't exist.
fn remove_path(obj: &mut Map<String, Value>, path: &str) {
    let segs: Vec<&str> = path.split('.').collect();
    if segs.is_empty() {
        return;
    }
    if segs.len() == 1 {
        obj.remove(segs[0]);
        return;
    }
    let (last, parents) = segs.split_last().unwrap();
    let mut cur: &mut Map<String, Value> = obj;
    for seg in parents {
        match cur.get_mut(*seg) {
            Some(Value::Object(m)) => {
                cur = m;
            }
            _ => return,
        }
    }
    cur.remove(*last);
}

/// Set a dotted path to `value`. Scalars in the path are overwritten
/// with fresh objects to match lodash.set's semantics.
fn set_path(obj: &mut Map<String, Value>, path: &str, value: Value) {
    let segs: Vec<&str> = path.split('.').collect();
    if segs.is_empty() {
        return;
    }
    if segs.len() == 1 {
        obj.insert(segs[0].to_string(), value);
        return;
    }
    let (last, parents) = segs.split_last().unwrap();
    let mut cur: &mut Map<String, Value> = obj;
    for seg in parents {
        let entry = cur
            .entry((*seg).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        cur = entry.as_object_mut().unwrap();
    }
    cur.insert(last.to_string(), value);
}

/// Does the value at `existing` equal the literal string `"."`? Used to
/// treat a user-placed `"."` as "hasn't filled this key yet" when the
/// trailing key-default loop runs.
fn is_dot_scalar(v: &Value) -> bool {
    matches!(v, Value::String(s) if s == ".")
}

/// A default-from-existing-value: when an explicit `path=.` happens and
/// the value already exists in `datumData`, the TS code treats the
/// existing value as the default so `"."` round-trips to it.
fn inferred_value_for_default(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

/// Find the default value declared for `path` in a keys list.
fn find_default_for_path(keys: &[String], path: &str) -> Option<String> {
    keys.iter().find_map(|spec| {
        let (raw, default) = split_first_eq(spec);
        if datum_path(raw) == path {
            default.map(|s| s.to_string())
        } else {
            None
        }
    })
}

// ─── alter_datum_data ────────────────────────────────────────────────────

/// Port of datum's `alterDatumData`: infer the right JSON shape for
/// `value` (delegating to [`infer_type`] or the key-triggered time/date/
/// duration parsers) and set or append it at `path` inside `obj`.
fn alter_datum_data(
    obj: &mut Map<String, Value>,
    path: &str,
    value: Option<&str>,
    default_value: Option<&str>,
    append: bool,
    is_required: bool,
) -> Result<()> {
    let state_aware = datum_path(path);

    // `.` → recurse with the default.
    if matches!(value, Some(".")) {
        return alter_datum_data(obj, path, default_value, None, append, is_required);
    }

    let inferred = match value {
        Some(v) => infer_typed_value(&state_aware, v, is_required)?,
        None => {
            if is_required {
                return Err(ParseError::MissingRequiredKey(state_aware.clone()));
            }
            None
        }
    };

    if append {
        let Some(new_val) = inferred else {
            return Ok(());
        };
        let existing = get_path(obj, &state_aware).cloned();
        let combined = create_or_append(existing, new_val);
        set_path(obj, &state_aware, combined);
    } else {
        match inferred {
            Some(v) => set_path(obj, &state_aware, v),
            None => remove_path(obj, &state_aware),
        }
    }
    Ok(())
}

/// Infer the JSON shape for a value, applying the time/date/duration
/// key-pattern rules before falling back to the generic `infer_type`.
///
/// Returns `Ok(None)` for values the TS code would turn into `undefined`
/// (empty string, literal `"undefined"`). Returns `Err(MissingRequiredKey)`
/// when the slot is required and nothing was supplied.
fn infer_typed_value(path: &str, value: &str, is_required: bool) -> Result<Option<Value>> {
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

    // Time-like key → parse as time.
    if path_is_time(path) {
        if let Ok(t) = parse_time_arg(value) {
            if let Some(abs) = t.absolute {
                return Ok(Some(Value::String(abs.format("%H:%M:%S").to_string())));
            }
        }
        // Fallthrough: if we couldn't parse as time, leave as inferType's
        // usual handling. Datum would have errored; we accept the value
        // so users can still record free-form time fields.
    }

    // Date-like key → parse as date.
    if path_is_date(path) {
        if let Ok(d) = parse_date_arg(value) {
            if let Some(abs) = d.absolute {
                return Ok(Some(Value::String(abs.format("%Y-%m-%d").to_string())));
            }
        }
    }

    // Duration-like key → parse and emit ISO 8601.
    if path_is_duration(path) {
        if let Ok(dur) = parse_duration(value) {
            return Ok(Some(Value::String(format_iso_duration(dur))));
        }
    }

    Ok(infer_type(value))
}

fn path_is_time(path: &str) -> bool {
    matches_suffix(path, "time")
}

fn path_is_date(path: &str) -> bool {
    matches_suffix(path, "date")
}

fn path_is_duration(path: &str) -> bool {
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

// ─── infer_type ───────────────────────────────────────────────────────────

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

    // `",a,b"` or `"a,b,"` → array with leading/trailing sentinel comma.
    if let Some(rest) = value.strip_prefix(',') {
        return Some(Value::Array(
            rest.split(',')
                .map(|v| infer_type(v).unwrap_or(Value::Null))
                .collect(),
        ));
    }
    if let Some(rest) = value.strip_suffix(',') {
        return Some(Value::Array(
            rest.split(',')
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

fn create_or_append(existing: Option<Value>, to_append: Value) -> Value {
    match existing {
        None => to_append,
        Some(Value::Array(mut arr)) => {
            arr.push(to_append);
            Value::Array(arr)
        }
        Some(other) => Value::Array(vec![other, to_append]),
    }
}

// ─── tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args() -> DataArgs {
        DataArgs::default()
    }

    fn run(a: DataArgs) -> DatumData {
        handle_data_args(&a).expect("handle_data_args")
    }

    // ── infer_type ──────────────────────────────────────────────────────
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
    fn infer_type_falls_back_to_string() {
        assert_eq!(infer_type("hello"), Some(Value::String("hello".into())));
    }

    #[test]
    fn infer_type_quoted_empty_string() {
        assert_eq!(infer_type("''"), Some(Value::String(String::new())));
        assert_eq!(infer_type("\"\""), Some(Value::String(String::new())));
    }

    // ── direct ports of TS handleDataArgs tests ─────────────────────────

    #[test]
    fn empty_input_yields_empty_map() {
        assert_eq!(run(args()), Map::new());
    }

    #[test]
    fn explicit_key_value_pairs() {
        let mut a = args();
        a.data = vec!["abc=def".into()];
        assert_eq!(run(a), *json!({"abc": "def"}).as_object().unwrap());

        let mut a = args();
        a.data = vec!["first=arg".into(), "second=another".into()];
        assert_eq!(
            run(a),
            *json!({"first": "arg", "second": "another"}).as_object().unwrap()
        );

        let mut a = args();
        a.data = vec!["blank=".into()];
        assert_eq!(run(a), Map::new());
    }

    #[test]
    fn extra_equals_in_value_are_preserved() {
        let mut a = args();
        a.data = vec!["equation=1+2=3".into()];
        assert_eq!(
            run(a),
            *json!({"equation": "1+2=3"}).as_object().unwrap()
        );

        let mut a = args();
        a.data = vec!["eqSep=====".into()];
        assert_eq!(run(a), *json!({"eqSep": "===="}).as_object().unwrap());
    }

    #[test]
    fn extra_data_without_leniency_errors() {
        let mut a = args();
        a.data = vec!["keyless".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::ExtraData(_))
        ));

        let mut a = args();
        a.keys = vec!["key1".into()];
        a.data = vec!["hasKey".into(), "noKey".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::ExtraData(_))
        ));
    }

    #[test]
    fn lenient_collects_into_extra_data() {
        let mut a = args();
        a.lenient = true;
        a.data = vec!["keyless".into()];
        assert_eq!(
            run(a),
            *json!({"extraData": "keyless"}).as_object().unwrap()
        );

        let mut a = args();
        a.lenient = true;
        a.data = vec![
            "extra=Data".into(),
            "can".into(),
            "be=interspersed".into(),
            "with".into(),
            "keyed=data".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "extra": "Data",
                "be": "interspersed",
                "keyed": "data",
                "extraData": ["can", "with"],
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn required_keys_consumed_in_order() {
        let mut a = args();
        a.keys = vec!["abc".into()];
        a.data = vec!["value".into()];
        assert_eq!(run(a), *json!({"abc": "value"}).as_object().unwrap());

        let mut a = args();
        a.keys = vec!["a".into(), "b".into()];
        a.data = vec!["first".into(), "second".into()];
        assert_eq!(
            run(a),
            *json!({"a": "first", "b": "second"}).as_object().unwrap()
        );
    }

    #[test]
    fn missing_required_key_errors() {
        let mut a = args();
        a.keys = vec!["a".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));

        let mut a = args();
        a.keys = vec!["a".into(), "b".into()];
        a.data = vec!["onlyOne".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));

        let mut a = args();
        a.lenient = true;
        a.keys = vec!["a".into(), "b".into()];
        a.data = vec!["lenientDoesNotHelp".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));
    }

    #[test]
    fn explicit_key_assignment_satisfies_required() {
        let mut a = args();
        a.keys = vec!["abc".into()];
        a.data = vec!["abc=def".into()];
        assert_eq!(run(a), *json!({"abc": "def"}).as_object().unwrap());
    }

    #[test]
    fn manual_assignment_overwrites_positional() {
        let mut a = args();
        a.keys = vec!["abc".into()];
        a.data = vec!["def".into(), "abc=ghi".into()];
        assert_eq!(run(a), *json!({"abc": "ghi"}).as_object().unwrap());
    }

    #[test]
    fn future_positional_skips_manually_specified_key() {
        let mut a = args();
        a.keys = vec!["abc".into(), "def".into(), "ghi".into()];
        a.data = vec!["def=123".into(), "xyz".into(), "foobar".into()];
        assert_eq!(
            run(a),
            *json!({"abc": "xyz", "def": 123, "ghi": "foobar"})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn optional_keys_with_and_without_defaults() {
        let mut a = args();
        a.keys = vec!["abc=".into()];
        a.data = vec!["cde".into()];
        assert_eq!(run(a), *json!({"abc": "cde"}).as_object().unwrap());

        let mut a = args();
        a.keys = vec!["optional=".into()];
        assert_eq!(run(a), Map::new());

        let mut a = args();
        a.keys = vec!["withDefault=3".into()];
        assert_eq!(
            run(a),
            *json!({"withDefault": 3}).as_object().unwrap()
        );

        let mut a = args();
        a.keys = vec!["withBlankDefault=".into()];
        assert_eq!(run(a), Map::new());

        let mut a = args();
        a.keys = vec!["withEmptyStringDefault=''".into()];
        assert_eq!(
            run(a),
            *json!({"withEmptyStringDefault": ""}).as_object().unwrap()
        );

        let mut a = args();
        a.keys = vec!["withDefault=3".into()];
        a.data = vec!["replacement".into()];
        assert_eq!(
            run(a),
            *json!({"withDefault": "replacement"}).as_object().unwrap()
        );
    }

    #[test]
    fn explicit_assignment_replaces_default() {
        let mut a = args();
        a.lenient = true;
        a.keys = vec!["abc=".into()];
        a.data = vec!["abc=cde".into(), "ghi".into()];
        assert_eq!(
            run(a),
            *json!({"abc": "cde", "extraData": "ghi"})
                .as_object()
                .unwrap()
        );

        let mut a = args();
        a.keys = vec!["abc=123".into()];
        a.data = vec!["replacesAbc".into(), "abc=replacesAgain".into()];
        assert_eq!(
            run(a),
            *json!({"abc": "replacesAgain"}).as_object().unwrap()
        );
    }

    #[test]
    fn saves_comments_into_comment_key() {
        let mut a = args();
        a.comment = vec!["this is a comment.".into()];
        assert_eq!(
            run(a),
            *json!({"comment": "this is a comment."})
                .as_object()
                .unwrap()
        );

        let mut a = args();
        a.comment = vec!["one".into(), "two".into()];
        assert_eq!(
            run(a),
            *json!({"comment": ["one", "two"]}).as_object().unwrap()
        );
    }

    #[test]
    fn concatenates_arg_comment_with_data_comment() {
        let mut a = args();
        a.comment = vec!["argComment".into()];
        a.data = vec!["comment=dataComment".into()];
        assert_eq!(
            run(a),
            *json!({"comment": ["dataComment", "argComment"]})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn remainder_collects_extra_data() {
        let mut a = args();
        a.remainder = Some("rem".into());
        a.data = vec!["oneArgHasNoArray".into()];
        assert_eq!(
            run(a),
            *json!({"rem": "oneArgHasNoArray"}).as_object().unwrap()
        );

        let mut a = args();
        a.remainder = Some("rem".into());
        a.data = vec!["abc".into(), "otherKey=def".into(), "hij".into()];
        assert_eq!(
            run(a),
            *json!({"rem": ["abc", "hij"], "otherKey": "def"})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn string_remainder_joins_with_spaces() {
        let mut a = args();
        a.string_remainder = true;
        a.remainder = Some("rem".into());
        a.keys = vec!["firstKey=".into()];
        a.data = vec![
            "firstArg".into(),
            "Additional".into(),
            "arguments".into(),
            "get".into(),
            "made".into(),
            "into".into(),
            "a".into(),
            "single".into(),
            "string.".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "firstKey": "firstArg",
                "rem": "Additional arguments get made into a single string.",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn comment_remainder_joins_into_comment() {
        let mut a = args();
        a.comment_remainder = true;
        a.keys = vec!["optional1=".into()];
        a.data = vec![
            "the".into(),
            "non".into(),
            "filled".into(),
            "keys".into(),
            "will".into(),
            "key=value".into(),
            "be".into(),
            "a".into(),
            "comment.".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "optional1": "the",
                "key": "value",
                "comment": "non filled keys will be a comment.",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn comment_remainder_appends_to_existing_comments() {
        let mut a = args();
        a.comment_remainder = true;
        a.comment = vec!["explicit comment".into()];
        a.data = vec!["comment".into(), "remainder".into(), "string".into()];
        assert_eq!(
            run(a),
            *json!({"comment": ["explicit comment", "comment remainder string"]})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn interspersed_required_and_optional_keys() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "opt1=".into(),
            "req2".into(),
            "opt2=defaultValue".into(),
        ];
        a.data = vec![
            "value1".into(),
            "value2".into(),
            "value3".into(),
            "value4".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "req1": "value1",
                "opt1": "value2",
                "req2": "value3",
                "opt2": "value4",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn optional_later_provides_default_for_required() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "withDefaultFromOptional".into(),
            "opt1=".into(),
            "withDefaultFromOptional=noError".into(),
        ];
        a.data = vec!["onlyOneValue".into()];
        assert_eq!(
            run(a),
            *json!({
                "req1": "onlyOneValue",
                "withDefaultFromOptional": "noError",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn first_seen_position_preserved_even_when_later_spec_changes_form() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "withDefaultFromOptional".into(),
            "opt1=".into(),
            "withDefaultFromOptional=noError".into(),
        ];
        a.data = vec!["one".into(), "two".into(), "three".into()];
        assert_eq!(
            run(a),
            *json!({
                "req1": "one",
                "withDefaultFromOptional": "two",
                "opt1": "three",
            })
            .as_object()
            .unwrap()
        );

        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "withDefaultFromOptional".into(),
            "opt1=".into(),
            "withDefaultFromOptional=noError".into(),
        ];
        a.data = vec![
            "one".into(),
            "two".into(),
            "three".into(),
            "four".into(),
        ];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::ExtraData(_))
        ));
    }

    #[test]
    fn dot_default_on_optional_key() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "opt1=dotDefault".into(),
            "opt2=overwrittenDefault".into(),
            "opt3=notGivenDefault".into(),
        ];
        a.data = vec!["oneValue".into(), ".".into(), "threeValue".into()];
        assert_eq!(
            run(a),
            *json!({
                "req1": "oneValue",
                "opt1": "dotDefault",
                "opt2": "threeValue",
                "opt3": "notGivenDefault",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn explicit_dot_assignment_uses_default() {
        let mut a = args();
        a.keys = vec![
            "req1".into(),
            "opt1=overwrittenDefault".into(),
            "opt2=dotDefault".into(),
            "opt3=notGivenDefault".into(),
        ];
        a.data = vec![
            "opt2=.".into(),
            "twoValue".into(),
            "threeValue".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "req1": "twoValue",
                "opt1": "threeValue",
                "opt2": "dotDefault",
                "opt3": "notGivenDefault",
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn key_equals_deletes_previously_set_value() {
        let mut a = args();
        a.data = vec![
            "key=value".into(),
            "key2=value2".into(),
            "key=".into(),
        ];
        assert_eq!(
            run(a),
            *json!({"key2": "value2"}).as_object().unwrap()
        );
    }

    #[test]
    fn dot_uses_existing_value_when_reassigned() {
        let mut a = args();
        a.data = vec!["key=value".into(), "key=.".into()];
        assert_eq!(run(a), *json!({"key": "value"}).as_object().unwrap());

        let mut a = args();
        a.keys = vec!["req1".into()];
        a.data = vec!["someValue".into(), "req1=.".into()];
        assert_eq!(run(a), *json!({"req1": "someValue"}).as_object().unwrap());
    }

    #[test]
    fn pathlike_keys() {
        let mut a = args();
        a.keys = vec![
            "a.b".into(),
            "a.c".into(),
            "a.d.e=".into(),
            "another.key=5".into(),
        ];
        a.data = vec![
            "data".into(),
            "moreData".into(),
            "7".into(),
            "key.nested=abc".into(),
        ];
        assert_eq!(
            run(a),
            *json!({
                "a": {
                    "b": "data",
                    "c": "moreData",
                    "d": {"e": 7},
                },
                "key": {"nested": "abc"},
                "another": {"key": 5},
            })
            .as_object()
            .unwrap()
        );
    }

    #[test]
    fn top_level_value_gets_overwritten_when_path_nests() {
        let mut a = args();
        a.data = vec!["topKey=value".into(), "topKey.nestedKey=abc".into()];
        assert_eq!(
            run(a),
            *json!({"topKey": {"nestedKey": "abc"}}).as_object().unwrap()
        );
    }

    #[test]
    fn state_shorthand_routes_to_state_id() {
        let mut a = args();
        a.data = vec!["state=active".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "active"}}).as_object().unwrap()
        );

        let mut a = args();
        a.keys = vec!["state".into()];
        a.data = vec!["inactive".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "inactive"}}).as_object().unwrap()
        );
    }

    #[test]
    fn leading_dot_routes_to_state_subpath() {
        let mut a = args();
        a.data = vec![".key=active".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"key": "active"}}).as_object().unwrap()
        );
    }

    #[test]
    fn state_and_dot_compose_into_complex_state() {
        let mut a = args();
        a.keys = vec!["state=".into()];
        a.data = vec!["active".into(), ".type=common".into()];
        assert_eq!(
            run(a),
            *json!({"state": {"id": "active", "type": "common"}})
                .as_object()
                .unwrap()
        );
    }

    #[test]
    fn dot_cannot_satisfy_required_key() {
        let mut a = args();
        a.keys = vec!["req1".into(), "req2".into()];
        a.data = vec!["value1".into(), ".".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));
    }

    #[test]
    fn optional_can_be_upgraded_to_required() {
        let mut a = args();
        a.keys = vec![
            "opt1=".into(),
            "changedToReq=default".into(),
            "changedToReq".into(),
        ];
        a.data = vec!["value1".into()];
        assert!(matches!(
            handle_data_args(&a),
            Err(ParseError::MissingRequiredKey(_))
        ));
    }

    #[test]
    fn duration_key_infers_iso_duration() {
        let mut a = args();
        a.data = vec!["taskDuration=3h".into()];
        assert_eq!(
            run(a),
            *json!({"taskDuration": "PT3H"}).as_object().unwrap()
        );
    }

    #[test]
    fn duration_default_value_uses_key_inference() {
        let mut a = args();
        a.keys = vec!["duration=5".into()];
        assert_eq!(
            run(a),
            *json!({"duration": "PT5M"}).as_object().unwrap()
        );
    }

    #[test]
    fn input_args_are_not_mutated() {
        let mut a = args();
        a.keys = vec!["req1".into(), "opt1=".into()];
        a.remainder = Some("rem".into());
        a.comment = vec!["c".into()];
        a.data = vec!["v1".into(), "v2".into(), "v3".into()];
        let before = a.clone();
        let _ = handle_data_args(&a).unwrap();
        assert_eq!(before.keys, a.keys);
        assert_eq!(before.data, a.data);
        assert_eq!(before.comment, a.comment);
        assert_eq!(before.remainder, a.remainder);
    }

    // ── -K flag (id key) ────────────────────────────────────────────────

    #[test]
    fn push_id_key_mirrors_ts_id_key_action() {
        let mut a = DataArgs::default();
        a.push_id_key("field1".to_string());
        a.push_id_key("field2=default".to_string());
        assert_eq!(a.keys, vec!["field1", "field2=default"]);
        assert_eq!(a.id_parts, vec!["%field1", "%field2"]);
    }

    // ── path helpers ────────────────────────────────────────────────────

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

    // ── path traversal primitives ───────────────────────────────────────

    #[test]
    fn set_and_get_paths_round_trip() {
        let mut m = Map::new();
        set_path(&mut m, "a.b.c", json!(1));
        assert_eq!(get_path(&m, "a.b.c"), Some(&json!(1)));
        set_path(&mut m, "a.b.c", json!(2));
        assert_eq!(get_path(&m, "a.b.c"), Some(&json!(2)));
        set_path(&mut m, "a.b.d", json!("x"));
        assert_eq!(get_path(&m, "a.b.d"), Some(&json!("x")));
    }

    #[test]
    fn set_path_overwrites_scalar_with_object() {
        let mut m = Map::new();
        set_path(&mut m, "top", json!("primitive"));
        set_path(&mut m, "top.nested", json!("abc"));
        assert_eq!(get_path(&m, "top.nested"), Some(&json!("abc")));
    }

    #[test]
    fn remove_path_removes_leaf() {
        let mut m = Map::new();
        set_path(&mut m, "a.b.c", json!(1));
        remove_path(&mut m, "a.b.c");
        assert_eq!(get_path(&m, "a.b.c"), None);
    }
}
