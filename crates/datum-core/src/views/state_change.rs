//! `state_change` — one row per state transition `(field, time)` → `{from, to}`.
//!
//! Ported from datum's `stateChangeView`. That implementation walks one doc
//! at a time and leans on explicit `lastState` / `dur` fields. We derive the
//! same info from our entry model:
//!
//! * Per field, order entries chronologically by `time_start`.
//! * For entries whose verb changes state (`switch`, `start`, `stop`, `occur`),
//!   emit a transition at `time_start` from the rolling prior state to the
//!   new state; when `time_end` is set, emit a closing transition back to
//!   "off" (Bool(false)) — mirroring `state::collect_events`.
//! * `note` entries are skipped.

use chrono::{DateTime, Utc};
use serde_json::{json, Value as JsonValue};

use super::{View, ViewRow};
use crate::db::Db;
use crate::entry::{Entry, Verb};
use crate::error::Result;

pub struct StateChangeView;

impl View for StateChangeView {
    fn name(&self) -> &str {
        "state_change"
    }

    fn map(&self, db: &Db) -> Result<Vec<ViewRow>> {
        let entries = db.all_entries()?;

        // Bucket events per field so we can walk prior state independently.
        let mut by_field: std::collections::BTreeMap<String, Vec<PendingEvent>> =
            std::collections::BTreeMap::new();
        for entry in &entries {
            if entry.deleted_at.is_some() {
                continue;
            }
            for ev in events_for_entry(entry) {
                by_field
                    .entry(entry.field.clone())
                    .or_default()
                    .push(ev);
            }
        }

        let mut rows = Vec::new();
        for (field, mut events) in by_field {
            events.sort_by(|a, b| a.time.cmp(&b.time).then(a.created_at.cmp(&b.created_at)));
            let mut prior = JsonValue::Null;
            for ev in events {
                rows.push(ViewRow {
                    key: json!([field.clone(), ev.time.to_rfc3339()]),
                    value: json!({"from": prior.clone(), "to": ev.new_state.clone()}),
                    id: Some(ev.entry_id.clone()),
                });
                prior = ev.new_state;
            }
        }
        Ok(rows)
    }

    fn supports_reduce(&self) -> bool {
        true
    }

    /// `_count` in datum speak.
    fn reduce(&self, _key: &JsonValue, values: &[JsonValue]) -> Result<JsonValue> {
        Ok(json!(values.len()))
    }
}

struct PendingEvent {
    time: DateTime<Utc>,
    created_at: DateTime<Utc>,
    new_state: JsonValue,
    entry_id: String,
}

