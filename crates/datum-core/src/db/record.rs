//! `collect` (da-qhm51): a field's record, `get <field>::`.

use chrono::Utc;
use rusqlite::params;
use serde_json::{json, Map, Value as JsonValue};

use crate::error::Result;

use super::types::LIVE_SQL;
use super::Db;

impl Db {
    /// The record of `field`: its state now ([`Db::state_at`]) and the fold
    /// of its live entries' data in the listing's order, `occur_time ??
    /// create_time` then slug, so a timeless entry folds at its create time.
    /// Each key holds the latest entry carrying it, and `null` clears it:
    /// JSON Merge Patch (RFC 7396), an array replaced whole. `comment` is
    /// gathered instead, each comment with its entry's time.
    pub fn collect(&self, field: &str) -> Result<Map<String, JsonValue>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT data, COALESCE(occur_time, create_time) FROM entries
             WHERE field = ?1 AND {LIVE_SQL} AND data IS NOT NULL
             ORDER BY COALESCE(occur_time, create_time), slug"
        ))?;
        let rows = stmt.query_map(params![field], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut record = Map::new();
        let mut comments = Vec::new();
        for row in rows {
            let (data, time) = row?;
            // Data that is no object holds no keys, and data that does not
            // parse is none, as every row read takes it (`row_mapper.rs`).
            let Ok(JsonValue::Object(data)) = serde_json::from_str(&data) else {
                continue;
            };
            for (key, value) in data {
                match (key.as_str(), value) {
                    ("comment", JsonValue::Array(said)) => comments.extend(
                        said.into_iter()
                            .map(|text| json!({"time": time, "text": text})),
                    ),
                    // Gathered, so a null clears none of them.
                    ("comment", JsonValue::Null) => {}
                    // A row from before comments were arrays holds a string.
                    ("comment", text) => comments.push(json!({"time": time, "text": text})),
                    (_, JsonValue::Null) => {
                        record.remove(&key);
                    }
                    (_, value) => {
                        record.insert(key, value);
                    }
                }
            }
        }
        if !comments.is_empty() {
            record.insert("comment".into(), comments.into());
        }
        match self.state_at(field, Utc::now())? {
            Some(state) if !state.is_null() => record.insert("state".into(), state),
            _ => record.remove("state"),
        };
        Ok(record)
    }
}
