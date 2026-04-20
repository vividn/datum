//! `datum_timing` — one row per live entry with `(field, local_date, start, end, duration_ms)`.
//!
//! Ported from datum's `timingView`. The datum version emits separate rows
//! per time metric (occur/create/modify/hybrid); we collapse to occur-time
//! only (our canonical `time_start`) and instead carry the entry's end and
//! duration in the value. The per-metric split is deferred until a caller
//! asks for it.

use serde_json::{json, Value as JsonValue};

use super::{View, ViewRow};
use crate::db::Db;
use crate::error::Result;
use crate::time::DatumTime;

pub struct TimingView;

impl View for TimingView {
    fn name(&self) -> &str {
        "datum_timing"
    }

    fn map(&self, db: &Db) -> Result<Vec<ViewRow>> {
        let entries = db.all_entries()?;
        let mut rows = Vec::with_capacity(entries.len());
        for entry in entries {
            if entry.deleted_at.is_some() {
                continue;
            }
            let Some(ts) = &entry.time_start else {
                continue;
            };
            let local_date = local_date_str(ts);
            let start_iso = ts.utc.to_rfc3339();
            let (end_iso, duration_ms) = match &entry.time_end {
                Some(te) => {
                    let dur = (te.utc - ts.utc).num_milliseconds();
                    (json!(te.utc.to_rfc3339()), json!(dur))
                }
                None => (JsonValue::Null, JsonValue::Null),
            };
            rows.push(ViewRow {
                key: json!([entry.field.clone(), local_date]),
                value: json!({
                    "start": start_iso,
                    "end": end_iso,
                    "duration_ms": duration_ms,
                }),
                id: Some(entry.slug.clone()),
            });
        }
        Ok(rows)
    }

    fn supports_reduce(&self) -> bool {
        true
    }

    /// `_count` in datum speak — one per map row in the group.
    fn reduce(&self, _key: &JsonValue, values: &[JsonValue]) -> Result<JsonValue> {
        Ok(json!(values.len()))
    }
}

/// Local-date string (`YYYY-MM-DD`) for a `DatumTime`, using its stored
/// `offset_minutes` so entries logged in different timezones land on the
/// correct civic day.
fn local_date_str(t: &DatumTime) -> String {
    t.local_time().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Verb;
    use crate::test_util::{entry_builder, fresh_db};
    use chrono::{TimeZone, Utc};

    #[test]
    fn timing_view_emits_entries_with_durations() {
        let db = fresh_db();

        // Bounded block: 30 min workout.
        let start = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 4, 20, 9, 30, 0).unwrap();
        let workout = entry_builder("workout")
            .verb(Verb::Start)
            .time_start_utc(start)
            .time_end_utc(end)
            .insert(&db);

        // Point-in-time occur.
        let coffee_at = Utc.with_ymd_and_hms(2026, 4, 20, 8, 15, 0).unwrap();
        let coffee = entry_builder("coffee")
            .verb(Verb::Occur)
            .value(serde_json::json!(1))
            .time_start_utc(coffee_at)
            .insert(&db);

        // Note with no time_start: skipped.
        entry_builder("word")
            .verb(Verb::Note)
            .no_time()
            .insert(&db);

        // Tombstoned entry: skipped.
        let ghost = entry_builder("coffee")
            .verb(Verb::Occur)
            .time_start_utc(Utc.with_ymd_and_hms(2026, 4, 20, 7, 0, 0).unwrap())
            .insert(&db);
        db.soft_delete_entry(&ghost.slug).unwrap();

        let rows = TimingView.map(&db).unwrap();
        assert_eq!(rows.len(), 2, "one row per live, timestamped entry");

        let by_id: std::collections::HashMap<_, _> =
            rows.iter().map(|r| (r.id.clone().unwrap(), r)).collect();

        let w = by_id.get(&workout.slug).expect("workout row present");
        assert_eq!(w.key, serde_json::json!(["workout", "2026-04-20"]));
        assert_eq!(w.value["duration_ms"], serde_json::json!(30 * 60 * 1000));
        assert_eq!(w.value["start"], serde_json::json!(start.to_rfc3339()));
        assert_eq!(w.value["end"], serde_json::json!(end.to_rfc3339()));

        let c = by_id.get(&coffee.slug).expect("coffee row present");
        assert_eq!(c.key, serde_json::json!(["coffee", "2026-04-20"]));
        assert_eq!(c.value["duration_ms"], JsonValue::Null);
        assert_eq!(c.value["end"], JsonValue::Null);
    }

    #[test]
    fn timing_view_reduce_counts_entries() {
        let db = fresh_db();
        let day = Utc.with_ymd_and_hms(2026, 4, 20, 8, 0, 0).unwrap();
        for offset in 0..3 {
            entry_builder("coffee")
                .verb(Verb::Occur)
                .time_start_utc(day + chrono::Duration::minutes(offset as i64 * 30))
                .insert(&db);
        }
        // Different field, same day — separate group.
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(serde_json::json!("good"))
            .time_start_utc(day)
            .insert(&db);

        let reg = crate::views::builtins();
        let params = crate::views::ViewParams {
            reduce: true,
            group: true,
            ..Default::default()
        };
        let rows = reg.query("datum_timing", &db, &params).unwrap();
        assert_eq!(rows.len(), 2);
        let coffee = rows
            .iter()
            .find(|r| r.key == serde_json::json!(["coffee", "2026-04-20"]))
            .unwrap();
        assert_eq!(coffee.value, serde_json::json!(3));
        let mood = rows
            .iter()
            .find(|r| r.key == serde_json::json!(["mood", "2026-04-20"]))
            .unwrap();
        assert_eq!(mood.value, serde_json::json!(1));
    }
}

