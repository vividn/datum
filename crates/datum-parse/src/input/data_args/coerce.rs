//! Coercion phase: a raw `&str` token + classification context becomes
//! the most specific JSON [`Value`] we can.
//!
//! Layered, in evaluation order:
//! - empty / `undefined` → `None` (or `Err` when the slot is required)
//! - literal `null` → `Value::Null`
//! - literal (`rekey`'s new key names) → the string as typed
//! - a typed key — by the field's spec, or by suffix (`*time`, `*date`,
//!   `*dur`/`*duration`, `*num`/`*number`) — parses as its [`KeyType`]
//!   or is refused
//! - generic [`infer_type`] (booleans, numbers, JSON, comma arrays)

use chrono::NaiveDateTime;
use serde_json::Value;

use crate::error::{ParseError, Result};
use crate::input::data_args::Compat;
use crate::shape::{REF_MARK, STR_MARK};
use crate::time::date::unsigned_days_hint;
use crate::time::{
    format_iso_duration, is_whole_days, parse_date, parse_duration, parse_time, DateArg, DatumTime,
    DefaultDirection, TimeBuilder, TimeCtx, Zone,
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
    /// By suffix (`*time`): an instant, parsed as `-t` parses its value and
    /// stored as v2 stored it, `{utc, o, tz}` (da-x15of).
    Time,
    /// By suffix (`*dur`, `*duration`): stored as ISO 8601.
    Duration,
    /// A list of one element type (da-jhi0x): a comma always splits, a
    /// single value is a one-item list, and each element parses as its
    /// type or the whole value is refused.
    #[serde(rename = "number[]")]
    NumberList,
    #[serde(rename = "string[]")]
    StringList,
    #[serde(rename = "bool[]")]
    BoolList,
    /// Each element a slug, kept as typed.
    #[serde(rename = "ref[]")]
    RefList,
}

impl KeyType {
    /// The type a key's name gives it (`due_date`, `wake_time`), if any.
    pub fn of_suffix(path: &str) -> Option<Self> {
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
            (ty, v) if ty.is_list() => v.as_array().is_some_and(|items| {
                items.iter().all(|v| match ty.element() {
                    Some(el) => el.fits(v),
                    None => v.is_string(),
                })
            }),
            (Self::Number, v) => v.is_number(),
            (Self::Bool, v) => v.is_boolean(),
            (Self::Date, Value::String(s)) => day(s),
            // A string is the shape before da-x15of — a clock time that lost
            // its day — whether the type came from the key's name or a spec.
            // It does not fit, so `check` lists it for `reparse --fix`.
            (Self::Time, v) => DatumTime::from_v2_json(v).is_some(),
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
            Self::NumberList => "number[]",
            Self::StringList => "string[]",
            Self::BoolList => "bool[]",
            Self::RefList => "ref[]",
        }
    }

    fn is_list(self) -> bool {
        matches!(
            self,
            Self::NumberList | Self::StringList | Self::BoolList | Self::RefList
        )
    }

    /// A list's element type; `None` for `ref[]`, whose elements are text.
    fn element(self) -> Option<Self> {
        match self {
            Self::NumberList => Some(Self::Number),
            Self::StringList => Some(Self::String),
            Self::BoolList => Some(Self::Bool),
            _ => None,
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
    if compat.literal {
        // Quotes still escape: `a='"b c"'` names the key `b c`.
        let name = match json(value) {
            Some(Value::String(s)) => s,
            _ => value.to_string(),
        };
        return Ok(Some(Value::String(name)));
    }

    let state_key = match path {
        "state.id" => Some("state"),
        "following_state" => Some(path),
        _ => None,
    };
    if let Some(e) = state_key.and_then(|key| bracket_state_error(key, value)) {
        return Err(e);
    }

    let declared = compat.types.and_then(|t| t.get(path)).copied();
    let Some(ty) = declared.or_else(|| KeyType::of_suffix(path)) else {
        return Ok(infer_type(value));
    };
    if ty.is_list() {
        return match coerce_list(ty, value, compat) {
            Ok(items) => Ok(Some(Value::Array(items))),
            Err(hint) => Err(ParseError::WrongType {
                key: path.to_string(),
                ty: ty.name(),
                value: value.to_string(),
                hint,
            }),
        };
    }
    // da-johpu: the flags alone are a day and the key alone a shift, of the
    // midnight that day is stored as. So the two are asked as one line, the
    // key one more `-t`; quoted, it is read as one all the same.
    if let (true, Some(flags)) = (path == "occur_time", compat.time_flags) {
        let key = parse_time(value.trim_matches('"'), time_ctx(compat));
        if let (Ok(flags), Ok(key)) = (flags.builder_anchored(TimeCtx::now()), key) {
            flags.with_time_arg(key).shift_needs_clock()?;
        }
    }
    // da-s6ech: a full-day time takes a duration in whole days alone, by
    // the slot, `dur=` or an alias's `-k dur=`: `outside -D 3d` is three
    // days, and `start walk -y 13:00`, a mistyped `-t`, is no 13h day.
    if let (true, Some(flags)) = (path == "dur", compat.time_flags) {
        let only_date = flags
            .builder(TimeCtx::now())
            .is_ok_and(|b| b.is_only_date());
        if only_date && !flags.omit_timestamp && value != "null" && !is_whole_days(value) {
            return Err(ParseError::InvalidTime(format!(
                "a full-day time takes a duration in days; {value} is not"
            )));
        }
    }
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
/// resolve against `compat.base` (the entry's effective occur-time, in
/// its zone) when supplied — `wake_time=+30` with `-t 10:00` is 10:30 on
/// the entry's day. With no base anchor a date that parses but cannot be
/// resolved is kept as typed; a time or datetime resolves against the
/// wall clock.
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
            match resolve_date_arg(&d, base_local(compat)) {
                Some(day) => text(day.format("%Y-%m-%d").to_string()),
                None => infer_type(value),
            }
        }
        KeyType::Time => Some(instant(value, compat)?.0.to_v2_json()),
        // The same instant (da-iqzxa), stored as its wall clock in the
        // entry's zone, or in UTC when the value gave its own offset.
        KeyType::DateTime => {
            let (at, zoned) = instant(value, compat)?;
            text(if zoned {
                at.utc.format("%Y-%m-%dT%H:%M:%SZ").to_string()
            } else {
                let local = at.local_time().naive_local();
                local.format("%Y-%m-%dT%H:%M:%S").to_string()
            })
        }
        _ => coerce_list(ty, value, compat).ok().map(Value::Array),
    }
}

