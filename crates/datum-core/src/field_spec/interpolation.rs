//! Template interpolation for entry slugs / id templates.
//!
//! Templates use `%key%` placeholders. `%field%`, `%hid%`, `%uid%`,
//! `%client_id%`, and the column-bound state/dur/following_state/occur_time
//! keys come from the [`InterpolationContext`]; everything else falls through
//! to a dot-path lookup against `extra_data`. `\%` is a literal `%`.
//!
//! `%hid%` is the entry's full hid, `%uid%` its first [`SHORT_HID_LEN`]
//! characters, and `%hid:N%` its first N (N ≥ [`SHORT_HID_LEN`]) — five
//! characters collide often enough at a few thousand entries that a field
//! keyed on them silently merges two (da-0ibow).
//!
//! A leading `?` (`%?key%`) marks the placeholder as optional. In an id
//! template a missing key renders as the empty string and the surrounding
//! delimiters collapse, whether or not it carries the `?`. A FIELD template
//! ([`interpolate_field`]) refuses a plain `%key%` that expands to nothing,
//! since that lands the entry in another field; the `?` is how a field
//! template says the key may be absent.
//!
//! A leading `-` (`%-key%`, `%-?key%`) filled a field's name only
//! (da-n1rcs). It is retired (da-vk747): a template given or declared with
//! one is refused ([`refuse_name_only_marker`]), and one already stored reads
//! as `%key%` does, so no name or slug moves.
//!
//! `%::key%` reads the field's own record (`get <field>::`) at that dotted
//! path, and `%::%` the record's value of the key the template is the value
//! of (da-3se7a). The caller hands the record in
//! ([`InterpolationContext::record`]); with none, they read nothing.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

use crate::entry::{Entry, SHORT_HID_LEN};
use crate::error::{DatumError, Result};
use crate::time::DatumTime;

/// Default delimiter used when `--id-delimiter` is not provided.
pub const DEFAULT_ID_DELIMITER: &str = ":";

// The slug's time tokens are rendered by `datum-parse`, which is where the
// browser reaches them too (da-7scm).
use datum_parse::entry_id;

/// Lookup context for [`interpolate`]. Keeps the helper decoupled from the
/// full [`Entry`] shape so it is easy to test and reuse.
pub struct InterpolationContext<'a> {
    pub field: &'a str,
    pub extra_data: Option<&'a JsonValue>,
    /// The entry's full hid. `%uid%` and `%hid:N%` are prefixes of it.
    pub hid: &'a str,
    /// da-5ns.4: column-bound state. `%state%` resolves from this first,
    /// falling back to `extra_data["state"]` for legacy entries that never
    /// migrated to the column form.
    pub state: Option<&'a JsonValue>,
    /// da-5ns.4: column-bound `dur`. `%dur%` reads this directly.
    /// Owned `String` because the source `Entry::dur` is now
    /// `Option<Dur>` (da-ju71.3); we render the storage form
    /// (`"PT5M"`, `"MOMENT"`, `"UNKNOWN"`) on the way in.
    pub dur: Option<String>,
    /// da-5ns.4: column-bound `following_state`. `%following_state%` reads
    /// this directly.
    pub following_state: Option<&'a JsonValue>,
    /// da-5ns.4: column-bound `occur_time`. `%occur_time%` formats this as
    /// `YYYY-MM-DDTHH:MM:SS.sssZ` (matching the default-slug builder),
    /// or as a bare local date when [`Self::occur_time_precision`] says
    /// the row is date-only.
    pub occur_time: Option<&'a DatumTime>,
    /// da-4fyb: the row's `occur_time_precision`. `Some("P1D")` marks a
    /// date-only entry (da-fc0l), whose `occur_time` was stripped to
    /// local midnight and carries no time-of-day. `%occur_time%` then
    /// renders the bare local date `YYYY-MM-DD` — the same degradation the
    /// display paths apply (da-5uph) — rather than a midnight stamp that
    /// asserts a precision the data does not have. Note the *local* date:
    /// the stored UTC instant is local midnight, so east-of-UTC rows land
    /// on the previous UTC day.
    pub occur_time_precision: Option<&'a str>,
    /// da-xbjy / da-r17h: `%create_time%` renders this and appends `'c'`
    /// (da-06jxa), so the slug for a timeless entry is
    /// `field:<create_time>c` and sorts beside the occur ids of its moment.
    pub create_time: Option<&'a DateTime<Utc>>,
    /// da-xbjy / da-r17h: `%modify_time%` renders this and auto-prepends `'m'`.
    pub modify_time: Option<&'a DateTime<Utc>>,
    /// da-z99wr: the writing client's id, the entry's `client_id`.
    /// `%client_id%` keys one record, or one field, per client. There is no
    /// name placeholder: a name can change, and a template is expanded when
    /// the entry is written, so a rename would split the history.
    pub client_id: &'a str,
    /// da-3se7a: the field's record, which `%::key%` reads; `None` reads
    /// nothing. Filled by a caller only for a template that reads it, with
    /// the keys it names ([`record_keys`]).
    pub record: Option<&'a JsonValue>,
    /// The key whose value the template is, which `%::%` reads in the record.
    pub record_key: Option<&'a str>,
}

