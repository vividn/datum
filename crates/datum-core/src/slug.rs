//! Slug derivation — write-time construction of the human-facing `slug`
//! column from a field name + optional id-keys (or timestamp fallback).
//!
//! Slugs anchor every entry with a human reference (`field:<ident>`) that
//! stays stable across devices. The default falls back to a timestamp when
//! the caller has no id-keys; the `-K` flag lets callers chain explicit
//! values joined by `__`.

use crate::ids::Slug;
use crate::time::DatumTime;

/// Build the slug for an entry.
///
/// - When `id_values` is non-empty, values are joined by `__` and appended
///   to `<field>:`. Empty values are dropped, so an absent
///   optional-no-default slot doesn't leave an empty `__` segment.
/// - When every `id_values` entry is empty (or the slice is empty) and
///   `fallback_time` is set, returns `<field>:<timestamp>` at millisecond
///   precision (same-field, same-millisecond inserts collide by design —
///   the DB retries with a suffix so the human-visible slug stays intact).
/// - When neither id-keys nor a fallback time is provided, returns
///   `<field>:unset` — the historical placeholder for tagless entries.
pub fn derive(field: &str, id_values: &[String], fallback_time: Option<&DatumTime>) -> Slug {
    let joined: Vec<&str> = id_values
        .iter()
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .collect();

    if !joined.is_empty() {
        let mut out = String::with_capacity(field.len() + joined.iter().map(|s| s.len() + 2).sum::<usize>());
        out.push_str(field);
        out.push(':');
        out.push_str(&joined.join("__"));
        return Slug(out);
    }

    let s = match fallback_time {
        Some(t) => format!("{}:{}", field, t.utc.format("%Y-%m-%dT%H:%M:%S%.3fZ")),
        None => format!("{}:unset", field),
    };
    Slug(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::DatumTime;
    use chrono::{TimeZone, Utc};

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
    fn id_values_join_with_double_underscore() {
        let values = vec!["jake".to_string(), "berlin".to_string()];
        let got = derive("person", &values, None);
        assert_eq!(got, "person:jake__berlin");
    }

    #[test]
    fn empty_id_values_are_dropped() {
        // First value populated, second omitted (e.g. optional-no-default absent)
        let values = vec!["Goldmund".to_string(), String::new()];
        let got = derive("book", &values, None);
        assert_eq!(got, "book:Goldmund");
    }

    #[test]
    fn all_empty_id_values_falls_back_to_time() {
        let values = vec![String::new(), String::new()];
        let got = derive("coffee", &values, Some(&fixed_time()));
        assert_eq!(got, "coffee:2024-03-16T09:15:00.000Z");
    }

    #[test]
    fn id_values_override_timestamp_fallback() {
        let values = vec!["morning".to_string()];
        let got = derive("journal", &values, Some(&fixed_time()));
        assert_eq!(got, "journal:morning");
    }
}