/// `value` as `-t` composes its own, from the entry's time in its zone: a
/// day word is that day's midnight, and an ISO instant is read in its own
/// offset, `Z` being UTC (da-kvosn).
/// With it, whether the value was an instant given with its offset.
fn instant(value: &str, compat: Compat) -> Option<(DatumTime, bool)> {
    let t = parse_time(value, time_ctx(compat)).ok()?;
    let zoned = t.iso_instant.is_some();
    let base = compat.base.cloned().or_else(wall_clock)?;
    let at = TimeBuilder::default()
        .with_time_arg(t)
        .build_with_uncertainty_at(base.utc, Zone::of(&base).ok()?, base.timezone);
    Some((at, zoned))
}

/// A list-typed value's elements (da-jhi0x), or the refusal's hint: the
/// first element that is not its element type, or a bracket value that is
/// not JSON (da-igesg) — `[a,b]` is never the fragments «[a» and «b]». The
/// bracket form gives its elements; otherwise the value is a list already,
/// as if marked with a leading comma: `8,8,6` and `8` split as `,8,8,6`
/// and `,8` would, and `a, b` is the one element «a, b».
fn coerce_list(
    ty: KeyType,
    value: &str,
    compat: Compat,
) -> std::result::Result<Vec<Value>, String> {
    let items: Vec<String> = match value.starts_with('[').then(|| json(value)) {
        Some(Some(Value::Array(a))) => a
            .into_iter()
            .map(|v| match v {
                Value::String(s) => s,
                v => v.to_string(),
            })
            .collect(),
        Some(_) => {
            // The example is in the key's own element type (da-80ox9).
            let (bracket, plain) = match ty {
                KeyType::NumberList => ("[8,6]", "8,6"),
                KeyType::BoolList => ("[true,false]", "true,false"),
                KeyType::RefList => (r#"["issue:a","issue:b"]"#, "issue:a,issue:b"),
                _ => (r#"["a","b"]"#, "a,b"),
            };
            return Err(format!(
                ": the bracket form is JSON: {bracket}, or write {plain}"
            ));
        }
        None => {
            let marked = if value.starts_with(',') {
                value.to_string()
            } else {
                format!(",{value}")
            };
            comma_items(&marked)
                .map(|items| items.into_iter().map(str::to_string).collect())
                .unwrap_or_else(|| vec![value.to_string()])
        }
    };
    items
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            match ty.element() {
                Some(el) => coerce_as(el, &item, compat),
                None => Some(Value::String(item.clone())),
            }
            .ok_or_else(|| {
                let el = ty.element().map_or("ref", KeyType::name);
                format!(": element {} {item:?} is not a {el}", i + 1)
            })
        })
        .collect()
}