impl<'a> InterpolationContext<'a> {
    /// Build an [`InterpolationContext`] from an [`Entry`].
    pub fn from_entry(entry: &'a Entry) -> Self {
        Self {
            field: &entry.field,
            extra_data: entry.data.as_ref(),
            hid: entry.hid.as_str(),
            state: entry.state.as_ref(),
            dur: entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
            following_state: entry.following_state.as_ref(),
            occur_time: entry.occur_time.as_ref(),
            occur_time_precision: entry.occur_uncertainty(),
            create_time: Some(&entry.create_time),
            modify_time: Some(&entry.modify_time),
            client_id: &entry.client_id,
            record: None,
            record_key: None,
        }
    }
}

/// True iff `s` contains at least one unescaped `%key%` placeholder. `\%`
/// counts as a literal percent and is ignored. Used to distinguish a
/// templated string (e.g. `"word__%srcLang%"`) from a literal one before
/// committing to the template-storage path.
pub fn contains_template(s: &str) -> bool {
    let mut chars = s.chars().peekable();
    let mut unescaped_percents = 0;
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'%') {
            chars.next();
            continue;
        }
        if c == '%' {
            unescaped_percents += 1;
            if unescaped_percents >= 2 {
                return true;
            }
        }
    }
    false
}

/// `template` without a leading `%field%` and the one separator after it,
/// or `None` when it does not start so (da-ipwqb). datum v2 kept the field
/// in its id template; v3's slug prefixes it implicitly, so a v2 template
/// carried over as-is names the field twice. A `%field%` anywhere else may
/// be deliberate and is not this function's to remove.
pub fn strip_field_prefix(template: &str) -> Option<&str> {
    let rest = template.strip_prefix("%field%")?;
    let sep = rest.chars().next()?;
    let body = &rest[sep.len_utf8()..];
    (!sep.is_alphanumeric() && !matches!(sep, '%' | '\\') && !body.is_empty()).then_some(body)
}

/// Interpolate a template like `"%field%:%uid%"` into a string using the
/// lookup context. Unknown keys resolve to the empty string; runs of
/// `delimiter` produced by empty substitutions collapse to a single delimiter
/// (leading/trailing delimiters are also stripped). `\%` is a literal `%`.
pub fn interpolate(template: &str, ctx: &InterpolationContext, delimiter: &str) -> String {
    let parts = split_raw_and_fields(template);
    let mut out = String::new();
    for (idx, part) in parts.iter().enumerate() {
        if idx % 2 == 0 {
            out.push_str(part);
        } else {
            let val = lookup_field(part, ctx);
            out.push_str(&val);
        }
    }
    collapse_delimiters(&out, delimiter)
}

/// [`interpolate`] for a FIELD-name template (da-0ibow). A plain `%key%`
/// that expands to nothing — an unknown name, a typo, an absent data key —
/// would otherwise drop out of the name and land the entry in a different
/// field, so it refuses, naming the key. `%?key%` may still be empty.
pub fn interpolate_field(
    template: &str,
    ctx: &InterpolationContext,
    delimiter: &str,
) -> Result<String> {
    refuse_name_only_marker(template, Some("-F"))?;
    let empty = unfilled(template, ctx);
    if !empty.is_empty() {
        return Err(DatumError::Other(format!(
            "field template '{template}': {} expands to nothing, which would \
             put the entry in another field. Give the key a value, or mark it \
             optional with %?key%.",
            empty.join(", ")
        )));
    }
    Ok(interpolate(template, ctx, delimiter))
}

/// A template's `%-key%`, retired (da-vk747), refused by name. `flag` is
/// the line's way to a name part from a value (`-F`); with none, a plain
/// `%key%` is.
pub fn refuse_name_only_marker(template: &str, flag: Option<&str>) -> Result<()> {
    let Some(k) = split_raw_and_fields(template)
        .into_iter()
        .skip(1)
        .step_by(2)
        .find(|k| markers(k).contains('-'))
    else {
        return Ok(());
    };
    let key = canonical_key(&k);
    let instead = match flag {
        Some(f) => format!("A name part from a value is `{f} {key}`, which stores no {key}."),
        None => format!("Write %{key}%."),
    };
    Err(DatumError::Other(format!(
        "'{template}': %{k}% is retired. {instead}"
    )))
}

/// A placeholder's leading markers: `?` optional, `-` the retired name only.
fn markers(key: &str) -> &str {
    &key[..key.len() - key.trim_start_matches(['?', '-']).len()]
}

fn is_optional(key: &str) -> bool {
    markers(key).contains('?')
}

/// `template`'s plain placeholders that expand to nothing in `ctx`, each as
/// `%key%`: what a template that must fill refuses (`%?key%` may be empty).
pub fn unfilled(template: &str, ctx: &InterpolationContext) -> Vec<String> {
    split_raw_and_fields(template)
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|k| !k.is_empty() && !is_optional(k) && lookup_field(k, ctx).is_empty())
        .map(|k| format!("%{k}%"))
        .collect()
}

