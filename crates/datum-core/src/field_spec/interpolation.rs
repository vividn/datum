//! Template interpolation for entry slugs / id templates.
//!
//! Templates use `%key%` placeholders. `%field%`, `%uid%`, and the column-
//! bound state/dur/following_state/occur_time keys come from the
//! [`InterpolationContext`]; everything else falls through to a dot-path
//! lookup against `extra_data`. `\%` is a literal `%`.

use serde_json::Value as JsonValue;

use crate::entry::{Entry, Verb};
use crate::time::DatumTime;

/// Default delimiter used when `--id-delimiter` is not provided.
pub const DEFAULT_ID_DELIMITER: &str = ":";

/// Lookup context for [`interpolate`]. Keeps the helper decoupled from the
/// full [`Entry`] shape so it is easy to test and reuse.
pub struct InterpolationContext<'a> {
    pub field: &'a str,
    pub verb: &'a Verb,
    pub value: Option<&'a JsonValue>,
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
}

impl<'a> InterpolationContext<'a> {
    /// Build an [`InterpolationContext`] and the associated short-uid buffer
    /// from an [`Entry`]. The uid buffer is returned separately so the
    /// borrow-checker can keep the context tied to it.
    pub fn from_entry(entry: &'a Entry, uid_buf: &'a str) -> Self {
        Self {
            field: &entry.field,
            verb: &entry.verb,
            value: entry.state.as_ref(),
            extra_data: entry.data.as_ref(),
            uid: uid_buf,
            state: entry.state.as_ref(),
            dur: entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
            following_state: entry.following_state.as_ref(),
            occur_time: entry.occur_time.as_ref(),
        }
    }
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
    match key {
        "" => String::new(),
        "field" => ctx.field.to_string(),
        "uid" => ctx.uid.to_string(),
        "state" => {
            // da-5ns.4: column-bound state wins. The column is canonical
            // post-pivot; the data-blob and Switch-value paths are kept as
            // fallbacks for legacy/in-flight entries that haven't been
            // canonicalized yet.
            if let Some(s) = ctx.state {
                return json_to_plain_string(s);
            }
            if let Some(s) = lookup_extra_data(ctx.extra_data, "state") {
                return s;
            }
            if matches!(ctx.verb, Verb::Switch) {
                if let Some(v) = ctx.value {
                    return json_to_plain_string(v);
                }
            }
            String::new()
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
        _ => lookup_extra_data(ctx.extra_data, key).unwrap_or_default(),
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
    if delim.is_empty() {
        return s.to_string();
    }
    let double = format!("{delim}{delim}");
    let mut result = s.to_string();
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
            verb: &Verb::Point,
            value: None,
            extra_data: extra,
            uid,
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
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
        // Non-switch verb: %state% pulls from extra_data.state
        let extra = serde_json::json!({"state": "happy"});
        let ctx = occur_ctx("mood", Some(&extra), "ZZZ");
        let got = interpolate("%field%:%state%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "mood:happy");

        // Switch verb: value is the state (no extra_data.state needed)
        let value = serde_json::json!("focused");
        let switch_ctx = InterpolationContext {
            field: "mood",
            verb: &Verb::Switch,
            value: Some(&value),
            extra_data: None,
            uid: "zz",
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
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
            verb: &Verb::Point,
            value: None,
            extra_data: Some(&extra),
            uid: "xx",
            state: None,
            dur: None,
            following_state: None,
            occur_time: None,
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

    #[test]
    fn interpolate_numeric_and_bool_values_stringify() {
        let extra = serde_json::json!({"count": 3, "on": true});
        let ctx = occur_ctx("f", Some(&extra), "u");
        let got = interpolate("%count%:%on%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "3:true");
    }
}
