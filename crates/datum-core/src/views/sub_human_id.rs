//! `datum_sub_human_id` — every non-empty prefix of each entry's short uid.
//!
//! Ported from datum's `subHumanIdView`. C3 (human-id shortening) uses this
//! to find the shortest unique prefix of an entry's id: for prefix lengths
//! 1..N, query `?key=<prefix>`; the first length with a single hit is the
//! shortest-unique form.
//!
//! datum keys off `meta.humanId` (a nanoid stamped at write time). daydumb
//! derives the "human id" from the entry's ULID tail via
//! [`Entry::short_uid`] — no new column needed.

use serde_json::{json, Value as JsonValue};

use super::{View, ViewRow};
use crate::db::Db;
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
            let hid = entry.short_uid();
            // Emit every non-empty prefix. Length 0 is omitted: an empty
            // key would match every entry and isn't useful to C3.
            for len in 1..=hid.chars().count() {
                let prefix: String = hid.chars().take(len).collect();
                rows.push(ViewRow {
                    key: JsonValue::String(prefix),
                    value: JsonValue::String(hid.clone()),
                    id: Some(entry.id.clone()),
                });
            }
        }
        Ok(rows)
    }

    fn supports_reduce(&self) -> bool {
        true
    }

    /// `_count` — number of distinct ids sharing this prefix. A value of 1
    /// under `group=true` means this prefix is already unique.
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

    /// Build an entry with a handcrafted id so we can assert exact prefixes.
    /// We bypass `Entry::new`'s ULID generation by constructing the struct
    /// directly and inserting it, then relying on `short_uid()`'s last-5-char
    /// rule to produce a known tail.
    fn insert_with_id(db: &crate::db::Db, field: &str, id: &str) -> Entry {
        let now = chrono::Utc::now();
        let e = Entry {
            id: id.to_string(),
            slug: format!("{field}:fixture"),
            content_hash: format!("hash-{id}"),
            field: field.to_string(),
            verb: Verb::Note,
            value: None,
            extra_data: None,
            unit: None,
            time_start: None,
            time_end: None,
            lat: None,
            lon: None,
            location: None,
            created_at: now,
            modified_at: now,
            device_id: "test".to_string(),
            id_structure: None,
            field_structure: None,
            deleted_at: None,
        };
        db.insert_entry(&e).unwrap();
        e
    }

    #[test]
    fn sub_human_id_view_returns_shortest_unique_prefix() {
        let db = fresh_db();

        // Last 5 chars → "abcde", "abxyz", "qrstu". Prefixes of first two
        // share "a" and "ab"; diverge at length 3.
        let e1 = insert_with_id(&db, "word", "01HZZZZZZZZZZZZZZZZZZABCDE");
        let e2 = insert_with_id(&db, "word", "01HZZZZZZZZZZZZZZZZZZABXYZ");
        let e3 = insert_with_id(&db, "word", "01HZZZZZZZZZZZZZZZZZZQRSTU");

        let hid1 = e1.short_uid(); // "abcde"
        let hid2 = e2.short_uid(); // "abxyz"
        let hid3 = e3.short_uid(); // "qrstu"
        assert_eq!(hid1, "abcde");
        assert_eq!(hid2, "abxyz");
        assert_eq!(hid3, "qrstu");

        // Tombstoned entries must not contribute prefixes.
        let ghost = insert_with_id(&db, "word", "01HZZZZZZZZZZZZZZZZZZABCDX");
        db.soft_delete_entry(&ghost.id).unwrap();

        let reg = builtins();

        // Shortest unique prefix of e1 ("abcde") is "abc": length 1 and 2
        // collide with e2; length 3 disambiguates.
        let shortest_unique = (1..=5)
            .map(|n| hid1.chars().take(n).collect::<String>())
            .find(|prefix| {
                let params = ViewParams {
                    key: Some(JsonValue::String(prefix.clone())),
                    ..Default::default()
                };
                let hits = reg.query("datum_sub_human_id", &db, &params).unwrap();
                // Dedupe by id since each entry contributes one row per prefix length.
                let unique_ids: std::collections::HashSet<_> =
                    hits.iter().map(|r| r.id.clone()).collect();
                unique_ids.len() == 1
            })
            .expect("should find a unique prefix");
        assert_eq!(shortest_unique, "abc");

        // "q" alone disambiguates e3.
        let params = ViewParams {
            key: Some(json!("q")),
            ..Default::default()
        };
        let hits = reg.query("datum_sub_human_id", &db, &params).unwrap();
        let ids: std::collections::HashSet<_> = hits.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, std::collections::HashSet::from([Some(e3.id.clone())]));

        // "ab" is ambiguous: returns both e1 and e2 (one row per, at this length).
        let params = ViewParams {
            key: Some(json!("ab")),
            ..Default::default()
        };
        let hits = reg.query("datum_sub_human_id", &db, &params).unwrap();
        let ids: std::collections::HashSet<_> = hits.iter().map(|r| r.id.clone()).collect();
        assert_eq!(
            ids,
            std::collections::HashSet::from([Some(e1.id.clone()), Some(e2.id.clone())])
        );

        // Grouped reduce: count of distinct-ish rows per prefix. "ab" → 2 rows
        // (one per contributing entry); "abc" → 1 row (e1 only).
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
