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
            let (end_value, dur_value) =
                match dur_iso.as_deref().and_then(crate::entry::compat_dur_secs) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Verb;
    use crate::test_util::{entry_builder, fresh_db};
    use chrono::TimeZone;

    #[test]
    fn timing_view_emits_entries_with_durations() {
        let db = fresh_db();

        let start = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 4, 20, 9, 30, 0).unwrap();
        let workout = entry_builder("workout")
            .time_start_utc(start)
            .time_end_utc(end)
            .insert(&db);

        let coffee_at = Utc.with_ymd_and_hms(2026, 4, 20, 8, 15, 0).unwrap();
        let coffee = entry_builder("coffee")
            .state(json!(1))
            .time_start_utc(coffee_at)
            .insert(&db);

        // Tombstoned entry: skipped by SQL (deleted_at IS NULL).
        let ghost = entry_builder("coffee")
            .time_start_utc(Utc.with_ymd_and_hms(2026, 4, 20, 7, 0, 0).unwrap())
            .insert(&db);
        db.soft_delete_entry(&ghost.slug).unwrap();

        let rows = TimingView.query(&db, &ViewParams::default()).unwrap();
        assert_eq!(rows.len(), 2, "one row per live entry");

        let by_id: std::collections::HashMap<_, _> =
            rows.iter().map(|r| (r.id.clone().unwrap(), r)).collect();

        let w = by_id
            .get(workout.slug.as_str())
            .expect("workout row present");
        assert_eq!(w.key, json!(["workout", "2026-04-20"]));
        assert_eq!(w.value["duration_ms"], json!(30 * 60 * 1000));
        assert_eq!(w.value["start"], json!(start.to_rfc3339()));
        assert_eq!(w.value["end"], json!(end.to_rfc3339()));

        let c = by_id.get(coffee.slug.as_str()).expect("coffee row present");
        assert_eq!(c.key, json!(["coffee", "2026-04-20"]));
        assert_eq!(c.value["duration_ms"], JsonValue::Null);
        assert_eq!(c.value["end"], JsonValue::Null);
    }

    #[test]
    fn timing_view_reduce_counts_entries() {
        let db = fresh_db();
        let day = Utc.with_ymd_and_hms(2026, 4, 20, 8, 0, 0).unwrap();
        for offset in 0..3 {
            entry_builder("coffee")
                .time_start_utc(day + chrono::Duration::minutes(offset as i64 * 30))
                .insert(&db);
        }
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("good"))
            .time_start_utc(day)
            .insert(&db);

        let reg = crate::views::builtins();
        let params = ViewParams {
            reduce: true,
            group: true,
            ..Default::default()
        };
        let rows = reg.query("datum_timing", &db, &params).unwrap();
        assert_eq!(rows.len(), 2);
        let coffee = rows
            .iter()
            .find(|r| r.key == json!(["coffee", "2026-04-20"]))
            .unwrap();
        assert_eq!(coffee.value, json!(3));
        let mood = rows
            .iter()
            .find(|r| r.key == json!(["mood", "2026-04-20"]))
            .unwrap();
        assert_eq!(mood.value, json!(1));
    }

    #[test]
    fn timing_view_pushes_field_filter_to_sql() {
        let db = fresh_db();
        let day = Utc.with_ymd_and_hms(2026, 4, 20, 8, 0, 0).unwrap();
        entry_builder("coffee").time_start_utc(day).insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("good"))
            .time_start_utc(day)
            .insert(&db);

        let params = ViewParams {
            key: Some(json!(["coffee"])),
            ..Default::default()
        };
        let rows = TimingView.query(&db, &params).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key[0], json!("coffee"));
    }

    #[test]
    fn timing_view_respects_limit() {
        let db = fresh_db();
        let day = Utc.with_ymd_and_hms(2026, 4, 20, 8, 0, 0).unwrap();
        for i in 0..5 {
            entry_builder("coffee")
                .time_start_utc(day + chrono::Duration::minutes(i as i64 * 5))
                .insert(&db);
        }
        let params = ViewParams {
            limit: Some(2),
            ..Default::default()
        };
        let rows = TimingView.query(&db, &params).unwrap();
        assert_eq!(rows.len(), 2);
    }
}
