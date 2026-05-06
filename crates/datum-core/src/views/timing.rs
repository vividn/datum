//! `datum_timing` — one row per live entry with `(field, local_date, start, end, duration_ms)`.
//!
//! SQL-backed. Ported from datum's `timingView` — which emits per time-metric
//! (hybrid/occur/create/modify) — with the same simplification as before:
//! one row per live timestamped entry keyed on occur-time (our canonical
//! `time_start`), with end and duration carried in the value. The per-metric
//! split is deferred until a caller asks for it.
//!
//! Scan: `entries WHERE deleted_at IS NULL AND start_time IS NOT NULL`,
//! index-backed via `entries_field_start` when the caller pushes a field
//! filter down through `--key`/`--startkey`.

use chrono::{DateTime, Utc};
use rusqlite::params_from_iter;
use serde_json::{json, Value as JsonValue};

use super::{
    apply_limit, cmp_json, field_filter_from_params, filter_by_key, group_key, View, ViewParams,
    ViewRow,
};
use crate::db::Db;
use crate::error::{DatumError, Result};

pub struct TimingView;

/// SQL row tuple for `TimingView::query`:
/// `(slug, field, start_time, start_time_offset, end_time)`.
type TimingRow = (String, String, String, Option<i32>, Option<String>);

impl View for TimingView {
    fn name(&self) -> &str {
        "datum_timing"
    }

    fn query(&self, db: &Db, params: &ViewParams) -> Result<Vec<ViewRow>> {
        let field_filter = field_filter_from_params(params);

        let mut sql = String::from(
            "SELECT slug, field, occur_time, occur_time_offset, dur
             FROM entries
             WHERE deleted_at IS NULL",
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
            |row| -> rusqlite::Result<TimingRow> {
                Ok((
                    row.get("slug")?,
                    row.get("field")?,
                    row.get("occur_time")?,
                    row.get("occur_time_offset")?,
                    row.get::<_, Option<String>>("dur")?,
                ))
            },
        )?;
        for r in mapped {
            let (slug, field, start_iso, start_offset, dur_iso) = r.map_err(DatumError::Db)?;
            let start: DateTime<Utc> = start_iso.parse().map_err(|e: chrono::ParseError| {
                DatumError::Other(format!("bad occur_time in entries row: {e}"))
            })?;
            let local_date = local_date_for(&start, start_offset.unwrap_or(0));
            let (end_value, dur_value) = match dur_iso
                .as_deref()
                .and_then(crate::entry::compat_dur_secs)
            {
                Some(secs) => {
                    let end = start + chrono::Duration::seconds(secs);
                    let ms = secs.saturating_mul(1000);
                    (json!(end.to_rfc3339()), json!(ms))
                }
                None => (JsonValue::Null, JsonValue::Null),
            };
            rows.push(ViewRow {
                key: json!([field, local_date]),
                value: json!({
                    "start": start.to_rfc3339(),
                    "end": end_value,
                    "duration_ms": dur_value,
                }),
                id: Some(slug),
            });
        }

        // Residual key filter (date component and arbitrary range).
        rows = filter_by_key(rows, params);
        rows.sort_by(|a, b| cmp_json(&a.key, &b.key));

        if params.reduce {
            rows = reduce_count(rows, params);
        }
        Ok(apply_limit(rows, params))
    }
}

/// Local-date string (`YYYY-MM-DD`) from a UTC timestamp + offset minutes.
fn local_date_for(utc: &DateTime<Utc>, offset_minutes: i32) -> String {
    let offset = chrono::FixedOffset::east_opt(offset_minutes * 60)
        .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
    utc.with_timezone(&offset).format("%Y-%m-%d").to_string()
}

/// Reduce via `_count`: one row per group containing the row count. Groups
/// come from [`group_key`]; rows are emitted in ascending group-key order.
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
