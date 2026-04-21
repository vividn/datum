//! `duration_sum` — sum of block durations per `(field, local_date)`.
//!
//! Only entries with both `time_start` and `time_end` contribute (point-in-time
//! events have no duration). Reduces via `_sum`; group by full key for
//! per-day totals, or `group_level=1` for per-field totals, or un-grouped for
//! an overall total.

use serde_json::{json, Value as JsonValue};

use super::{View, ViewRow};
use crate::db::Db;
use crate::error::Result;
use crate::time::DatumTime;

pub struct DurationSumView;

impl View for DurationSumView {
    fn name(&self) -> &str {
        "duration_sum"
    }

    fn map(&self, db: &Db) -> Result<Vec<ViewRow>> {
        let entries = db.all_entries()?;
        let mut rows = Vec::new();
        for entry in entries {
            if entry.deleted_at.is_some() {
                continue;
            }
            let (Some(ts), Some(te)) = (&entry.start_time, &entry.end_time) else {
                continue;
            };
            let dur_ms = (te.utc - ts.utc).num_milliseconds();
            // Negative or zero durations are legal (datum emits corrective
            // rows with -seconds for "hole" blocks), but for Phase 1 we only
            // surface the wall-clock duration and let negatives cancel out
            // when summed.
            rows.push(ViewRow {
                key: json!([entry.field.clone(), local_date_str(ts)]),
                value: json!(dur_ms),
                id: Some(entry.slug.clone()),
            });
        }
        Ok(rows)
    }

    fn supports_reduce(&self) -> bool {
        true
    }

    fn reduce(&self, _key: &JsonValue, values: &[JsonValue]) -> Result<JsonValue> {
        let sum: i64 = values
            .iter()
            .filter_map(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
            .sum();
        Ok(json!(sum))
    }
}

fn local_date_str(t: &DatumTime) -> String {
    t.local_time().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Verb;
    use crate::test_util::{entry_builder, fresh_db};
    use crate::views::{builtins, ViewParams};
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn duration_sum_view_reduces_by_field_and_day() {
        let db = fresh_db();

        let morning = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();
        let afternoon = Utc.with_ymd_and_hms(2026, 4, 20, 14, 0, 0).unwrap();
        let next_day = Utc.with_ymd_and_hms(2026, 4, 21, 9, 0, 0).unwrap();

        // 30-min + 45-min workouts on day 1; 60-min on day 2.
        entry_builder("workout")
            .verb(Verb::Start)
            .time_start_utc(morning)
            .time_end_utc(morning + Duration::minutes(30))
            .insert(&db);
        entry_builder("workout")
            .verb(Verb::Start)
            .time_start_utc(afternoon)
            .time_end_utc(afternoon + Duration::minutes(45))
            .insert(&db);
        entry_builder("workout")
            .verb(Verb::Start)
            .time_start_utc(next_day)
            .time_end_utc(next_day + Duration::minutes(60))
            .insert(&db);

        // Different field, same day: separate group.
        entry_builder("reading")
            .verb(Verb::Start)
            .time_start_utc(morning)
            .time_end_utc(morning + Duration::minutes(20))
            .insert(&db);

        // Entry without time_end: skipped (no duration).
        entry_builder("coffee")
            .verb(Verb::Occur)
            .time_start_utc(morning)
            .insert(&db);

        let reg = builtins();

        // Grouped by full key (field, date).
        let params = ViewParams {
            reduce: true,
            group: true,
            ..Default::default()
        };
        let rows = reg.query("duration_sum", &db, &params).unwrap();
        let by_key: std::collections::HashMap<_, _> =
            rows.iter().map(|r| (r.key.clone(), r.value.clone())).collect();
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

        // group_level=1 collapses to per-field totals.
        let params = ViewParams {
            reduce: true,
            group_level: Some(1),
            ..Default::default()
        };
        let rows = reg.query("duration_sum", &db, &params).unwrap();
        let by_key: std::collections::HashMap<_, _> =
            rows.iter().map(|r| (r.key.clone(), r.value.clone())).collect();
        assert_eq!(by_key[&json!(["workout"])], json!((30 + 45 + 60) * 60 * 1000));
        assert_eq!(by_key[&json!(["reading"])], json!(20 * 60 * 1000));
        assert_eq!(by_key.len(), 2);

        // Un-grouped reduce: overall total.
        let params = ViewParams {
            reduce: true,
            ..Default::default()
        };
        let rows = reg.query("duration_sum", &db, &params).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].key, JsonValue::Null);
        assert_eq!(rows[0].value, json!((30 + 45 + 60 + 20) * 60 * 1000));
    }
}
