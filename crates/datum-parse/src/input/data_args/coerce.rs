//! Coercion phase: a raw `&str` token + classification context becomes
//! the most specific JSON [`Value`] we can.
//!
//! Layered, in evaluation order:
//! - empty / `undefined` → `None` (or `Err` when the slot is required)
//! - literal `null` → `Value::Null`
//! - a typed key — by the field's spec, or by suffix (`*time`, `*date`,
//!   `*dur`/`*duration`, `*num`/`*number`) — parses as its [`KeyType`]
//!   or is refused
//! - generic [`infer_type`] (booleans, numbers, JSON, comma arrays)

use chrono::NaiveDateTime;
use serde_json::Value;

use crate::error::{ParseError, Result};
use crate::input::data_args::Compat;
use crate::shape::{REF_MARK, STR_MARK};
use crate::time::{
    format_iso_duration, parse_date, parse_duration, parse_time, DateArg, DefaultDirection, TimeCtx,
};

use super::classify::{path_is_date, path_is_duration, path_is_number, path_is_time};

/// A key's type (da-b2f8o): declared by the field's spec, or by the key's
/// suffix (`due_date`, `wake_time`, `run_dur`, `km_num`). A typed key's
/// value parses as its type or is refused — as v2 refused an unparseable
/// time or date — and an untyped key is guessed by [`infer_type`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    /// A decimal point, `_` between digits; a comma makes a list, refused.
    Number,
    /// The text as typed, never guessed: `007` stays `"007"`.
    String,
    /// `true` or `false`.
    Bool,
    /// A day, stored `YYYY-MM-DD`.
    Date,
    /// A day and a time, stored `YYYY-MM-DDTHH:MM:SS` (an instant given
    /// with its offset is stored in UTC, `…Z`).
    DateTime,
    /// By suffix (`*time`): a clock time `HH:MM:SS`, or a day as its midnight.
    Time,
    /// By suffix (`*dur`, `*duration`): stored as ISO 8601.
    Duration,
}

impl KeyType {
    fn of_suffix(path: &str) -> Option<Self> {
        [
            (path_is_time(path), Self::Time),
            (path_is_date(path), Self::Date),
            (path_is_duration(path), Self::Duration),
            (path_is_number(path), Self::Number),
        ]
        .into_iter()
        .find_map(|(hit, ty)| hit.then_some(ty))
    }

    /// Whether a stored value is one of this type, as logging it would
    /// have stored it — for `datum check`, since an entry another device
    /// wrote is merged unrefused. A null is no value, and fits.
    pub fn fits(self, v: &Value) -> bool {
        let day = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok();
        let at = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok();
        match (self, v) {
            (_, Value::Null) => true,
            (Self::Number, v) => v.is_number(),
            (Self::Bool, v) => v.is_boolean(),
            (Self::Date, Value::String(s)) => day(s),
            (Self::DateTime, Value::String(s)) => {
                at(s.strip_suffix('Z').unwrap_or(s))
                    || chrono::DateTime::parse_from_rfc3339(s).is_ok()
            }
            (_, Value::String(s)) => coerce_as(self, s, Compat::default()).is_some(),
            _ => false,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::String => "string",
            Self::Bool => "bool",
            Self::Date => "date",
            Self::DateTime => "datetime",
            Self::Time => "time",
            Self::Duration => "duration",
        }
    }
}

