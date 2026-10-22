//! `state_change` — one row per state transition `(field, time)` → `{from, to}`.
//!
//! Ported from datum's `stateChangeView`. The transitions are the field's
//! state as the one oracle reads it ([`crate::state::FieldStateRows::changes`],
//! da-u5l1o) — a block's end hands its field back, a compound switch's
//! end switches it to its `following_state` — with each row's `id` the
//! slug of the entry whose edge it is.

use serde_json::{json, Value as JsonValue};

use super::{
    apply_limit, cmp_json, field_filter_from_params, filter_by_key, reduce_count, View, ViewParams,
    ViewRow,
};
use crate::db::Db;
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::state::{normalize_state, FieldStateRows};

pub struct StateChangeView;

impl View for StateChangeView {
    fn name(&self) -> &str {
        "state_change"
    }

    fn query(&self, db: &Db, params: &ViewParams) -> Result<Vec<ViewRow>> {
        let field_filter = field_filter_from_params(params);
        let mut sql = format!(
            "SELECT * FROM entries WHERE {}",
            crate::db::STATE_PARTICIPATION_SQL
        );
        if field_filter.is_some() {
            sql.push_str(" AND field = ?1");
        }
        sql.push_str(&format!(
            " ORDER BY field, {}",
            crate::db::STATE_ORDER_OLDEST_FIRST
        ));
        let mut stmt = db.conn().prepare(&sql)?;
        let entries = stmt
            .query_map(
                rusqlite::params_from_iter(field_filter.iter()),
                crate::db::row_to_entry_public,
            )?
            .collect::<rusqlite::Result<Vec<Entry>>>()
            .map_err(DatumError::Db)?;

        let mut rows = Vec::new();
        for entries in entries.chunk_by(|a, b| a.field == b.field) {
            let field = &entries[0].field;
            let slugs: std::collections::HashMap<&str, String> = entries
                .iter()
                .map(|e| (e.hid.as_str(), e.slug.to_string()))
                .collect();
            let mut prior = JsonValue::Null;
            for change in FieldStateRows::from_entries(entries).changes() {
                let to = normalize_state(&change.state).to_json();
                if to == prior {
                    continue;
                }
                rows.push(ViewRow {
                    key: json!([field, change.at.to_rfc3339()]),
                    value: json!({"from": prior, "to": to}),
                    id: Some(slugs[change.row.id.as_str()].clone()),
                });
                prior = to;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Verb;
    use crate::test_util::{entry_builder, fresh_db};
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn state_change_view_emits_transitions() {
        let db = fresh_db();

        let t0 = Utc.with_ymd_and_hms(2026, 4, 20, 8, 0, 0).unwrap();
        let t1 = t0 + Duration::minutes(30);
        let t2 = t0 + Duration::hours(2);
        let t3 = t0 + Duration::hours(3);

        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("focused"))
            .time_start_utc(t0)
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("tired"))
            .time_start_utc(t2)
            .insert(&db);
        // v3: legacy Stop is expressed as Switch state=false.
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!(false))
            .time_start_utc(t3)
            .insert(&db);

        // A block: open at its leading edge, and at its end the field is
        // handed back — to no state, as nothing preceded it.
        entry_builder("workout")
            .state(json!(true))
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
        assert_eq!(workout[1].value, json!({"from": true, "to": null}));
        // The builder stores `time_start` as the block's END (occur_time).
        assert_eq!(workout[1].key[1], json!(t1.to_rfc3339()));
    }

    /// A row per TRANSITION: a switch to the state the field already
    /// holds changes nothing, so it adds no row (and no reduce count).
    #[test]
    fn state_change_view_emits_no_row_for_a_repeat() {
        let db = fresh_db();
        let t0 = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();
        for (mins, state) in [(0, "happy"), (10, "happy"), (20, "sad")] {
            entry_builder("mood")
                .verb(Verb::Switch)
                .state(json!(state))
                .time_start_utc(t0 + Duration::minutes(mins))
                .insert(&db);
        }
        let rows = StateChangeView.query(&db, &ViewParams::default()).unwrap();
        let values: Vec<_> = rows.iter().map(|r| r.value.clone()).collect();
        assert_eq!(
            values,
            vec![
                json!({"from": null, "to": "happy"}),
                json!({"from": "happy", "to": "sad"}),
            ]
        );
    }

    #[test]
    fn state_change_view_skips_tombstoned() {
        let db = fresh_db();
        let t0 = Utc.with_ymd_and_hms(2026, 4, 20, 9, 0, 0).unwrap();

        let ghost = entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("ephemeral"))
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
            .state(json!("focused"))
            .time_start_utc(t0)
            .insert(&db);
        entry_builder("workout")
            .state(json!(true))
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