fn placeholders(template: &str) -> impl Iterator<Item = String> {
    split_raw_and_fields(template)
        .into_iter()
        .skip(1)
        .step_by(2)
}

fn split_raw_and_fields(template: &str) -> Vec<String> {
    // Replace unescaped `%` with NUL then split. `\%` → literal `%`.
    let mut buf = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'%') {
            chars.next();
            buf.push('%');
        } else if c == '%' {
            buf.push('\x00');
        } else {
            buf.push(c);
        }
    }
    buf.split('\x00').map(String::from).collect()
}

/// The name a placeholder's key resolves by: its markers stripped, and a
/// legacy camelCase time name mapped to its snake_case form.
fn canonical_key(key: &str) -> &str {
    let key = key.trim_start_matches(['?', '-']);
    // da-k2xv: legacy camelCase time metadata names (occurTime / createTime
    // / modifyTime) resolve to their snake_case counterparts so id-templates
    // ported verbatim from the original datum (which used camelCase) keep
    // working. Other camelCase keys fall through to lookup_extra_data
    // unchanged.
    match key {
        "occurTime" => "occur_time",
        "createTime" => "create_time",
        "modifyTime" => "modify_time",
        // following_state and dur have no legacy camelCase form (they're
        // post-v3 additions); pass through unchanged.
        other => other,
    }
}

/// True iff `template` has a placeholder for one of the entry's times
/// (`occur_time` / `create_time` / `modify_time`, in any spelling
/// [`lookup_field`] resolves). A template without one is a KEY: two
/// entries it renders alike are one record, not two events (da-b5k87).
pub fn names_time(template: &str) -> bool {
    split_raw_and_fields(template)
        .iter()
        .skip(1)
        .step_by(2)
        .any(|k| {
            matches!(
                canonical_key(k),
                "occur_time" | "create_time" | "modify_time"
            )
        })
}

fn lookup_field(key: &str, ctx: &InterpolationContext) -> String {
    let key = canonical_key(key);
    match key {
        "" => String::new(),
        "field" => ctx.field.to_string(),
        "hid" => ctx.hid.to_string(),
        "uid" => ctx.hid.chars().take(SHORT_HID_LEN).collect(),
        "client_id" => ctx.client_id.to_string(),
        "state" => {
            // da-5ns.4: column-bound state wins. The column is canonical
            // post-pivot; the data-blob path is kept as a fallback for
            // legacy/in-flight entries that haven't been canonicalized yet.
            if let Some(s) = ctx.state {
                return json_to_plain_string(s);
            }
            lookup_extra_data(ctx.extra_data, "state").unwrap_or_default()
        }
        "dur" => ctx.dur.clone().unwrap_or_default(),
        "following_state" => ctx
            .following_state
            .map(json_to_plain_string)
            .unwrap_or_default(),
        // da-4fyb: a date-only row (`P1D`) has no time-of-day, so the slug
        // it keys on is the bare local date. Anything else keeps the full
        // UTC stamp the default-slug builder emits.
        "occur_time" => ctx
            .occur_time
            .map(|t| entry_id::render_occur_token(t, ctx.occur_time_precision))
            .unwrap_or_default(),
        // da-xbjy / da-r17h: mark the time `c` / `m`, only when the value is
        // non-empty so missing-key delimiter collapsing still works. The `c`
        // goes after the time (da-06jxa, vividn: «the c should go after so
        // that the -T documents are in chronological order with their occur
        // counterparts»): `field:<create_time>c` sorts right after
        // `field:<occur_time>` of the same instant.
        "create_time" => ctx
            .create_time
            .map(|t| format!("{}c", entry_id::render_stamp(t)))
            .unwrap_or_default(),
        "modify_time" => prefix_when_present(ctx.modify_time.map(entry_id::render_stamp), 'm'),
        _ if key.starts_with("::") => record_value(key, ctx)
            .map(json_to_plain_string)
            .unwrap_or_default(),
        _ => match key.strip_prefix("hid:").map(str::parse::<usize>) {
            Some(Ok(n)) if n >= SHORT_HID_LEN => ctx.hid.chars().take(n).collect(),
            Some(_) => String::new(),
            None => lookup_extra_data(ctx.extra_data, key).unwrap_or_default(),
        },
    }
}

fn prefix_when_present(rendered: Option<String>, marker: char) -> String {
    match rendered {
        Some(s) if !s.is_empty() => {
            let mut out = String::with_capacity(s.len() + 1);
            out.push(marker);
            out.push_str(&s);
            out
        }
        _ => String::new(),
    }
}

/// Walk `extra_data` using dot-path `key` (e.g. `"a.b"`, `"comment.0"`).
/// A segment names an object's key or an array's index, `-1` the last
/// element (da-7nv8r). Returns None if any segment is missing.
fn lookup_extra_data(extra: Option<&JsonValue>, key: &str) -> Option<String> {
    // da-ult68: one comment slugs as itself, so `--id %comment%` still does.
    walk(extra, key).map(|v| json_to_plain_string(crate::entry::shown_data_value(key, v)))
}