/// What to say after a refusal: a decimal comma is a list (da-fwsre), and
/// the point is how a decimal is written; a day count takes a sign.
fn wrong_type_hint(ty: KeyType, value: &str) -> String {
    if ty == KeyType::Date {
        return unsigned_days_hint(value).map_or_else(String::new, |hint| format!(": {hint}"));
    }
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
        .anchored_at(base_local(compat))
        .facing(DefaultDirection::Nearest)
}

/// The entry's time as a wall clock in its own zone.
fn base_local(compat: Compat) -> Option<NaiveDateTime> {
    compat.base.map(|b| b.local_time().naive_local())
}

/// The wall clock, where there is one: what a `*_time` value resolves
/// against on a line with no entry time.
fn wall_clock() -> Option<DatumTime> {
    #[cfg(feature = "native")]
    return Some(DatumTime::now());
    #[cfg(not(feature = "native"))]
    None
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
/// string and the trailing comma (no dependency for them); values that aren't JSON in that sense
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

    // JSON, relaxed as v2 read it. A quoted `"ref://…"` is a literal,
    // which stores as `str://ref://…` (da-s5k2s).
    if let Some(v) = json(value) {
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

/// Strict JSON — numbers, arrays, objects, quoted strings — or the same
/// with single-quoted strings and a trailing comma in brackets, as v2's
/// relaxed-json read them (da-qzfnb, da-igesg).
fn json(value: &str) -> Option<Value> {
    serde_json::from_str(value)
        .ok()
        .or_else(|| relaxed_json(value))
}

/// `value` as JSON once each single-quoted string in it is double-quoted
/// and a trailing comma before `]` or `}` is dropped, as relaxed-json and
/// JSON5 read it: `'a,b'` → «a,b», `['a', 'b',]` → `["a","b"]`, `[8,]` →
/// `[8]`. Inside a string, `\'` is a quote and `"` needs no escape.
/// `None` when the result is not JSON (`'unbalanced`, `'`, `it's`, `[,]`).
fn relaxed_json(value: &str) -> Option<Value> {
    if !value.contains(['\'', ',']) {
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
            // A trailing comma, not one standing alone (`[,]`, `[8,,]`).
            (None, ',')
                if chars.as_str().trim_start().starts_with([']', '}'])
                    && !out.trim_end().ends_with(['[', '{', ',']) => {}
            _ => out.push(c),
        }
    }
    serde_json::from_str(&out).ok()
}

/// da-cktfm: a state in brackets that is not JSON (`[a,b]`) would be split
/// by the comma rule with its brackets kept, `["[a","b]"]` — no state
/// anyone meant, and one that hides a sentinel from da-z9efl's refusal.
/// Refused as a typed list key's bracket text is, in its words.
pub fn bracket_state_error(key: &str, value: &str) -> Option<ParseError> {
    // Any comma: a switch splits `[a, b]` too, where the comma rule would not.
    if !value.starts_with('[') || !value.contains(',') || json(value).is_some() {
        return None;
    }
    let items: Vec<&str> = value
        .trim_matches(['[', ']'])
        .split(',')
        .map(str::trim)
        .collect();
    Some(ParseError::WrongType {
        key: key.to_string(),
        ty: "list",
        value: value.to_string(),
        hint: format!(
            ": the bracket form is JSON: {}, or write {}",
            Value::from(items.clone()),
            items.join(",")
        ),
    })
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

    /// A stored list fits its list type when every element fits its
    /// element type (da-jhi0x); a lone value is not a list.
    #[test]
    fn a_list_fits_by_its_elements() {
        assert!(KeyType::NumberList.fits(&json!([8, 6])));
        assert!(KeyType::NumberList.fits(&json!([])));
        assert!(!KeyType::NumberList.fits(&json!(8)));
        assert!(!KeyType::NumberList.fits(&json!(["8"])));
        assert!(KeyType::StringList.fits(&json!(["a"])));
        assert!(!KeyType::BoolList.fits(&json!([true, "x"])));
        assert!(KeyType::RefList.fits(&json!(["issue:a"])));
        assert!(!KeyType::RefList.fits(&json!([1])));
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

    /// da-igesg: a trailing comma inside brackets is dropped, as relaxed-json
    /// and JSON5 read it, for every key; a comma standing alone is not one,
    /// and the value falls through to the comma rule.
    #[test]
    fn infer_type_trailing_comma_in_brackets() {
        for (typed, want) in [
            ("[8,]", json!([8])),
            ("[8, 6 , ]", json!([8, 6])),
            ("['a',]", json!(["a"])),
            (r#"{"k": 1,}"#, json!({"k": 1})),
            (r#"[["a",],]"#, json!([["a"]])),
            (r#"["a,]"]"#, json!(["a,]"])),
            ("[,]", json!(["[", "]"])),
            ("[8,,]", json!(["[8", null, "]"])),
            ("[a,b]", json!(["[a", "b]"])),
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