/// Infer the JSON shape for a value: as its key's [`KeyType`] when it has
/// one, else by the generic [`infer_type`].
///
/// Returns `Ok(None)` for values the TS code would turn into `undefined`
/// (empty string, literal `"undefined"`). Returns `Err(MissingRequiredKey)`
/// when the slot is required and nothing was supplied, and
/// `Err(WrongType)` when a typed key's value does not parse as its type.
pub(super) fn infer_typed_value(
    path: &str,
    value: &str,
    is_required: bool,
    compat: Compat,
) -> Result<Option<Value>> {
    // `undefined` / empty → undefined; error if required.
    if value.is_empty() || value.eq_ignore_ascii_case("undefined") {
        if is_required {
            return Err(ParseError::MissingRequiredKey(path.to_string()));
        }
        return Ok(None);
    }

    // Literal null pass-through: a JSON null, as in v2 (da-revva). The
    // string is `'"null"'`, the JSON-quote escape.
    if value.eq_ignore_ascii_case("null") {
        return Ok(Some(Value::Null));
    }

    let declared = compat.types.and_then(|t| t.get(path)).copied();
    let Some(ty) = declared.or_else(|| KeyType::of_suffix(path)) else {
        return Ok(infer_type(value));
    };
    match coerce_as(ty, value, compat) {
        Some(v) => Ok(Some(v)),
        // The entry's own time and duration keep their own parsers, and
        // their own refusals (`dur=1h±5m` is a duration with uncertainty).
        None if ENTRY_KEYS.contains(&path) => Ok(infer_type(value)),
        None => Err(ParseError::WrongType {
            key: path.to_string(),
            ty: ty.name(),
            value: value.to_string(),
            hint: wrong_type_hint(ty, value),
        }),
    }
}

/// The keys that place the entry rather than describe it.
const ENTRY_KEYS: [&str; 4] = ["dur", "occur_time", "start_time", "end_time"];

/// `value` as a `ty`, or `None` when it is not one.
///
/// Relative forms (`+30`, `-1h`) and day words (`sunday`, `yesterday`)
/// resolve against `compat.base_local` (the entry's effective occur-time
/// in its local zone) when supplied — `wake_time=+30` with `-t 10:00`
/// becomes `10:30:00`. With no base anchor a
/// time or date that parses but cannot be resolved is kept as typed; a
/// datetime, which names its own day, resolves against the wall clock.
/// Data args face `Nearest`: an unmarked `finish_time=sunday` means the
/// Sunday closest to the entry, not one a week out.
fn coerce_as(ty: KeyType, value: &str, compat: Compat) -> Option<Value> {
    let text = |s: String| Some(Value::String(s));
    match ty {
        KeyType::Number => infer_type(value).filter(Value::is_number),
        KeyType::Bool => infer_type(value).filter(Value::is_boolean),
        // A string is never a reference: `ref://x` is stored as its literal.
        KeyType::String if value.starts_with(REF_MARK) => text(format!("{STR_MARK}{value}")),
        KeyType::String => text(value.to_string()),
        KeyType::Duration => text(format_iso_duration(parse_duration(value).ok()?)),
        KeyType::Date => {
            // Relative day offsets (`nextDate=+3`) count from the entry's day.
            let d = parse_date(value, time_ctx(compat)).ok()?;
            match resolve_date_arg(&d, compat.base_local) {
                Some(day) => text(day.format("%Y-%m-%d").to_string()),
                None => infer_type(value),
            }
        }
        KeyType::Time => {
            let t = parse_time(value, time_ctx(compat)).ok()?;
            if let Some(abs) = t.absolute {
                return text(abs.format("%H:%M:%S").to_string());
            }
            if let (Some(offset), Some(base)) = (t.offset, compat.base_local) {
                return text((base + offset).time().format("%H:%M:%S").to_string());
            }
            // The value named a day rather than a clock (`finish_time=sunday`,
            // `next_time=yesterday`). It becomes that day's midnight, the same
            // full-datetime shape the plugin due-dates below already use.
            if let Some(day) = t.date.and_then(|d| resolve_date_arg(&d, compat.base_local)) {
                return text(day.format("%Y-%m-%dT00:00:00").to_string());
            }
            // ISO datetimes (`2026-05-03T00:00:00Z`, naive `2026-05-20T08:15:00`)
            // on a `*_time` key pass through as-is — the chores kit and similar
            // plugins store full-instant due-dates on `next_time`-style keys
            // and would be broken by truncating to time-of-day.
            infer_type(value)
        }
        KeyType::DateTime => {
            let ctx = time_ctx(compat);
            let t = parse_time(value, ctx).ok()?;
            if let Some(utc) = t.iso_instant {
                return text(utc.format("%Y-%m-%dT%H:%M:%SZ").to_string());
            }
            let base = ctx.reference;
            let at = t
                .iso_naive
                .or_else(|| match (t.offset, t.absolute, t.date) {
                    (Some(offset), _, _) => Some(base? + offset),
                    (_, Some(clock), _) => Some(base?.date().and_time(clock)),
                    (_, _, Some(d)) => resolve_date_arg(&d, base)?.and_hms_opt(0, 0, 0),
                    _ => None,
                })?;
            text(at.format("%Y-%m-%dT%H:%M:%S").to_string())
        }
    }
}