fn walk<'v>(extra: Option<&'v JsonValue>, key: &str) -> Option<&'v JsonValue> {
    let mut cur = extra?;
    for segment in key.split('.') {
        cur = match cur {
            JsonValue::Object(m) => m.get(segment)?,
            JsonValue::Array(a) => match segment.parse::<isize>().ok()? {
                i if i < 0 => a.get(a.len().checked_sub(i.unsigned_abs())?)?,
                i => a.get(i.unsigned_abs())?,
            },
            _ => return None,
        };
    }
    Some(cur)
}

/// What `::path` (a canonical key) reads in the record: `::` alone the
/// template's own key. Null is nothing.
fn record_value<'v>(key: &str, ctx: &InterpolationContext<'v>) -> Option<&'v JsonValue> {
    let path = match &key[2..] {
        "" => ctx.record_key?,
        path => path,
    };
    walk(ctx.record, path).filter(|v| !v.is_null())
}

/// True iff `template` has a `%::key%` placeholder: a value read from the
/// field's record.
pub fn reads_record(template: &str) -> bool {
    placeholders(template).any(|k| canonical_key(&k).starts_with("::"))
}

/// The record's keys `template` reads, each once: `%::a.b%` reads `a`, and
/// `%::%` `own`, the key the template is the value of.
pub fn record_keys(template: &str, own: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for k in placeholders(template) {
        let key = match canonical_key(&k).strip_prefix("::") {
            Some("") => own,
            Some(path) => path.split('.').next().unwrap_or(path),
            None => continue,
        };
        if !keys.iter().any(|k| k == key) {
            keys.push(key.into());
        }
    }
    keys
}

/// A value template that reads the record (da-3se7a), resolved against
/// `ctx`: one `%::key%` alone is the record's value as it is, a list a list;
/// anything else the text with each placeholder's. `None` for an optional
/// one alone that reads nothing; a plain one that does refuses, naming it.
pub fn copy_from_record(template: &str, ctx: &InterpolationContext) -> Result<Option<JsonValue>> {
    let parts = split_raw_and_fields(template);
    if let [before, key, after] = parts.as_slice() {
        if before.is_empty() && after.is_empty() && canonical_key(key).starts_with("::") {
            return match record_value(canonical_key(key), ctx) {
                Some(v) => Ok(Some(v.clone())),
                None if is_optional(key) => Ok(None),
                None => Err(no_record_value(key, ctx)),
            };
        }
    }
    let missing = |k: &&String| {
        let key = canonical_key(k);
        key.starts_with("::") && !is_optional(k) && record_value(key, ctx).is_none()
    };
    if let Some(k) = parts.iter().skip(1).step_by(2).find(missing) {
        return Err(no_record_value(k, ctx));
    }
    let mut out = String::new();
    for (idx, part) in parts.iter().enumerate() {
        match idx % 2 {
            0 => out.push_str(part),
            _ => out.push_str(&lookup_field(part, ctx)),
        }
    }
    Ok(Some(out.into()))
}

fn no_record_value(key: &str, ctx: &InterpolationContext) -> DatumError {
    let path = match canonical_key(key) {
        "::" => ctx.record_key.unwrap_or_default(),
        k => k.trim_start_matches(':'),
    };
    DatumError::Other(format!(
        "%{key}%: {}'s record holds no {path}, so nothing is written. \
         Mark it optional with %?{}%.",
        ctx.field,
        canonical_key(key)
    ))
}

fn json_to_plain_string(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        JsonValue::Null => String::new(),
        other => relaxed_json(other),
    }
}

/// An array or object as it reads in an id (da-7nv8r): JSON without the
/// spaces, and without the quotes around a key or string unless it is empty
/// or holds one of `,[]{}:` — `[electricity,paid]`, `{a:1,b:x}`.
fn relaxed_json(v: &JsonValue) -> String {
    let text = |s: &str| {
        if s.is_empty() || s.contains([',', '[', ']', '{', '}', ':']) {
            JsonValue::from(s).to_string()
        } else {
            s.to_string()
        }
    };
    match v {
        JsonValue::String(s) => text(s),
        JsonValue::Array(a) => {
            let items: Vec<String> = a.iter().map(relaxed_json).collect();
            format!("[{}]", items.join(","))
        }
        JsonValue::Object(m) => {
            let items: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}:{}", text(k), relaxed_json(v)))
                .collect();
            format!("{{{}}}", items.join(","))
        }
        other => other.to_string(),
    }
}

