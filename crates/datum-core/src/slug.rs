//! Slug derivation — write-time construction of the human-facing `slug`
//! column from a field name + optional id-keys (or timestamp fallback).
//!
//! Historically slugs were built inside the DB layer's entry insertion.
//! Moving this to a dedicated module paves the way for D2's `-K` flag,
//! which lets callers pass explicit id-keys to override the default
//! `field:timestamp` shape without touching the DB.

use serde_json::Value as JsonValue;

use crate::time::DatumTime;

/// Build the slug for an entry.
///
/// - When `id_keys` is non-empty, each key-value pair is appended as
///   `:<k>=<v>`; the leading `field:` prefix still anchors the slug.
/// - When `id_keys` is empty, falls back to `<field>:<timestamp>` at
///   millisecond precision (matching the pre-move default). Same-field,
///   same-millisecond inserts collide by design — the DB retries with a
///   suffix so the human-visible slug stays intact.
/// - When neither id-keys nor a fallback time is provided, returns
///   `<field>:unset` — the historical placeholder for tagless entries.
pub fn derive(field: &str, id_keys: &[(String, JsonValue)], fallback_time: Option<&DatumTime>) -> String {
    if !id_keys.is_empty() {
        let mut out = String::with_capacity(field.len() + 16);
        out.push_str(field);
        for (k, v) in id_keys {
            out.push(':');
            out.push_str(k);
            out.push('=');
            out.push_str(&render_value(v));
        }
        return out;
    }

    match fallback_time {
        Some(t) => format!("{}:{}", field, t.utc.format("%Y-%m-%dT%H:%M:%S%.3fZ")),
        None => format!("{}:unset", field),
    }
}

fn render_value(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::DatumTime;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn fixed_time() -> DatumTime {
        let utc = Utc.with_ymd_and_hms(2024, 3, 16, 9, 15, 0).unwrap();
        DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() }
    }

    #[test]
    fn default_is_field_colon_timestamp() {
        let got = derive("coffee", &[], Some(&fixed_time()));
        assert_eq!(got, "coffee:2024-03-16T09:15:00.000Z");
    }

    #[test]
    fn without_time_falls_back_to_unset() {
        let got = derive("word", &[], None);
        assert_eq!(got, "word:unset");
    }

    #[test]
    fn id_keys_append_as_pairs() {
        let keys = vec![
            ("id".to_string(), json!("correr")),
            ("srcLang".to_string(), json!("pt")),
        ];
        let got = derive("word", &keys, None);
        assert_eq!(got, "word:id=correr:srcLang=pt");
    }

    #[test]
    fn id_keys_override_timestamp_fallback() {
        let keys = vec![("id".to_string(), json!("morning"))];
        let got = derive("journal", &keys, Some(&fixed_time()));
        assert_eq!(got, "journal:id=morning");
    }
}