/// What to say after a refusal: a decimal comma is a list (da-fwsre), and
/// the point is how a decimal is written.
fn wrong_type_hint(ty: KeyType, value: &str) -> String {
    let typed_list = value.starts_with('[');
    if ty != KeyType::Number || typed_list || !infer_type(value).is_some_and(|v| v.is_array()) {
        return String::new();
    }
    let point = value.replace(',', ".");
    match infer_type(&point) {
        Some(Value::Number(_)) => format!(": a comma makes a list; a decimal is {point}"),
        _ => ": a comma makes a list".to_string(),
    }
}

/// What ambiguous data-arg values resolve against: the entry's own
/// occur-time when it has one, else the wall clock.
fn time_ctx(compat: Compat) -> TimeCtx {
    TimeCtx::now()
        .anchored_at(compat.base_local)
        .facing(DefaultDirection::Nearest)
}

/// The calendar day a parsed [`DateArg`] names. Pinned dates stand alone;
/// a relative day-offset (`+3`, `-7`, `0`, `yesterday`) needs a base, and
/// yields `None` without one.
fn resolve_date_arg(d: &DateArg, base: Option<NaiveDateTime>) -> Option<chrono::NaiveDate> {
    d.absolute
        .or_else(|| Some(base?.date() + chrono::Duration::days(d.days_offset)))
}

/// Port of datum's `inferType`: turn a raw string into the most specific
/// JSON shape we can. Of datum's relaxed-JSON we read the single-quoted
/// string (no dependency for it); values that aren't JSON in that sense
/// or one of the recognised literals fall through unchanged.
pub fn infer_type(value: &str) -> Option<Value> {
    if value.is_empty() {
        return None;
    }

    // The `\.` escape hatch: two literal characters `\` + `.` resolve to
    // the single-char string ".". Matches the TS regex `/^\\.$/`.
    if value == r"\." {
        return Some(Value::String(".".to_string()));
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

    // Strict JSON — numbers, arrays, objects, quoted strings — or the
    // same with single-quoted strings, as v2's relaxed-json read them
    // (da-qzfnb). A quoted `"ref://…"` is a literal, which stores as
    // `str://ref://…` (da-s5k2s).
    if let Some(v) = serde_json::from_str::<Value>(value)
        .ok()
        .or_else(|| single_quoted_json(value))
    {
        return Some(match v {
            Value::String(s) if s.starts_with(REF_MARK) => Value::String(format!("{STR_MARK}{s}")),
            v => v,
        });
    }

    // A comma makes a list (vividn, 10-07): a leading one marks it
    // (`,Anna` → `["Anna"]`), and one glued to what follows splits
    // (`a,b` → `["a","b"]`, `a,b, c` → `["a","b, c"]`), and a list drops
    // one trailing comma (`a,b,` → `["a","b"]`). A comma before whitespace
    // or at the end of a value that does not split is text (`met Anna,
    // Ben`, `Anna,`, `,`, `,,`); the bracket form, tried above, is the escape hatch.
    if let Some(items) = comma_items(value) {
        return Some(Value::Array(
            items
                .into_iter()
                .map(|v| infer_type(v).unwrap_or(Value::Null))
                .collect(),
        ));
    }

    // `_` between digits separates them, as in JavaScript and Rust:
    // `1_000` → 1000; `_1`, `1_` and `1__0` stay text.
    let b = value.as_bytes();
    let between_digits = |(i, _): (usize, &str)| {
        i > 0 && b[i - 1].is_ascii_digit() && b.get(i + 1).is_some_and(u8::is_ascii_digit)
    };
    if value.contains('_') && value.match_indices('_').all(between_digits) {
        if let Ok(n @ Value::Number(_)) = serde_json::from_str(&value.replace('_', "")) {
            return Some(n);
        }
    }

    Some(Value::String(value.to_string()))
}

/// `value` as JSON once each single-quoted string in it is double-quoted,
/// as relaxed-json and JSON5 read it: `'a,b'` → «a,b», `['a', 'b']` →
/// `["a","b"]`. Inside one, `\'` is a quote and `"` needs no escape.
/// `None` when the result is not JSON (`'unbalanced`, `'`, `it's`).
fn single_quoted_json(value: &str) -> Option<Value> {
    if !value.contains('\'') {
        return None;
    }
    let mut out = String::with_capacity(value.len() + 2);
    let mut quote = None;
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('\''), '\\') => match chars.next()? {
                '\'' => out.push('\''),
                e => out.extend(['\\', e]),
            },
            (Some(_), '\\') => out.extend(['\\', chars.next()?]),
            (Some('\''), '"') => out.push_str("\\\""),
            (Some(q), c) if c == q => {
                quote = None;
                out.push('"');
            }
            (None, '"' | '\'') => {
                quote = Some(c);
                out.push('"');
            }
            _ => out.push(c),
        }
    }
    serde_json::from_str(&out).ok()
}

