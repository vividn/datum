//! `datum_sub_human_id` — every non-empty prefix of each entry's short hid.
//!
//! Ported from datum's `subHumanIdView`. Used to find the shortest unique
//! prefix of an entry's human id: for prefix lengths 1..N, query
//! `?key=<prefix>`; the first length with a single hit is the
//! shortest-unique form.
//!
//! daydumb's `hid` column is a stored 12-char base36 random — that value IS
//! the human id (no derivation from a separate key). The short form shown
//! in action output is [`Entry::short_uid`] (first 5 chars), so this view
//! emits prefixes up to that length, which covers all realistic ambiguity.

use serde_json::{json, Value as JsonValue};

use super::{View, ViewRow};
use crate::db::Db;
use crate::entry::SHORT_HID_LEN;
use crate::error::Result;

pub struct SubHumanIdView;

impl View for SubHumanIdView {
    fn name(&self) -> &str {
        "datum_sub_human_id"
    }

    fn map(&self, db: &Db) -> Result<Vec<ViewRow>> {
        let entries = db.all_entries()?;
        let mut rows = Vec::new();
        for entry in entries {
            if entry.deleted_at.is_some() {
                continue;
            }
            let hid_short = entry.short_uid();
            // Emit every non-empty prefix up to the short-hid length. C3
            // only cares about the shortest-unique form for display; full
            // hid prefix lookups go through `Db::resolve_prefix` instead.
            for len in 1..=hid_short.chars().count().min(SHORT_HID_LEN) {
                let prefix: String = hid_short.chars().take(len).collect();
                rows.push(ViewRow {
                    key: JsonValue::String(prefix),
                    value: JsonValue::String(hid_short.clone()),
                    id: Some(entry.slug.clone()),
                });
            }
        }
        Ok(rows)
    }

    fn supports_reduce(&self) -> bool {
        true
    }

    /// `_count` — number of distinct entries sharing this prefix. A value of
    /// 1 under `group=true` means this prefix is already unique.
    fn reduce(&self, _key: &JsonValue, values: &[JsonValue]) -> Result<JsonValue> {
        Ok(json!(values.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{Entry, Verb};
    use crate::test_util::fresh_db;
    use crate::views::{builtins, ViewParams};

    /// Insert an entry with a handcrafted hid so short_uid is deterministic.
    fn insert_with_hid(db: &crate::db::Db, field: &str, slug: &str, hid: &str) -> Entry {
        use crate::entry::{EntryShape, PrecisionUnit};
        let now = chrono::Utc::now();
        let e = Entry {
            hid: hid.to_string(),
            slug: slug.to_string(),
            content_hash: format!("hash-{hid}"),
            field: field.to_string(),
            entry_shape: EntryShape::Point,
            intent: Some("note".to_string()),
            verb: Verb::Add,
            value: None,
            data: None,
            unit: None,
            start_time: None,
            end_time: None,
            start_time_precision: 0,
            start_time_precision_unit: PrecisionUnit::Exact,
            end_time_precision: 0,
            end_time_precision_unit: PrecisionUnit::Exact,
            lat: None,
            lon: None,
            location: None,
            create_time: now,
            modify_time: now,
            device_id: "test".to_string(),
            id_structure: None,
            field_structure: None,
            deleted_at: None,
            raw_input: String::new(),
            alias_chain: None,
            parser_version: 1,
        };
        db.insert_entry(&e).unwrap();
        e
    }

    #[test]
    fn sub_human_id_view_returns_shortest_unique_prefix() {
        let db = fresh_db();

        // short_uid() takes the first 5 chars of hid: "abcde", "abxyz", "qrstu".
        // Prefixes of the first two share "a" and "ab"; diverge at length 3.
        let e1 = insert_with_hid(&db, "word", "word:1", "abcde0000001");
        let e2 = insert_with_hid(&db, "word", "word:2", "abxyz0000002");
        let e3 = insert_with_hid(&db, "word", "word:3", "qrstu0000003");

        assert_eq!(e1.short_uid(), "abcde");
        assert_eq!(e2.short_uid(), "abxyz");
        assert_eq!(e3.short_uid(), "qrstu");

        // Tombstoned entries must not contribute prefixes.
        let ghost = insert_with_hid(&db, "word", "word:4", "abcdx0000004");
        db.soft_delete_entry(&ghost.slug).unwrap();

        let reg = builtins();

        // Shortest unique prefix of e1 ("abcde") is "abc": length 1 and 2
        // collide with e2; length 3 disambiguates.
        let shortest_unique = (1..=5)
            .map(|n| e1.short_uid().chars().take(n).collect::<String>())
            .find(|prefix| {
                let params = ViewParams {
                    key: Some(JsonValue::String(prefix.clone())),
                    ..Default::default()
                };
                let hits = reg.query("datum_sub_human_id", &db, &params).unwrap();
                // Dedupe by slug since each entry contributes one row per prefix length.
                let unique_slugs: std::collections::HashSet<_> =
                    hits.iter().map(|r| r.id.clone()).collect();
                unique_slugs.len() == 1
            })
            .expect("should find a unique prefix");
        assert_eq!(shortest_unique, "abc");

        // "q" alone disambiguates e3.
        let params = ViewParams {
            key: Some(json!("q")),
            ..Default::default()
        };
        let hits = reg.query("datum_sub_human_id", &db, &params).unwrap();
        let slugs: std::collections::HashSet<_> = hits.iter().map(|r| r.id.clone()).collect();
        assert_eq!(slugs, std::collections::HashSet::from([Some(e3.slug.clone())]));

        // "ab" is ambiguous: returns both e1 and e2 (one row per, at this length).
        let params = ViewParams {
            key: Some(json!("ab")),
            ..Default::default()
        };
        let hits = reg.query("datum_sub_human_id", &db, &params).unwrap();
        let slugs: std::collections::HashSet<_> = hits.iter().map(|r| r.id.clone()).collect();
        assert_eq!(
            slugs,
            std::collections::HashSet::from([Some(e1.slug.clone()), Some(e2.slug.clone())])
        );

        // Grouped reduce: count of distinct rows per prefix. "ab" → 2 rows;
        // "abc" → 1 row (e1 only); "q" → 1 row (e3).
        let params = ViewParams {
            reduce: true,
            group: true,
            ..Default::default()
        };
        let rows = reg.query("datum_sub_human_id", &db, &params).unwrap();
        let by_key: std::collections::HashMap<_, _> =
            rows.iter().map(|r| (r.key.clone(), r.value.clone())).collect();
        assert_eq!(by_key[&json!("ab")], json!(2));
        assert_eq!(by_key[&json!("abc")], json!(1));
        assert_eq!(by_key[&json!("q")], json!(1));
    }
}
