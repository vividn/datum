//! `state_change` — one row per state transition `(field, time)` → `{from, to}`.
//!
//! SQL-backed scan. Ported from datum's `stateChangeView`, which walks one
//! doc at a time and leans on explicit `lastState` / `dur` fields. We derive
//! the same info from our entry model:
//!
//! * Fetch live entries per field from SQL (indexed via `entries_field_start`
//!   when the caller pushes a field filter down, otherwise a live-only scan).
//! * For entries whose verb changes state (`switch`, `start`, `stop`, `occur`,
//!   `block`), emit a transition at `time_start` from the rolling prior state
//!   to the new state; when `time_end` is set, emit a closing transition back
//!   to "off" (`false`) — mirroring `state::collect_events`.
//! * `add` entries are skipped.

use chrono::{DateTime, Utc};
use rusqlite::params_from_iter;
use serde_json::{json, Value as JsonValue};

use super::{
    apply_limit, cmp_json, field_filter_from_params, filter_by_key, group_key, View, ViewParams,
    ViewRow,
};
use crate::db::Db;
use crate::entry::Verb;
use crate::error::{DatumError, Result};

pub struct StateChangeView;

impl View for StateChangeView {
    fn name(&self) -> &str {
        "state_change"
    }

    fn query(&self, db: &Db, params: &ViewParams) -> Result<Vec<ViewRow>> {
        let field_filter = field_filter_from_params(params);

        // Only verbs that can mutate state surface here.
        let mut sql = String::from(
            "SELECT slug, field, verb, state, occur_time, dur, create_time
             FROM entries
             WHERE deleted_at IS NULL
               AND verb IN ('switch','block','point','block-switch','point-switch')",
        );
        let mut args: Vec<String> = Vec::new();
        if let Some(f) = &field_filter {
            sql.push_str(" AND field = ?1");
            args.push(f.clone());
        }
        sql.push_str(" ORDER BY field ASC, occur_time ASC, create_time ASC");

        // Bucket candidate events per field so rolling-prior-state is isolated
        // per field. SQL orders within a field but we fan out to multiple
        // events per entry (start + optional end), so we sort again below.
        let mut by_field: std::collections::BTreeMap<String, Vec<PendingEvent>> =
            std::collections::BTreeMap::new();
        let mut stmt = db.conn().prepare(&sql)?;
        let mapped = stmt.query_map(
            params_from_iter(args.iter()),
            |row| -> rusqlite::Result<CandidateRow> {
                let occur: Option<String> = row.get("occur_time")?;
                let dur: Option<String> = row.get("dur")?;
                let end = match (occur.as_deref(), dur.as_deref()) {
                    (Some(o), Some(d)) => crate::entry::compat_dur_secs(d).and_then(|secs| {
                        o.parse::<DateTime<Utc>>()
                            .ok()
                            .map(|t| (t + chrono::Duration::seconds(secs)).to_rfc3339())
                    }),
                    _ => None,
                };
                Ok(CandidateRow {
                    slug: row.get("slug")?,
                    field: row.get("field")?,
                    verb: row.get("verb")?,
                    value: row.get::<_, Option<String>>("state")?,
                    start_time: occur,
                    end_time: end,
                    create_time: row.get("create_time")?,
                })
            },
        )?;
        for r in mapped {
            let row = r.map_err(DatumError::Db)?;
            let field = row.field.clone();
            for ev in events_for_row(&row)? {
                by_field.entry(field.clone()).or_default().push(ev);
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

        rows = filter_by_key(rows, params);
        rows.sort_by(|a, b| cmp_json(&a.key, &b.key));

        if params.reduce {
            rows = reduce_count(rows, params);
        }
        Ok(apply_limit(rows, params))
    }
}

struct CandidateRow {
    slug: String,
    field: String,
    verb: String,
    value: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    create_time: String,
}

struct PendingEvent {
    time: DateTime<Utc>,
    created_at: DateTime<Utc>,
    new_state: JsonValue,
    entry_id: String,
}

fn parse_utc(s: &str) -> Result<DateTime<Utc>> {
    s.parse::<DateTime<Utc>>()
        .map_err(|e| DatumError::Other(format!("bad timestamp in entries row: {e}")))
}

fn parse_verb(s: &str) -> Result<Verb> {
    s.parse::<Verb>()
        .map_err(|e| DatumError::Other(format!("unknown verb '{s}' in entries row: {e}")))
}

fn parse_value(v: &Option<String>) -> Option<JsonValue> {
    v.as_deref().and_then(|s| serde_json::from_str(s).ok())
}

fn events_for_row(row: &CandidateRow) -> Result<Vec<PendingEvent>> {
    let mut out = Vec::new();
    let verb = parse_verb(&row.verb)?;
    let created_at = parse_utc(&row.create_time)?;
    let start = match &row.start_time {
        Some(s) => Some(parse_utc(s)?),
        None => None,
    };
    let end = match &row.end_time {
        Some(s) => Some(parse_utc(s)?),
        None => None,
    };
    let value = parse_value(&row.value);

    match verb {
        Verb::Point => {
            if let Some(ts) = start {
                let state = value
                    .as_ref()
                    .map(|v| crate::state::normalize_state(v).to_json())
                    .unwrap_or(JsonValue::Null);
                out.push(PendingEvent {
                    time: ts,
                    created_at,
                    new_state: state,
                    entry_id: row.slug.clone(),
                });
            }
        }
        Verb::BlockSwitch | Verb::PointSwitch => {
            // Sub-bead 5 expands this with the trailing `following_state`
            // event; for now treat like Switch.
            if let Some(ts) = start {
                let state = value
                    .as_ref()
                    .map(|v| crate::state::normalize_state(v).to_json())
                    .unwrap_or(JsonValue::Null);
                out.push(PendingEvent {
                    time: ts,
                    created_at,
                    new_state: state,
                    entry_id: row.slug.clone(),
                });
            }
        }
        Verb::Switch => {
            if let Some(ts) = start {
                let state = value
                    .as_ref()
                    .map(|v| crate::state::normalize_state(v).to_json())
                    .unwrap_or(JsonValue::Null);
                out.push(PendingEvent {
                    time: ts,
                    created_at,
                    new_state: state,
                    entry_id: row.slug.clone(),
                });
                if let Some(te) = end {
                    if te > ts {
                        out.push(PendingEvent {
                            time: te,
                            created_at,
                            new_state: JsonValue::Bool(false),
                            entry_id: row.slug.clone(),
                        });
                    }
                }
            }
        }
        Verb::Block => {
            if let Some(ts) = start {
                let state = value
                    .as_ref()
                    .map(|v| crate::state::normalize_state(v).to_json())
                    .unwrap_or(JsonValue::Bool(true));
                out.push(PendingEvent {
                    time: ts,
                    created_at,
                    new_state: state,
                    entry_id: row.slug.clone(),
                });
                if let Some(te) = end {
                    if te > ts {
                        out.push(PendingEvent {
                            time: te,
                            created_at,
                            new_state: JsonValue::Bool(false),
                            entry_id: row.slug.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(out)
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
        // v3: legacy Stop is expressed as Switch state=false.
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!(false))
            .time_start_utc(t3)
            .insert(&db);

        // Block-shaped workout entry: occur_time + dur → open + close events.
        entry_builder("workout")
            .time_start_utc(t1)
            .time_end_utc(t1 + Duration::minutes(45))
            .insert(&db);

        let rows = StateChangeView.query(&db, &ViewParams::default()).unwrap();

        assert_eq!(rows.len(), 5);

        let mood: Vec<_> = rows.iter().filter(|r| r.key[0] == json!("mood")).collect();
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
    fn state_change_view_skips_tombstoned() {
        let db = fresh_db();
        let t0 = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();

        let ghost = entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("ephemeral"))
            .time_start_utc(t0)
            .insert(&db);
        db.soft_delete_entry(&ghost.slug).unwrap();

        let rows = StateChangeView.query(&db, &ViewParams::default()).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn state_change_pushes_field_filter() {
        let db = fresh_db();
        let t0 = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("focused"))
            .time_start_utc(t0)
            .insert(&db);
        entry_builder("workout")
            .time_start_utc(t0)
            .time_end_utc(t0 + Duration::minutes(30))
            .insert(&db);

        let params = ViewParams {
            key: Some(json!(["workout"])),
            ..Default::default()
        };
        let rows = StateChangeView.query(&db, &params).unwrap();
        // Both transitions belong to workout (open + close).
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.key[0] == json!("workout")));
    }
}
