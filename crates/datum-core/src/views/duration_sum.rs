//! `duration_sum` — sum of block durations per `(field, local_date)`.
//!
//! SQL-backed. Only entries with a non-NULL `dur` contribute (point-in-time
//! events have no duration). Reduces via `_sum`; the field filter is pushed
//! down when the caller supplies an array key whose first element is a
//! field name.

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
            "SELECT slug, field, occur_time, occur_time_offset, dur
             FROM entries
             WHERE deleted_at IS NULL
               AND dur IS NOT NULL",
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
                    row.get("occur_time")?,
                    row.get("occur_time_offset")?,
                    row.get("dur")?,
                ))
            },
        )?;
        for r in mapped {
            let (slug, field, start_iso, start_offset, dur_iso) = r.map_err(DatumError::Db)?;
            let start: DateTime<Utc> = start_iso.parse().map_err(|e: chrono::ParseError| {
                DatumError::Other(format!("bad occur_time in entries row: {e}"))
            })?;
            let Some(secs) = crate::entry::compat_dur_secs(&dur_iso) else {
                continue;
            };
            let dur_ms = secs.saturating_mul(1000);
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
        let v = r
            .value
            .as_i64()
            .unwrap_or_else(|| r.value.as_f64().map(|f| f as i64).unwrap_or(0));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{entry_builder, fresh_db};
    use crate::views::{builtins, ViewParams};
    use chrono::{Duration, TimeZone};

    #[test]
    fn duration_sum_view_reduces_by_field_and_day() {
        let db = fresh_db();

        let morning = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();
        let afternoon = Utc.with_ymd_and_hms(2026, 4, 20, 14, 0, 0).unwrap();
        let next_day = Utc.with_ymd_and_hms(2026, 4, 21, 9, 0, 0).unwrap();

        entry_builder("workout")
            .time_start_utc(morning)
            .time_end_utc(morning + Duration::minutes(30))
            .insert(&db);
        entry_builder("workout")
            .time_start_utc(afternoon)
            .time_end_utc(afternoon + Duration::minutes(45))
            .insert(&db);
        entry_builder("workout")
            .time_start_utc(next_day)
            .time_end_utc(next_day + Duration::minutes(60))
            .insert(&db);

        entry_builder("reading")
            .time_start_utc(morning)
            .time_end_utc(morning + Duration::minutes(20))
            .insert(&db);

        // Entry without dur: skipped by SQL.
        entry_builder("coffee").time_start_utc(morning).insert(&db);

        let reg = builtins();

        let params = ViewParams {
            reduce: true,
            group: true,
            ..Default::default()
        };
        let rows = reg.query("duration_sum", &db, &params).unwrap();
        let by_key: std::collections::HashMap<_, _> = rows
            .iter()
            .map(|r| (r.key.clone(), r.value.clone()))
            .collect();
        assert_eq!(
            by_key[&json!(["workout", "2026-04-20"])],
            json!((30 + 45) * 60 * 1000)
        );
        assert_eq!(
            by_key[&json!(["workout", "2026-04-21"])],
            json!(60 * 60 * 1000)
        );
        assert_eq!(
            by_key[&json!(["reading", "2026-04-20"])],
            json!(20 * 60 * 1000)
        );
        assert_eq!(by_key.len(), 3);

        let params = ViewParams {
            reduce: true,
            group_level: Some(1),
            ..Default::default()
        };
        let rows = reg.query("duration_sum", &db, &params).unwrap();
        let by_key: std::collections::HashMap<_, _> = rows
            .iter()
            .map(|r| (r.key.clone(), r.value.clone()))
            .collect();
        assert_eq!(
            by_key[&json!(["workout"])],
            json!((30 + 45 + 60) * 60 * 1000)
        );
        assert_eq!(by_key[&json!(["reading"])], json!(20 * 60 * 1000));
        assert_eq!(by_key.len(), 2);

        let params = ViewParams {
            reduce: true,
            ..Default::default()
        };
        let rows = reg.query("duration_sum", &db, &params).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, JsonValue::Null);
        assert_eq!(rows[0].value, json!((30 + 45 + 60 + 20) * 60 * 1000));
    }

    #[test]
    fn duration_sum_pushes_field_filter() {
        let db = fresh_db();
        let t0 = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();
        entry_builder("workout")
            .time_start_utc(t0)
            .time_end_utc(t0 + Duration::minutes(30))
            .insert(&db);
        entry_builder("reading")
            .time_start_utc(t0)
            .time_end_utc(t0 + Duration::minutes(20))
            .insert(&db);

        let params = ViewParams {
            key: Some(json!(["workout"])),
            reduce: true,
            group_level: Some(1),
            ..Default::default()
        };
        let rows = DurationSumView.query(&db, &params).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, json!(["workout"]));
        assert_eq!(rows[0].value, json!(30 * 60 * 1000));
    }
}
