//! `duration_sum` — sum of block durations per `(field, local_date)`.
//!
//! SQL-backed. Only entries with both `time_start` and `time_end` contribute
//! (point-in-time events have no duration). Reduces via `_sum`; the dataset
//! is scanned via `entries_field_start` (or `entries_live_field_start`),
//! with the field filter pushed down when the caller supplies an array key
//! whose first element is a field name.

use chrono::{DateTime, Utc};
use rusqlite::params_from_iter;
use serde_json::{json, Value as JsonValue};

use super::{
    apply_limit, cmp_json, field_filter_from_params, filter_by_key, group_key, View, ViewParams,
    ViewRow,
};
use crate::db::Db;
use crate::error::{DatumError, Result};

pub struct DurationSumView;

impl View for DurationSumView {
    fn name(&self) -> &str {
        "duration_sum"
    }

    fn query(&self, db: &Db, params: &ViewParams) -> Result<Vec<ViewRow>> {
        let field_filter = field_filter_from_params(params);

        let mut sql = String::from(
            "SELECT slug, field, start_time, start_time_offset, end_time
             FROM entries
             WHERE deleted_at IS NULL
               AND start_time IS NOT NULL
               AND end_time IS NOT NULL",
        );
        let mut args: Vec<String> = Vec::new();
        if let Some(f) = &field_filter {
            sql.push_str(" AND field = ?1");
            args.push(f.clone());
        }

        let mut rows: Vec<ViewRow> = Vec::new();
        let mut stmt = db.conn().prepare(&sql)?;
        let mapped = stmt.query_map(
            params_from_iter(args.iter()),
            |row| -> rusqlite::Result<(String, String, String, Option<i32>, String)> {
                Ok((
                    row.get("slug")?,
                    row.get("field")?,
                    row.get("start_time")?,
                    row.get("start_time_offset")?,
                    row.get("end_time")?,
                ))
            },
        )?;
        for r in mapped {
            let (slug, field, start_iso, start_offset, end_iso) = r.map_err(DatumError::Db)?;
            let start: DateTime<Utc> = start_iso.parse().map_err(|e: chrono::ParseError| {
                DatumError::Other(format!("bad start_time in entries row: {e}"))
            })?;
            let end: DateTime<Utc> = end_iso.parse().map_err(|e: chrono::ParseError| {
                DatumError::Other(format!("bad end_time in entries row: {e}"))
            })?;
            let dur_ms = (end - start).num_milliseconds();
            let local_date = local_date_for(&start, start_offset.unwrap_or(0));
            rows.push(ViewRow {
                key: json!([field, local_date]),
                value: json!(dur_ms),
                id: Some(slug),
            });
        }

        rows = filter_by_key(rows, params);
        rows.sort_by(|a, b| cmp_json(&a.key, &b.key));

        if params.reduce {
            rows = reduce_sum(rows, params);
        }
        Ok(apply_limit(rows, params))
    }
}

fn local_date_for(utc: &DateTime<Utc>, offset_minutes: i32) -> String {
    let offset = chrono::FixedOffset::east_opt(offset_minutes * 60)
        .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
    utc.with_timezone(&offset).format("%Y-%m-%d").to_string()
}

fn reduce_sum(rows: Vec<ViewRow>, params: &ViewParams) -> Vec<ViewRow> {
    let mut groups: Vec<(JsonValue, i64)> = Vec::new();
    for r in rows {
        let gk = group_key(&r.key, params);
        let v = r.value.as_i64().unwrap_or_else(|| {
            r.value.as_f64().map(|f| f as i64).unwrap_or(0)
        });
        if let Some(entry) = groups.iter_mut().find(|(k, _)| k == &gk) {
            entry.1 += v;
        } else {
            groups.push((gk, v));
        }
    }
    groups
        .into_iter()
        .map(|(key, sum)| ViewRow {
            key,
            value: json!(sum),
            id: None,
        })
        .collect()
}
