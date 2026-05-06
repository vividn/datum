//! `datum_sub_human_id` — every non-empty prefix of each entry's short hid.
//!
//! SQL-backed. Used to find the shortest unique prefix of an entry's human
//! id: for prefix lengths 1..N, query `?key=<prefix>`; the first length
//! with a single hit is the shortest-unique form.
//!
//! daydumb's `hid` column is a stored 12-char base36 random — that value
//! IS the human id. The short form shown in action output is
//! [`Entry::short_uid`] (first [`SHORT_HID_LEN`] chars), so this view emits
//! prefixes up to that length. When the caller supplies a string `key` up
//! to that length, the scan pushes a `substr(hid,1,SHORT_HID_LEN)
//! LIKE <key>%` filter into SQL and only fans out prefixes in Rust for the
//! matching entries.

use rusqlite::params_from_iter;
use serde_json::{json, Value as JsonValue};

use super::{apply_limit, cmp_json, filter_by_key, group_key, View, ViewParams, ViewRow};
use crate::db::Db;
use crate::entry::SHORT_HID_LEN;
use crate::error::{DatumError, Result};

pub struct SubHumanIdView;

impl View for SubHumanIdView {
    fn name(&self) -> &str {
        "datum_sub_human_id"
    }

    fn query(&self, db: &Db, params: &ViewParams) -> Result<Vec<ViewRow>> {
        // Push down a prefix filter when `--key` / `--startkey` narrow the
        // scan. Everything else surfaces all live entries.
        let prefix_filter = prefix_from_params(params);

        let mut sql = String::from(
            "SELECT slug, substr(hid, 1, ?1) AS short_hid
             FROM entries
             WHERE deleted_at IS NULL",
        );
        let mut args: Vec<String> = vec![SHORT_HID_LEN.to_string()];
        if let Some(p) = &prefix_filter {
            // Escape SQL LIKE meta-chars; hids are base36 so normally none of
            // these show up, but be safe against callers passing arbitrary
            // strings via `--key`.
            let escaped = escape_like(p);
            sql.push_str(" AND substr(hid, 1, ?1) LIKE ?2 || '%' ESCAPE '\\'");
            args.push(escaped);
        }

        let mut rows = Vec::new();
        let mut stmt = db.conn().prepare(&sql)?;
        let mapped = stmt.query_map(
            params_from_iter(args.iter()),
            |row| -> rusqlite::Result<(String, String)> {
                Ok((row.get("slug")?, row.get("short_hid")?))
            },
        )?;
        for r in mapped {
            let (slug, short_hid) = r.map_err(DatumError::Db)?;
            let n = short_hid.chars().count().min(SHORT_HID_LEN);
            for len in 1..=n {
                let prefix: String = short_hid.chars().take(len).collect();
                rows.push(ViewRow {
                    key: JsonValue::String(prefix),
                    value: JsonValue::String(short_hid.clone()),
                    id: Some(slug.clone()),
                });
            }
        }

        rows = filter_by_key(rows, params);
        rows.sort_by(|a, b| cmp_json(&a.key, &b.key));

        if params.reduce {
            rows = reduce_count(rows, params);
        }
        Ok(apply_limit(rows, params))
    }
}

fn prefix_from_params(params: &ViewParams) -> Option<String> {
    if let Some(k) = &params.key {
        if let Some(s) = k.as_str() {
            return Some(s.to_string());
        }
    }
    if let (Some(s), Some(e)) = (params.startkey.as_ref(), params.endkey.as_ref()) {
        if let (Some(a), Some(b)) = (s.as_str(), e.as_str()) {
            // A common range like `["pre", "pre"]` means exact prefix.
            if a == b {
                return Some(a.to_string());
            }
            // Range scan — narrow to the shared leading characters.
            let shared: String = a
                .chars()
                .zip(b.chars())
                .take_while(|(x, y)| x == y)
                .map(|(x, _)| x)
                .collect();
            if !shared.is_empty() {
                return Some(shared);
            }
        }
    }
    None
}

fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '%' | '_' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

fn reduce_count(rows: Vec<ViewRow>, params: &ViewParams) -> Vec<ViewRow> {
    let mut groups: Vec<(JsonValue, i64)> = Vec::new();
    for r in rows {
        let gk = group_key(&r.key, params);
        if let Some(entry) = groups.iter_mut().find(|(k, _)| k == &gk) {
            entry.1 += 1;
        } else {
            groups.push((gk, 1));
        }
    }
    groups
        .into_iter()
        .map(|(key, count)| ViewRow {
            key,
            value: json!(count),
            id: None,
        })
        .collect()
}