fn events_for_entry(entry: &Entry) -> Vec<PendingEvent> {
    let mut out = Vec::new();
    match entry.verb {
        Verb::Switch => {
            if let Some(ts) = &entry.start_time {
                let state = entry
                    .value
                    .clone()
                    .map(|v| crate::state::normalize_state(&v))
                    .unwrap_or(JsonValue::Null);
                out.push(PendingEvent {
                    time: ts.utc,
                    created_at: entry.create_time,
                    new_state: state,
                    entry_id: entry.slug.clone(),
                });
                if let Some(te) = &entry.end_time {
                    if te.utc > ts.utc {
                        out.push(PendingEvent {
                            time: te.utc,
                            created_at: entry.create_time,
                            new_state: JsonValue::Bool(false),
                            entry_id: entry.slug.clone(),
                        });
                    }
                }
            }
        }
        Verb::Start => {
            if let Some(ts) = &entry.start_time {
                out.push(PendingEvent {
                    time: ts.utc,
                    created_at: entry.create_time,
                    new_state: JsonValue::Bool(true),
                    entry_id: entry.slug.clone(),
                });
                if let Some(te) = &entry.end_time {
                    if te.utc > ts.utc {
                        out.push(PendingEvent {
                            time: te.utc,
                            created_at: entry.create_time,
                            new_state: JsonValue::Bool(false),
                            entry_id: entry.slug.clone(),
                        });
                    }
                }
            }
        }
        Verb::Stop => {
            let at = entry.end_time.as_ref().or(entry.start_time.as_ref());
            if let Some(t) = at {
                out.push(PendingEvent {
                    time: t.utc,
                    created_at: entry.create_time,
                    new_state: JsonValue::Bool(false),
                    entry_id: entry.slug.clone(),
                });
            }
        }
        Verb::Occur => {
            if let Some(ts) = &entry.start_time {
                out.push(PendingEvent {
                    time: ts.utc,
                    created_at: entry.create_time,
                    new_state: JsonValue::Bool(false),
                    entry_id: entry.slug.clone(),
                });
            }
        }
        Verb::Block => {
            if let Some(ts) = &entry.start_time {
                let state = entry
                    .value
                    .clone()
                    .map(|v| crate::state::normalize_state(&v))
                    .unwrap_or(JsonValue::Bool(true));
                out.push(PendingEvent {
                    time: ts.utc,
                    created_at: entry.create_time,
                    new_state: state,
                    entry_id: entry.slug.clone(),
                });
                if let Some(te) = &entry.end_time {
                    if te.utc > ts.utc {
                        out.push(PendingEvent {
                            time: te.utc,
                            created_at: entry.create_time,
                            new_state: JsonValue::Bool(false),
                            entry_id: entry.slug.clone(),
                        });
                    }
                }
            }
        }
        Verb::Note | Verb::Add => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{entry_builder, fresh_db};
    use chrono::{Duration, TimeZone};

    #[test]
    fn state_change_view_emits_transitions() {
        let db = fresh_db();

        let t0 = Utc.with_ymd_and_hms(2026, 4, 20, 8, 0, 0).unwrap();
        let t1 = t0 + Duration::minutes(30);
        let t2 = t0 + Duration::hours(2);
        let t3 = t0 + Duration::hours(3);

        // mood: switch "focused" at t0, then switch "tired" at t2, then stop at t3.
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("focused"))
            .time_start_utc(t0)
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("tired"))
            .time_start_utc(t2)
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Stop)
            .time_start_utc(t3)
            .insert(&db);

        // workout: start → time_end bounded block at t1 for 45m.
        entry_builder("workout")
            .verb(Verb::Start)
            .time_start_utc(t1)
            .time_end_utc(t1 + Duration::minutes(45))
            .insert(&db);

        // A note — never emits.
        entry_builder("word")
            .verb(Verb::Note)
            .no_time()
            .insert(&db);

        let rows = StateChangeView.map(&db).unwrap();

        // Expected rows, sorted by (field asc, time asc):
        //   ("mood", t0):   null → "focused"
        //   ("mood", t2):   "focused" → "tired"
        //   ("mood", t3):   "tired" → false
        //   ("workout", t1):       null → true
        //   ("workout", t1+45min): true → false
        assert_eq!(rows.len(), 5);

        let mood: Vec<_> = rows
            .iter()
            .filter(|r| r.key[0] == json!("mood"))
            .collect();
        assert_eq!(mood.len(), 3);
        assert_eq!(mood[0].value, json!({"from": null, "to": "focused"}));
        assert_eq!(mood[1].value, json!({"from": "focused", "to": "tired"}));
        assert_eq!(mood[2].value, json!({"from": "tired", "to": false}));
        assert_eq!(mood[0].key[1], json!(t0.to_rfc3339()));
        assert_eq!(mood[1].key[1], json!(t2.to_rfc3339()));
        assert_eq!(mood[2].key[1], json!(t3.to_rfc3339()));

        let workout: Vec<_> = rows
            .iter()
            .filter(|r| r.key[0] == json!("workout"))
            .collect();
        assert_eq!(workout.len(), 2);
        assert_eq!(workout[0].value, json!({"from": null, "to": true}));
        assert_eq!(workout[1].value, json!({"from": true, "to": false}));
    }

    #[test]
    fn state_change_view_skips_tombstoned_and_noteonly() {
        let db = fresh_db();
        let t0 = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();

        let ghost = entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("ephemeral"))
            .time_start_utc(t0)
            .insert(&db);
        db.soft_delete_entry(&ghost.slug).unwrap();

        entry_builder("word").verb(Verb::Note).no_time().insert(&db);

        let rows = StateChangeView.map(&db).unwrap();
        assert!(rows.is_empty());
    }
}