fn collapse_delimiters(s: &str, delim: &str) -> String {
    let mut result = s.to_string();
    // da-ivld: `__` is the canonical slug-key separator emitted by the
    // `-K`-derived id_structure (`%key1%__%key2%`). Collapse it
    // unconditionally so an absent slot drops cleanly without leaving
    // stranded `__` runs or a trailing/leading tail — independent of
    // whatever `delim` was passed in.
    const SLUG_SEP: &str = "__";
    let slug_sep_double = format!("{SLUG_SEP}{SLUG_SEP}");
    while result.contains(&slug_sep_double) {
        result = result.replace(&slug_sep_double, SLUG_SEP);
    }
    if let Some(stripped) = result.strip_prefix(SLUG_SEP) {
        result = stripped.to_string();
    }
    if let Some(stripped) = result.strip_suffix(SLUG_SEP) {
        result = stripped.to_string();
    }
    if delim.is_empty() || delim == SLUG_SEP {
        return result;
    }
    let double = format!("{delim}{delim}");
    while result.contains(&double) {
        result = result.replace(&double, delim);
    }
    if let Some(stripped) = result.strip_prefix(delim) {
        result = stripped.to_string();
    }
    if let Some(stripped) = result.strip_suffix(delim) {
        result = stripped.to_string();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occur_ctx<'a>(
        field: &'a str,
        extra: Option<&'a JsonValue>,
        hid: &'a str,
    ) -> InterpolationContext<'a> {
        InterpolationContext {
            field,
            extra_data: extra,
            hid,
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
            occur_time_precision: None,
            create_time: None,
            modify_time: None,
            client_id: "",
            record: None,
            record_key: None,
        }
    }

    #[test]
    fn interpolate_field_uid_template() {
        let ctx = occur_ctx("coffee", None, "abc12");
        let got = interpolate("%field%:%uid%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:abc12");
    }

    /// da-ult68: one comment slugs as itself; two stay a list.
    #[test]
    fn interpolate_comment_array() {
        let one = serde_json::json!({"comment": ["electricity"]});
        let two = serde_json::json!({"comment": ["a", "b"]});
        let t = |d| interpolate("%comment%", &occur_ctx("tx", Some(d), "h"), "_");
        assert_eq!(t(&one), "electricity");
        assert_eq!(t(&two), "[a,b]");
    }

    /// da-7nv8r: a non-scalar slugs as relaxed JSON — no spaces, and quotes
    /// only on a string that is empty or holds one of `,[]{}:`.
    #[test]
    fn interpolate_non_scalar_is_relaxed_json() {
        let d = serde_json::json!({
            "o": {"a": 1, "b": "x", "c": [true, null]},
            "q": ["", "a,b", "k:v", "plain words"],
        });
        let t = |k| interpolate(k, &occur_ctx("f", Some(&d), "h"), "_");
        assert_eq!(t("%o%"), "{a:1,b:x,c:[true,null]}");
        assert_eq!(t("%q%"), r#"["","a,b","k:v",plain words]"#);
    }

    /// da-7nv8r: a path segment indexes an array, `-1` from the end, and
    /// mixes with object keys; an index out of range expands to nothing.
    #[test]
    fn interpolate_path_indexes_arrays() {
        let d = serde_json::json!({"comment": ["a", "b", "c"], "x": [{"y": "deep"}]});
        let t = |k| interpolate(k, &occur_ctx("f", Some(&d), "h"), "_");
        let got = [
            "%comment.0%",
            "%comment.-1%",
            "%x.0.y%",
            "%comment.3%",
            "%comment.-4%",
        ]
        .map(t);
        assert_eq!(got, ["a", "c", "deep", "", ""]);
    }

    /// da-0ibow: `%hid%` whole, `%uid%` the first five, `%hid:N%` the first
    /// N from five up; below five it expands to nothing.
    #[test]
    fn interpolate_hid_uid_and_hid_prefix() {
        let ctx = occur_ctx("f", None, "a1b2c3d4e5f6");
        let got = interpolate("%hid%:%uid%:%hid:8%:%hid:99%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "a1b2c3d4e5f6:a1b2c:a1b2c3d4:a1b2c3d4e5f6");
        assert_eq!(interpolate("x__%hid:3%", &ctx, "__"), "x");
    }

    /// da-vk747: a stored `%-key%` reads as `%key%`; given, it is refused.
    #[test]
    fn a_retired_name_only_marker_reads_as_plain_and_is_refused() {
        let extra = serde_json::json!({"a": "q"});
        let ctx = occur_ctx("f", Some(&extra), "a1b2c3d4e5f6");
        assert_eq!(interpolate("%-a%:%-?b%", &ctx, ":"), "q");
        let err = refuse_name_only_marker("x__%-?a%", None).unwrap_err();
        assert!(
            err.to_string().ends_with("%-?a% is retired. Write %a%."),
            "{err}"
        );
        let err = interpolate_field("x__%-a%", &ctx, "__").unwrap_err();
        assert!(
            err.to_string().contains("`-F a`, which stores no a"),
            "{err}"
        );
    }

    /// da-0ibow: a field template refuses a plain placeholder that expands
    /// to nothing, naming it; `%?key%` and a present key pass.
    #[test]
    fn interpolate_field_refuses_empty_expansion() {
        let extra = serde_json::json!({"lang": "pt"});
        let ctx = occur_ctx("f", Some(&extra), "a1b2c3d4e5f6");
        let err = interpolate_field("x__%nope%__%hid:3%", &ctx, "__")
            .unwrap_err()
            .to_string();
        assert!(err.contains("%nope%") && err.contains("%hid:3%"), "{err}");
        assert_eq!(
            interpolate_field("w__%lang%__%?nope%", &ctx, "__").unwrap(),
            "w__pt"
        );
        assert_eq!(
            interpolate_field("w__%lang%", &ctx, "__").unwrap(),
            interpolate("w__%lang%", &ctx, "__")
        );
    }

    /// da-3se7a: `%::key%` reads the record handed in, at a dotted path, and
    /// `%::%` the template's own key; with no record, nothing, so a field
    /// template refuses it as before. One alone copies the value whole.
    #[test]
    fn record_placeholders_read_the_record_handed_in() {
        let own = serde_json::json!({"title": "own"});
        let record = serde_json::json!({"title": "T", "meta": {"n": 2}, "by": ["a", "b"]});
        let bare = occur_ctx("f", Some(&own), "a1b2c3d4e5f6");
        assert!(interpolate_field("x__%::title%", &bare, "__")
            .unwrap_err()
            .to_string()
            .contains("%::title%"));
        let ctx = InterpolationContext {
            record: Some(&record),
            record_key: Some("by"),
            ..occur_ctx("f", Some(&own), "a1b2c3d4e5f6")
        };
        assert!(reads_record("x %::title%") && !reads_record("%title% \\%::%"));
        assert_eq!(record_keys("%::% %::a.b% %::a% %b%", "own"), ["own", "a"]);
        assert_eq!(
            interpolate("%::title%:%::meta.n%:%title%", &ctx, ":"),
            "T:2:own"
        );
        let copy = |t: &str| copy_from_record(t, &ctx);
        assert_eq!(copy("%::%").unwrap(), Some(serde_json::json!(["a", "b"])));
        assert_eq!(copy("%::meta%").unwrap(), Some(serde_json::json!({"n": 2})));
        assert_eq!(copy("on %::title%").unwrap(), Some("on T".into()));
        assert_eq!(copy("%?::nope%").unwrap(), None);
        assert_eq!(copy("[%?::nope%]").unwrap(), Some("[]".into()));
        let err = copy("a %::nope%").unwrap_err().to_string();
        assert!(
            err.contains("%::nope%") && err.contains("f's record holds no nope"),
            "{err}"
        );
    }

    #[test]
    fn interpolate_state_from_extra_data() {
        // Legacy fallback: %state% pulls from extra_data.state when the
        // column-bound state is unset.
        let extra = serde_json::json!({"state": "happy"});
        let ctx = occur_ctx("mood", Some(&extra), "ZZZ");
        let got = interpolate("%field%:%state%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "mood:happy");

        // Canonical path: column-bound state wins.
        let state = serde_json::json!("focused");
        let switch_ctx = InterpolationContext {
            field: "mood",
            extra_data: None,
            hid: "zz",
            state: Some(&state),
            dur: None,
            following_state: None,
            occur_time: None,
            occur_time_precision: None,
            create_time: None,
            modify_time: None,
            client_id: "",
            record: None,
            record_key: None,
        };
        let got = interpolate("%field%:%state%", &switch_ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "mood:focused");
    }

    #[test]
    fn interpolate_missing_key_collapses_delimiter() {
        // state is missing → consecutive `:` collapse to one
        let ctx = occur_ctx("coffee", None, "abc12");
        let got = interpolate("%field%:%state%:%uid%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:abc12");

        // Leading delimiter from an empty head is trimmed.
        let got = interpolate("%state%:%field%:%uid%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:abc12");

        // Trailing delimiter from an empty tail is trimmed.
        let got = interpolate("%field%:%uid%:%state%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:abc12");

        // Custom delimiter
        let got = interpolate("%field%-%state%-%uid%", &ctx, "-");
        assert_eq!(got, "coffee-abc12");
    }

    #[test]
    fn interpolate_extra_data_arbitrary_key() {
        // %srcLang% and %pos% pull from extra_data
        let extra = serde_json::json!({"srcLang": "pt", "pos": "verb"});
        let ctx = InterpolationContext {
            field: "word",
            extra_data: Some(&extra),
            hid: "xx",
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
            occur_time_precision: None,
            create_time: None,
            modify_time: None,
            client_id: "",
            record: None,
            record_key: None,
        };
        let got = interpolate("%field%:%srcLang%:%pos%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "word:pt:verb");
    }

    #[test]
    fn interpolate_dot_path_reaches_nested_extra_data() {
        let extra = serde_json::json!({"a": {"b": "deep"}});
        let ctx = occur_ctx("f", Some(&extra), "u");
        let got = interpolate("%a.b%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "deep");
    }

    #[test]
    fn interpolate_escaped_percent_is_literal() {
        let ctx = occur_ctx("f", None, "u");
        let got = interpolate(r"100\% of %field%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "100% of f");
    }

    // da-xbjy / da-r17h: %create_time% / %modify_time% carry c / m markers,
    // the c after the time (da-06jxa).
    fn ctx_with_times<'a>(
        create: Option<&'a DateTime<Utc>>,
        modify: Option<&'a DateTime<Utc>>,
    ) -> InterpolationContext<'a> {
        InterpolationContext {
            field: "f",
            extra_data: None,
            hid: "u",
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
            occur_time_precision: None,
            create_time: create,
            modify_time: modify,
            client_id: "",
            record: None,
            record_key: None,
        }
    }

    #[test]
    fn interpolate_create_time_auto_suffix_c() {
        let t: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let ctx = ctx_with_times(Some(&t), None);
        let got = interpolate("%create_time%", &ctx, "__");
        assert_eq!(got, "2026-05-13T15:00:00.000Zc");
    }

    #[test]
    fn interpolate_modify_time_auto_prefix_m() {
        let t: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let ctx = ctx_with_times(None, Some(&t));
        let got = interpolate("%modify_time%", &ctx, "__");
        assert_eq!(got, "m2026-05-13T15:00:00.000Z");
    }

    #[test]
    fn interpolate_occur_time_has_no_suffix() {
        let dt: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let occur = DatumTime::from_utc_and_tz(dt, "UTC").unwrap();
        let ctx = InterpolationContext {
            field: "f",
            extra_data: None,
            hid: "u",
            state: None,
            dur: None,
            following_state: None,
            occur_time: Some(&occur),
            occur_time_precision: None,
            create_time: None,
            modify_time: None,
            client_id: "",
            record: None,
            record_key: None,
        };
        let got = interpolate("%occur_time%", &ctx, "__");
        assert_eq!(got, "2026-05-13T15:00:00.000Z");
    }

    /// da-4fyb: a date-only row (`P1D`) slugs on the bare *local* date. The
    /// stored instant is local midnight, so for an east-of-UTC zone the UTC
    /// day is the one before — rendering `t.utc` would key the entry to the
    /// wrong calendar day, on top of asserting a midnight that isn't data.
    #[test]
    fn interpolate_occur_time_renders_bare_local_date_for_p1d() {
        // 2026-09-01 00:00 in Europe/Berlin == 2026-08-31T22:00:00Z.
        let dt: DateTime<Utc> = "2026-08-31T22:00:00Z".parse().unwrap();
        let occur = DatumTime::from_utc_and_tz(dt, "Europe/Berlin").unwrap();
        let ctx = InterpolationContext {
            occur_time: Some(&occur),
            occur_time_precision: Some("P1D"),
            ..occur_ctx("coffee", None, "u")
        };
        let got = interpolate("%field%:%occur_time%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:2026-09-01");
    }

    /// A non-`P1D` precision is an `±N` uncertainty, not a date-only marker:
    /// the stamp stays full-resolution.
    #[test]
    fn interpolate_occur_time_keeps_full_stamp_for_other_precisions() {
        let dt: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let occur = DatumTime::from_utc_and_tz(dt, "UTC").unwrap();
        let ctx = InterpolationContext {
            occur_time: Some(&occur),
            occur_time_precision: Some("PT5M"),
            ..occur_ctx("f", None, "u")
        };
        let got = interpolate("%occur_time%", &ctx, "__");
        assert_eq!(got, "2026-05-13T15:00:00.000Z");
    }

    #[test]
    fn interpolate_create_time_composes_with_other_keys() {
        let extra = serde_json::json!({"a": "x"});
        let t: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let ctx = InterpolationContext {
            field: "f",
            extra_data: Some(&extra),
            hid: "u",
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
            occur_time_precision: None,
            create_time: Some(&t),
            modify_time: None,
            client_id: "",
            record: None,
            record_key: None,
        };
        let got = interpolate("%a%__%create_time%", &ctx, "__");
        assert_eq!(got, "x__2026-05-13T15:00:00.000Zc");
    }

    #[test]
    fn interpolate_create_time_missing_collapses_without_orphan_suffix() {
        // When create_time is unset, the rendered value must be empty so the
        // delimiter-collapse step trims cleanly — we must NOT emit a bare
        // 'c' that would survive the trim.
        let ctx = ctx_with_times(None, None);
        let got = interpolate("%field%:%create_time%:%uid%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "f:u");
    }

    // da-aq67: `%?key%` optional placeholder — missing keys render empty and
    // the surrounding delimiters collapse.
    #[test]
    fn interpolate_optional_missing_renders_empty() {
        let ctx = occur_ctx("f", None, "u");
        let got = interpolate("%?missing%", &ctx, "__");
        assert_eq!(got, "");
    }

    #[test]
    fn interpolate_optional_missing_strips_trailing_delimiter() {
        let extra = serde_json::json!({"a": "a"});
        let ctx = occur_ctx("f", Some(&extra), "u");
        let got = interpolate("%a%__%?b%", &ctx, "__");
        assert_eq!(got, "a");
    }

    #[test]
    fn interpolate_optional_missing_collapses_inner_delimiter() {
        let extra = serde_json::json!({"a": "a", "c": "c"});
        let ctx = occur_ctx("f", Some(&extra), "u");
        let got = interpolate("%a%__%?b%__%c%", &ctx, "__");
        assert_eq!(got, "a__c");
    }

    #[test]
    fn interpolate_optional_present_renders_value() {
        let extra = serde_json::json!({"a": "a", "b": "B", "c": "c"});
        let ctx = occur_ctx("f", Some(&extra), "u");
        let got = interpolate("%a%__%?b%__%c%", &ctx, "__");
        assert_eq!(got, "a__B__c");
    }

    #[test]
    fn interpolate_optional_create_time_keeps_c_when_present() {
        let t: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let ctx = ctx_with_times(Some(&t), None);
        let got = interpolate("%?create_time%", &ctx, "__");
        assert_eq!(got, "2026-05-13T15:00:00.000Zc");
    }

    #[test]
    fn interpolate_optional_create_time_absent_drops_cleanly() {
        let ctx = ctx_with_times(None, None);
        let got = interpolate("%field%:%?create_time%:%uid%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "f:u");
    }

    #[test]
    fn contains_template_detects_unescaped_pair() {
        assert!(contains_template("word__%srcLang%"));
        assert!(contains_template("%a%__%b%"));
    }

    #[test]
    fn contains_template_ignores_literal_percents() {
        assert!(!contains_template("plain_field"));
        assert!(!contains_template(""));
        // A single unescaped `%` (no closing pair) is not a template.
        assert!(!contains_template("100%"));
        // Escaped pairs are literal `%` chars; not a template.
        assert!(!contains_template(r"100\% off"));
        assert!(!contains_template(r"\%not_a_key\%"));
    }

    #[test]
    fn interpolate_numeric_and_bool_values_stringify() {
        let extra = serde_json::json!({"count": 3, "on": true});
        let ctx = occur_ctx("f", Some(&extra), "u");
        let got = interpolate("%count%:%on%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "3:true");
    }

    // ─── da-k2xv: camelCase aliases for time-metadata keys ─────────────

    #[test]
    fn interpolate_camelcase_createtime_aliases_create_time() {
        let t: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let ctx = ctx_with_times(Some(&t), None);
        let got = interpolate("%createTime%", &ctx, "__");
        // camelCase resolves to the snake_case branch — including the
        // `c` marker that create_time carries.
        assert_eq!(got, "2026-05-13T15:00:00.000Zc");
    }

    #[test]
    fn interpolate_camelcase_modifytime_aliases_modify_time() {
        let t: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let ctx = ctx_with_times(None, Some(&t));
        let got = interpolate("%modifyTime%", &ctx, "__");
        assert_eq!(got, "m2026-05-13T15:00:00.000Z");
    }

    #[test]
    fn strip_field_prefix_takes_only_a_leading_field_and_one_separator() {
        assert_eq!(
            strip_field_prefix("%field%:%occurTime%"),
            Some("%occurTime%")
        );
        assert_eq!(strip_field_prefix("%field%__%x%"), Some("_%x%"));
        for kept in [
            "%occurTime%",
            "%occurTime%:%field%",
            "%field%",
            "%field%:",
            "%field%%occurTime%",
            "%field%x%occurTime%",
            "%?field%:%occurTime%",
        ] {
            assert_eq!(strip_field_prefix(kept), None, "{kept}");
        }
    }

    #[test]
    fn interpolate_camelcase_occurtime_aliases_occur_time() {
        let dt: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let occur = DatumTime::from_utc_and_tz(dt, "UTC").unwrap();
        let ctx = InterpolationContext {
            field: "task",
            extra_data: None,
            hid: "z",
            state: None,
            dur: None,
            following_state: None,
            occur_time: Some(&occur),
            occur_time_precision: None,
            create_time: None,
            modify_time: None,
            client_id: "",
            record: None,
            record_key: None,
        };
        let got = interpolate("%occurTime%", &ctx, "__");
        assert_eq!(got, "2026-05-13T15:00:00.000Z");
    }

    #[test]
    fn interpolate_camelcase_optional_form_also_works() {
        // The `?` (optional) prefix should compose with the camelCase
        // alias — `%?createTime%` resolves the same way as
        // `%createTime%`.
        let t: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let ctx = ctx_with_times(Some(&t), None);
        let got = interpolate("%?createTime%", &ctx, "__");
        assert_eq!(got, "2026-05-13T15:00:00.000Zc");
    }

    /// da-b5k87: `names_time` reads placeholders as `interpolate` splits
    /// them, so an optional or camelCase time counts and a near-miss key, an
    /// escaped literal or a time name in raw text does not.
    #[test]
    fn names_time_walks_placeholders_like_interpolate() {
        for t in [
            "%occur_time%",
            "%?occur_time%",
            "%?createTime%",
            "x__%modify_time%",
        ] {
            assert!(names_time(t), "{t}");
        }
        for t in [
            "%uid%",
            "%occur_time_extra%",
            r"\%occur_time\%",
            "a%b%occur_time",
            "fixed",
        ] {
            assert!(!names_time(t), "{t}");
        }
    }
}
