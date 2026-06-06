//! Template interpolation for entry slugs / id templates.
//!
//! Templates use `%key%` placeholders. `%field%`, `%uid%`, and the column-
//! bound state/dur/following_state/occur_time keys come from the
//! [`InterpolationContext`]; everything else falls through to a dot-path
//! lookup against `extra_data`. `\%` is a literal `%`.
//!
//! A leading `?` (`%?key%`) marks the placeholder as optional. Missing keys
//! always render as the empty string and the surrounding delimiters collapse,
//! so `%?key%` and `%key%` produce identical output today; the `?` documents
//! intent and reserves a hook for a future strict-required mode.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

use crate::entry::Entry;
use crate::time::DatumTime;

/// Default delimiter used when `--id-delimiter` is not provided.
pub const DEFAULT_ID_DELIMITER: &str = ":";

/// Lookup context for [`interpolate`]. Keeps the helper decoupled from the
/// full [`Entry`] shape so it is easy to test and reuse.
pub struct InterpolationContext<'a> {
    pub field: &'a str,
    pub extra_data: Option<&'a JsonValue>,
    pub uid: &'a str,
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
    /// `YYYY-MM-DDTHH:MM:SS.sssZ` (matching the default-slug builder).
    pub occur_time: Option<&'a DatumTime>,
    /// da-xbjy: `%create_time%` renders this and auto-appends `'c'`
    /// (deliberate divergence from datum, which required the user to write
    /// the literal suffix).
    pub create_time: Option<&'a DateTime<Utc>>,
    /// da-xbjy: `%modify_time%` renders this and auto-appends `'m'`.
    pub modify_time: Option<&'a DateTime<Utc>>,
}

impl<'a> InterpolationContext<'a> {
    /// Build an [`InterpolationContext`] and the associated short-uid buffer
    /// from an [`Entry`]. The uid buffer is returned separately so the
    /// borrow-checker can keep the context tied to it.
    pub fn from_entry(entry: &'a Entry, uid_buf: &'a str) -> Self {
        Self {
            field: &entry.field,
            extra_data: entry.data.as_ref(),
            uid: uid_buf,
            state: entry.state.as_ref(),
            dur: entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
            following_state: entry.following_state.as_ref(),
            occur_time: entry.occur_time.as_ref(),
            create_time: Some(&entry.create_time),
            modify_time: Some(&entry.modify_time),
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

fn lookup_field(key: &str, ctx: &InterpolationContext) -> String {
    let key = key.strip_prefix('?').unwrap_or(key);
    // da-k2xv: legacy camelCase time metadata names (occurTime / createTime
    // / modifyTime) resolve to their snake_case counterparts so id-templates
    // ported verbatim from the original datum (which used camelCase) keep
    // working. Other camelCase keys fall through to lookup_extra_data
    // unchanged.
    let key = match key {
        "occurTime" => "occur_time",
        "createTime" => "create_time",
        "modifyTime" => "modify_time",
        // following_state and dur have no legacy camelCase form (they're
        // post-v3 additions); pass through unchanged.
        other => other,
    };
    match key {
        "" => String::new(),
        "field" => ctx.field.to_string(),
        "uid" => ctx.uid.to_string(),
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
        "occur_time" => ctx
            .occur_time
            .map(|t| t.utc.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
            .unwrap_or_default(),
        // da-xbjy: auto-suffix `c` / `m` (deliberate divergence from datum).
        // The suffix is appended to the rendered value, not the key, and only
        // when the value is non-empty so missing-key delimiter collapsing
        // still works.
        "create_time" => suffix_when_present(
            ctx.create_time
                .map(|t| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()),
            'c',
        ),
        "modify_time" => suffix_when_present(
            ctx.modify_time
                .map(|t| t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()),
            'm',
        ),
        _ => lookup_extra_data(ctx.extra_data, key).unwrap_or_default(),
    }
}

fn suffix_when_present(rendered: Option<String>, suffix: char) -> String {
    match rendered {
        Some(s) if !s.is_empty() => {
            let mut out = s;
            out.push(suffix);
            out
        }
        _ => String::new(),
    }
}

/// Walk `extra_data` using dot-path `key` (e.g. `"a.b"`). Returns None if any
/// segment is missing or not an object.
fn lookup_extra_data(extra: Option<&JsonValue>, key: &str) -> Option<String> {
    let mut cur = extra?;
    for segment in key.split('.') {
        match cur {
            JsonValue::Object(m) => {
                cur = m.get(segment)?;
            }
            _ => return None,
        }
    }
    Some(json_to_plain_string(cur))
}

fn json_to_plain_string(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => String::new(),
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
        uid: &'a str,
    ) -> InterpolationContext<'a> {
        InterpolationContext {
            field,
            extra_data: extra,
            uid,
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
            create_time: None,
            modify_time: None,
        }
    }

    #[test]
    fn interpolate_field_uid_template() {
        let ctx = occur_ctx("coffee", None, "abc12");
        let got = interpolate("%field%:%uid%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:abc12");
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
            uid: "zz",
            state: Some(&state),
            dur: None,
            following_state: None,
            occur_time: None,
            create_time: None,
            modify_time: None,
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
            uid: "xx",
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
            create_time: None,
            modify_time: None,
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

    // da-xbjy: %create_time% / %modify_time% auto-append c / m suffixes.
    fn ctx_with_times<'a>(
        create: Option<&'a DateTime<Utc>>,
        modify: Option<&'a DateTime<Utc>>,
    ) -> InterpolationContext<'a> {
        InterpolationContext {
            field: "f",
            extra_data: None,
            uid: "u",
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
            create_time: create,
            modify_time: modify,
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
    fn interpolate_modify_time_auto_suffix_m() {
        let t: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let ctx = ctx_with_times(None, Some(&t));
        let got = interpolate("%modify_time%", &ctx, "__");
        assert_eq!(got, "2026-05-13T15:00:00.000Zm");
    }

    #[test]
    fn interpolate_occur_time_has_no_suffix() {
        let dt: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let occur = DatumTime::from_utc_and_tz(dt, "UTC").unwrap();
        let ctx = InterpolationContext {
            field: "f",
            extra_data: None,
            uid: "u",
            state: None,
            dur: None,
            following_state: None,
            occur_time: Some(&occur),
            create_time: None,
            modify_time: None,
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
            uid: "u",
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
            create_time: Some(&t),
            modify_time: None,
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
    fn interpolate_optional_create_time_keeps_c_suffix_when_present() {
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
        // `c` auto-suffix that create_time carries.
        assert_eq!(got, "2026-05-13T15:00:00.000Zc");
    }

    #[test]
    fn interpolate_camelcase_modifytime_aliases_modify_time() {
        let t: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let ctx = ctx_with_times(None, Some(&t));
        let got = interpolate("%modifyTime%", &ctx, "__");
        assert_eq!(got, "2026-05-13T15:00:00.000Zm");
    }

    #[test]
    fn interpolate_camelcase_occurtime_aliases_occur_time() {
        let dt: DateTime<Utc> = "2026-05-13T15:00:00Z".parse().unwrap();
        let occur = DatumTime::from_utc_and_tz(dt, "UTC").unwrap();
        let ctx = InterpolationContext {
            field: "task",
            extra_data: None,
            uid: "z",
            state: None,
            dur: None,
            following_state: None,
            occur_time: Some(&occur),
            create_time: None,
            modify_time: None,
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
}