/// The items of a comma list, or `None` when `value` is not one: split
/// after a leading comma and at every comma followed by something other
/// than whitespace, then drop one trailing comma from the last item.
fn comma_items(value: &str) -> Option<Vec<&str>> {
    // Nothing but commas has nothing to list: `,` and `,,` are text.
    if value.bytes().all(|b| b == b',') {
        return None;
    }
    let marked = value.strip_prefix(',');
    let rest = marked.unwrap_or(value);
    let mut items = Vec::new();
    let mut start = 0;
    for (i, _) in rest.match_indices(',') {
        if rest[i + 1..].starts_with(|c: char| !c.is_whitespace()) {
            items.push(&rest[start..i]);
            start = i + 1;
        }
    }
    if items.is_empty() && marked.is_none() {
        return None;
    }
    let last = &rest[start..];
    items.push(last.strip_suffix(',').unwrap_or(last));
    Some(items)
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

    /// The double a person typed is the double we keep — not the one a
    /// digit short (da-uru7). `0.23633156966490299` is the shortest
    /// round-trip spelling of its double; serde_json's default parser
    /// lands one ULP away on mantissas this long, so the workspace turns
    /// on its `float_roundtrip` feature. This test is what notices if
    /// that feature is ever dropped.
    #[test]
    fn infer_type_keeps_the_exact_double() {
        for spelling in ["0.23633156966490299", "0.6666666666666666", "1e-320"] {
            let want: f64 = spelling.parse().unwrap();
            let got = match infer_type(spelling) {
                Some(Value::Number(n)) => n.as_f64().unwrap(),
                other => panic!("{spelling}: expected a number, got {other:?}"),
            };
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "{spelling} inferred as {got}, a different double"
            );
        }
    }

    #[test]
    fn infer_type_arrays_and_objects() {
        assert_eq!(infer_type("[1,2,3]"), Some(json!([1, 2, 3])));
        assert_eq!(infer_type(r#"{"a":1}"#), Some(json!({"a": 1})));
    }

    /// The comma rule (da-fwsre, vividn 10-07): a glued comma splits, a
    /// leading one marks a list, comma + space and an edge or lone comma
    /// at the end are text, and the bracket form is the escape hatch.
    #[test]
    fn infer_type_comma_lists() {
        for (typed, want) in [
            ("a,b", json!(["a", "b"])),
            ("3,5", json!([3, 5])),
            ("a,1,true", json!(["a", 1, true])),
            ("met Anna, Ben", json!("met Anna, Ben")),
            (",Anna", json!(["Anna"])),
            (",a,b", json!(["a", "b"])),
            ("Anna,", json!("Anna,")),
            (",", json!(",")),
            ("[]", json!([])),
            (r#"["a", " b"]"#, json!(["a", " b"])),
            ("[3,5]", json!([3, 5])),
            (r#""a,b""#, json!("a,b")),
            // Only the glued comma splits; the item keeps its `, `.
            ("a,b, c", json!(["a", "b, c"])),
            // A list drops one trailing comma (vividn, 14:57); a word keeps it.
            ("a,b,", json!(["a", "b"])),
            ("3,5,", json!([3, 5])),
            (",tag1,", json!(["tag1"])),
            (",work,home,", json!(["work", "home"])),
            // Nothing but commas is text (vividn, 17:22).
            (",,", json!(",,")),
            (",,,", json!(",,,")),
            ("a,,", json!(["a", null])),
            // An empty item between glued commas is a null, as before.
            ("a,,b", json!(["a", null, "b"])),
            ("1_000,2", json!([1000, 2])),
        ] {
            assert_eq!(infer_type(typed), Some(want), "{typed}");
        }
    }

    /// `_` separates digits, as in JavaScript and Rust, and only digits.
    #[test]
    fn infer_type_digit_separators() {
        for (typed, want) in [
            ("1_000", json!(1000)),
            ("1_000.5", json!(1000.5)),
            ("-1_000_000", json!(-1_000_000)),
            ("_1", json!("_1")),
            ("1_", json!("1_")),
            ("1__000", json!("1__000")),
            ("1_.5", json!("1_.5")),
            ("a_1", json!("a_1")),
            ("1_a", json!("1_a")),
        ] {
            assert_eq!(infer_type(typed), Some(want), "{typed}");
        }
    }

    #[test]
    fn infer_type_falls_back_to_string() {
        assert_eq!(infer_type("hello"), Some(Value::String("hello".into())));
    }

    /// da-s5k2s: `ref://x` is stored as typed; both literal spellings store
    /// `str://ref://x`; a quoted plain string is untouched.
    #[test]
    fn infer_type_ref_marker_and_its_literal() {
        assert_eq!(infer_type("ref://a:b"), Some(json!("ref://a:b")));
        assert_eq!(
            infer_type("str://ref://a:b"),
            Some(json!("str://ref://a:b"))
        );
        assert_eq!(infer_type(r#""ref://a:b""#), Some(json!("str://ref://a:b")));
        assert_eq!(infer_type(r#""a:b""#), Some(json!("a:b")));
        assert_eq!(
            infer_type(r#",ref://a:b,"ref://c:d","#),
            Some(json!(["ref://a:b", "str://ref://c:d"]))
        );
    }

    /// da-qzfnb: a single-quoted string means what the double-quoted one
    /// does, as v2's relaxed-json read it, inside brackets too; a lone or
    /// unbalanced `'` is text.
    #[test]
    fn infer_type_single_quoted_strings() {
        for (typed, want) in [
            ("'a,b'", json!("a,b")),
            ("'5'", json!("5")),
            ("',,'", json!(",,")),
            (r"'it\'s'", json!("it's")),
            (r#"'say "hi"'"#, json!("say \"hi\"")),
            (r"'a\nb'", json!("a\nb")),
            ("['a', 'b']", json!(["a", "b"])),
            ("{'k': 'v'}", json!({"k": "v"})),
            (r#"['it"s', "it's"]"#, json!(["it\"s", "it's"])),
            ("'ref://a:b'", json!("str://ref://a:b")),
            ("'unbalanced", json!("'unbalanced")),
            ("'", json!("'")),
            ("it's", json!("it's")),
        ] {
            assert_eq!(infer_type(typed), Some(want), "{typed}");
        }
    }

    #[test]
    fn infer_type_quoted_empty_string() {
        assert_eq!(infer_type("''"), Some(Value::String(String::new())));
        assert_eq!(infer_type("\"\""), Some(Value::String(String::new())));
    }

    /// Digits followed by letters are a string, stored as typed — there
    /// is no number+unit inference (da-jh7io).
    #[test]
    fn digits_then_letters_stay_a_string() {
        for typed in [
            "4th", "5pm", "2x", "16gb", "3d", "6orhg", "75.2kg", "8291251c",
        ] {
            assert_eq!(infer_type(typed), Some(Value::String(typed.into())));
        }
    }
}
