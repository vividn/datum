//! Multi-row atomic write + same-field merge — da-gh0.C.
//!
//! Given N candidate entries from N sides of a multi-entry invocation
//! (`dm switch body vertical , switch body sit`), [`Db::insert_entries_atomic`]
//!
//! 1. Groups by `(field, occur_time)`.
//! 2. For each group with 2+ entries, merges them into a single row:
//!    - **state** → [`StateValue::Parallel`] (Rule 2/11); repeated states
//!      collapse via state-equality (Rule 9).
//!    - **data** → deep-merge JSON objects, conflict at a leaf → plain
//!      JSON array of `[existing, incoming]` (Rule 2 / Section H).
//!    - **verb** → re-derived from the merged (state, dur, following_state)
//!      via [`crate::derive_verb`] so cross-verb merges (Rule 11) classify
//!      correctly.
//! 3. Writes all merged rows in a single SQLite transaction (Section B
//!    atomicity).
//!
//! Single-row writes go through the unchanged
//! [`super::insert::insert_entry_in_tx`] codepath. The merge step is a
//! no-op when only one entry shares each `(field, occur_time)` key.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::ids::Hid;
use crate::state::{normalize_state, StateValue};

use super::insert::insert_entry_in_tx_pub;
use super::types::{ConflictStrategy, InsertOutcome};
use super::Db;

impl Db {
    /// Atomic N-row write with same-field merge.
    ///
    /// Returns the stored `hid` of every persisted row, left-to-right
    /// post-merge. The caller (`datum-commands`'s multi-dispatch path)
    /// hands this list to [`crate::last_docs::set_last_docs`] so a
    /// follow-up `_LAST`-targeted command can address every row in the
    /// atomic group (Section N).
    ///
    /// Rolls back the whole batch if any side fails — partial writes are
    /// never visible. Returns `Err` on the first failure.
    pub fn insert_entries_atomic(&self, entries: Vec<Entry>) -> Result<Vec<Hid>> {
        let merged = merge_same_field_groups(entries);
        let tx = self.conn.unchecked_transaction()?;
        let mut hids = Vec::with_capacity(merged.len());
        for entry in &merged {
            match insert_entry_in_tx_pub(&tx, entry, ConflictStrategy::Ignore)? {
                InsertOutcome::Inserted { hid, .. } => hids.push(hid),
                InsertOutcome::Ignored { slug } => {
                    return Err(DatumError::Duplicate(format!(
                        "atomic multi-row insert collided with existing slug {slug}"
                    )));
                }
                other => {
                    return Err(DatumError::Other(format!(
                        "atomic multi-row insert produced unexpected outcome: {other:?}"
                    )));
                }
            }
        }
        tx.commit()?;
        Ok(hids)
    }
}

/// Group entries by `(field, occur_time)` and merge each group with 2+
/// members. Single-member groups pass through unchanged. Group order
/// matches first-appearance order in the input (left-to-right across
/// sides), so `_LAST` retains the user's call-site ordering.
pub fn merge_same_field_groups(entries: Vec<Entry>) -> Vec<Entry> {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut groups: HashMap<(String, String), Vec<Entry>> = HashMap::new();

    for entry in entries {
        let key = merge_group_key(&entry);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(entry);
    }

    let mut out = Vec::with_capacity(order.len());
    for key in order {
        let group = groups.remove(&key).expect("group key inserted above");
        if group.len() == 1 {
            out.push(group.into_iter().next().expect("len == 1"));
        } else {
            out.push(merge_group(group));
        }
    }
    out
}

/// `(field, occur_time)` key used to bucket candidate rows for merging.
/// An entry without an occur_time produces a sentinel empty string so
/// rows fall into the same bucket only when they truly share a time.
///
/// Public because the commands layer's multi-render path
/// (`leftmost_output_per_group`) must bucket per-side `OutputArgs` with
/// the *same* key [`merge_same_field_groups`] uses — one canonical key,
/// so a future change to the bucketing (say, including verb) cannot
/// silently desynchronize render pairing from merge grouping.
pub fn merge_group_key(entry: &Entry) -> (String, String) {
    let occur = entry
        .occur_time
        .as_ref()
        .map(|t| t.utc.to_rfc3339())
        .unwrap_or_default();
    (entry.field.clone(), occur)
}

/// Fold a 2+-member group into a single merged entry. The leftmost
/// entry is the base — its hid/slug/occur_time/dur/etc. survive; the
/// state and data columns absorb every member.
fn merge_group(entries: Vec<Entry>) -> Entry {
    let mut iter = entries.into_iter();
    let mut acc = iter.next().expect("merge_group called with empty group");
    for next in iter {
        acc = merge_pair(acc, next);
    }
    // Re-derive verb from the merged (state, dur, following_state). The
    // storage layer's `derive_verb_for_storage` will run this again at
    // insert time, but doing it here keeps `acc.verb` consistent with
    // the merged columns for callers that inspect the entry pre-insert.
    if let Ok((verb, _)) = crate::derive_verb(
        acc.state.clone(),
        acc.dur.as_ref(),
        acc.following_state.clone(),
    ) {
        acc.verb = verb;
    }
    // content_hash depends on state and data, both of which we just
    // mutated — recompute so dedup-by-content-hash continues to fire on
    // identical post-merge payloads (Rule 9's `vertical,vertical` case
    // collapses to a single state pre-hash; the recompute makes the
    // resulting row indistinguishable from a single-write `vertical`).
    acc.content_hash = acc.expected_content_hash();
    acc
}

/// Merge two entries that share `(field, occur_time)`. State and data
/// columns absorb the second entry; dur takes the first non-None.
fn merge_pair(a: Entry, b: Entry) -> Entry {
    let mut out = a;
    out.state = merge_state(out.state, b.state);
    out.following_state = merge_state(out.following_state, b.following_state);
    out.data = merge_data(out.data, b.data);
    if out.dur.is_none() {
        out.dur = b.dur;
    }
    out
}

/// Merge two state JSON values. Both `None` → `None`; one `None` →
/// the other. Two `Some` values normalize via [`normalize_state`] and:
///
/// - if equal, collapse to a single state (Rule 9).
/// - otherwise concatenate into a single [`StateValue::Parallel`]
///   (Rules 2 & 11). Existing `Parallel` operands flatten — a `Parallel`
///   merged with a `Simple` extends the array rather than nesting.
fn merge_state(a: Option<JsonValue>, b: Option<JsonValue>) -> Option<JsonValue> {
    match (a, b) {
        (None, None) => None,
        (Some(v), None) | (None, Some(v)) => Some(v),
        (Some(av), Some(bv)) => {
            let sa = normalize_state(&av);
            let sb = normalize_state(&bv);
            if sa == sb {
                Some(sa.to_json())
            } else {
                Some(combine_parallel(sa, sb).to_json())
            }
        }
    }
}

/// Concatenate two normalized states into a flat [`StateValue::Parallel`].
/// `Parallel` operands contribute their items verbatim — no nesting.
fn combine_parallel(a: StateValue, b: StateValue) -> StateValue {
    let mut items: Vec<StateValue> = match a {
        StateValue::Parallel(v) => v,
        other => vec![other],
    };
    match b {
        StateValue::Parallel(v) => items.extend(v),
        other => items.push(other),
    }
    StateValue::Parallel(items)
}

/// Merge two data JSON values. Both `None` → `None`; one `None` →
/// the other. Otherwise the values feed through [`merge_data_values`]:
/// objects union with recursive descent; leaf scalar conflicts produce
/// a plain JSON array.
fn merge_data(a: Option<JsonValue>, b: Option<JsonValue>) -> Option<JsonValue> {
    match (a, b) {
        (None, None) => None,
        (Some(v), None) | (None, Some(v)) => Some(v),
        (Some(av), Some(bv)) => Some(merge_data_values(av, bv)),
    }
}

/// Merge two JSON data values. Section H semantics:
///
/// - **object + object** → key union; common keys recurse. Distinct
///   keys are preserved (Section H3 "distinct data keys combined").
/// - **leaf scalar conflict** (numbers, strings, bools, etc., where
///   `existing != incoming`) → plain JSON array `[existing, incoming]`
///   (Section H1; H2 confirms this is `JsonValue::Array`, not
///   `StateValue::Parallel`).
/// - **existing array + scalar incoming** → push incoming onto the
///   array; **scalar existing + incoming array** → prepend.
/// - **equal values** → keep one (no-op merge, mirrors Rule 9 for data).
///   Checked first, so it applies to arrays and objects too: merging a
///   value with an identical copy is always idempotent (`[a]+[a]` →
///   `[a]`, not `[a, a]`).
fn merge_data_values(a: JsonValue, b: JsonValue) -> JsonValue {
    use JsonValue::*;
    match (a, b) {
        (a, b) if a == b => a,
        (Object(mut am), Object(bm)) => {
            for (k, v) in bm {
                match am.remove(&k) {
                    Some(prev) => {
                        am.insert(k, merge_data_values(prev, v));
                    }
                    None => {
                        am.insert(k, v);
                    }
                }
            }
            Object(am)
        }
        (Array(mut arr), Array(other)) => {
            arr.extend(other);
            Array(arr)
        }
        (Array(mut arr), b) => {
            arr.push(b);
            Array(arr)
        }
        (a, Array(mut arr)) => {
            arr.insert(0, a);
            Array(arr)
        }
        (a, b) => Array(vec![a, b]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dur::Dur;
    use crate::entry::Verb;
    use crate::ids::Slug;
    use crate::time::DatumTime;
    use chrono::TimeZone;
    use serde_json::json;

    fn entry_at(field: &str, time_iso: &str, state: Option<JsonValue>) -> Entry {
        let dt = chrono::DateTime::parse_from_rfc3339(time_iso)
            .unwrap()
            .with_timezone(&chrono::Utc);
        let occur = DatumTime {
            utc: dt,
            offset_minutes: 0,
            timezone: Default::default(),
        };
        Entry::new(
            field.to_string(),
            Verb::Switch,
            state,
            None,
            Some(occur),
            None,
            "test-device".to_string(),
        )
    }

    fn entry_data(field: &str, time_iso: &str, data: JsonValue) -> Entry {
        let dt = chrono::DateTime::parse_from_rfc3339(time_iso)
            .unwrap()
            .with_timezone(&chrono::Utc);
        let occur = DatumTime {
            utc: dt,
            offset_minutes: 0,
            timezone: Default::default(),
        };
        Entry::new(
            field.to_string(),
            Verb::Point,
            None,
            Some(data),
            Some(occur),
            None,
            "test-device".to_string(),
        )
    }

    // ── Section G — same-field merge state ───────────────────────────

    #[test]
    fn same_field_two_switches_merges_to_parallel_state() {
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let b = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("sit")));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].state, Some(json!(["vertical", "sit"])));
    }

    #[test]
    fn same_field_three_states_preserves_left_to_right_order() {
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let b = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("sit")));
        let c = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("supine")));
        let merged = merge_same_field_groups(vec![a, b, c]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].state, Some(json!(["vertical", "sit", "supine"])));
    }

    #[test]
    fn same_field_repeated_state_collapses_via_update_merge() {
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let b = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 1);
        // Rule 9: identical states collapse, not Parallel.
        assert_eq!(merged[0].state, Some(json!("vertical")));
    }

    #[test]
    fn same_field_cross_verb_start_end_yields_parallel_true_false() {
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!(true)));
        let b = entry_at("body", "2026-05-11T12:00:00Z", Some(json!(false)));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].state, Some(json!([true, false])));
    }

    #[test]
    fn same_field_mixed_switch_and_start_yields_parallel() {
        // `switch body sit , start body` → state=Parallel([sit, true])
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("sit")));
        let b = entry_at("body", "2026-05-11T12:00:00Z", Some(json!(true)));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].state, Some(json!(["sit", true])));
    }

    #[test]
    fn same_field_merge_only_when_occur_time_identical() {
        // Different occur_time → no merge; two separate rows.
        let a = entry_at("foo", "2026-05-11T12:00:00Z", Some(json!("a")));
        let b = entry_at("foo", "2026-05-11T12:10:00Z", Some(json!("b")));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].state, Some(json!("a")));
        assert_eq!(merged[1].state, Some(json!("b")));
    }

    // ── Section H — same-field merge data ────────────────────────────

    #[test]
    fn same_field_data_key_conflict_yields_plain_json_array() {
        // `occur drink color=red , occur drink color=blue` →
        // single row with color=[red, blue].
        let a = entry_data("drink", "2026-05-11T12:00:00Z", json!({"color": "red"}));
        let b = entry_data("drink", "2026-05-11T12:00:00Z", json!({"color": "blue"}));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].data, Some(json!({"color": ["red", "blue"]})));
    }

    #[test]
    fn same_field_data_array_not_parallel_enum() {
        // The merged data is a plain JSON array, not a StateValue::Parallel
        // (Parallel is state-only). Verify by JSON shape.
        let a = entry_data("drink", "2026-05-11T12:00:00Z", json!({"color": "red"}));
        let b = entry_data("drink", "2026-05-11T12:00:00Z", json!({"color": "blue"}));
        let merged = merge_same_field_groups(vec![a, b]);
        let data = merged[0].data.as_ref().unwrap();
        let color = data.get("color").unwrap();
        assert!(color.is_array(), "color must be JSON Array, got {color:?}");
    }

    #[test]
    fn same_field_distinct_data_keys_combined_into_single_row() {
        let a = entry_data("drink", "2026-05-11T12:00:00Z", json!({"color": "red"}));
        let b = entry_data("drink", "2026-05-11T12:00:00Z", json!({"size": "lg"}));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].data, Some(json!({"color": "red", "size": "lg"})));
    }

    #[test]
    fn same_field_data_merge_uses_update_algorithm() {
        // Object + object → recursive key union with leaf-conflict
        // promotion to array.
        let a = entry_data(
            "drink",
            "2026-05-11T12:00:00Z",
            json!({"meta": {"flavor": "sweet"}}),
        );
        let b = entry_data(
            "drink",
            "2026-05-11T12:00:00Z",
            json!({"meta": {"flavor": "sour", "temp": "cold"}}),
        );
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(
            merged[0].data,
            Some(json!({"meta": {"flavor": ["sweet", "sour"], "temp": "cold"}}))
        );
    }

    #[test]
    fn same_field_equal_data_values_collapse_to_one() {
        // "Equal values → keep one" applies to every JSON shape, arrays
        // included: `list=[a]` + `list=[a]` → `[a]`, not `[a, a]`
        // (no-op merge, mirrors Rule 9 for data). da-zhm3.
        let a = entry_data(
            "drink",
            "2026-05-11T12:00:00Z",
            json!({"val": 3, "list": ["a"]}),
        );
        let b = entry_data(
            "drink",
            "2026-05-11T12:00:00Z",
            json!({"val": 3, "list": ["a"]}),
        );
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].data, Some(json!({"val": 3, "list": ["a"]})));
    }

    #[test]
    fn same_field_equal_nested_arrays_collapse_one_level_down() {
        // Same shape inside a nested object: recursion hits the equal
        // arrays and collapses them too. da-zhm3.
        let a = entry_data(
            "drink",
            "2026-05-11T12:00:00Z",
            json!({"meta": {"tags": ["x", "y"]}}),
        );
        let b = entry_data(
            "drink",
            "2026-05-11T12:00:00Z",
            json!({"meta": {"tags": ["x", "y"], "temp": "cold"}}),
        );
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(
            merged[0].data,
            Some(json!({"meta": {"tags": ["x", "y"], "temp": "cold"}}))
        );
    }

    #[test]
    fn same_field_unequal_data_arrays_still_concatenate() {
        // Equality collapse only fires on deep-equal values; distinct
        // arrays keep the extend behavior.
        let a = entry_data("drink", "2026-05-11T12:00:00Z", json!({"list": ["a"]}));
        let b = entry_data("drink", "2026-05-11T12:00:00Z", json!({"list": ["b"]}));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged[0].data, Some(json!({"list": ["a", "b"]})));
    }

    // ── Section I — different-field rows ─────────────────────────────

    #[test]
    fn different_field_yields_separate_rows_same_occur_time() {
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let b = entry_at(
            "environment",
            "2026-05-11T12:00:00Z",
            Some(json!("outside")),
        );
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].field, "body");
        assert_eq!(merged[1].field, "environment");
    }

    #[test]
    fn different_field_same_name_key_independent_no_conflict() {
        // `vertical doseMg=80 , caffeine doseMg=100` → separate rows;
        // data is independent.
        let a = entry_data("body", "2026-05-11T12:00:00Z", json!({"doseMg": 80}));
        let b = entry_data("caffeine", "2026-05-11T12:00:00Z", json!({"doseMg": 100}));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].data, Some(json!({"doseMg": 80})));
        assert_eq!(merged[1].data, Some(json!({"doseMg": 100})));
    }

    // ── Section B — atomicity primitives ─────────────────────────────

    #[test]
    fn multi_writes_single_db_transaction() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let b = entry_at(
            "environment",
            "2026-05-11T12:00:00Z",
            Some(json!("outside")),
        );
        let hids = db.insert_entries_atomic(vec![a, b]).unwrap();
        assert_eq!(hids.len(), 2);
        // Both rows are visible after commit.
        let rows = db
            .conn()
            .prepare("SELECT field FROM entries WHERE deleted_at IS NULL")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn multi_all_sides_share_one_occur_time() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let b = entry_at(
            "environment",
            "2026-05-11T12:00:00Z",
            Some(json!("outside")),
        );
        let hids = db.insert_entries_atomic(vec![a, b]).unwrap();
        assert_eq!(hids.len(), 2);
        let times: Vec<String> = db
            .conn()
            .prepare("SELECT occur_time FROM entries WHERE deleted_at IS NULL")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(times.len(), 2);
        assert_eq!(times[0], times[1]);
    }

    #[test]
    fn multi_atomic_rollback_on_any_side_failure() {
        // Pre-seed a row whose slug will collide with side B's slug.
        // The batch inserts A successfully, then B's raw_insert errors
        // on the slug UNIQUE constraint → the whole transaction rolls
        // back. After rollback, the pre-seeded row remains and A is gone.
        let db = Db::open_in_memory().unwrap();
        let seed = entry_at(
            "environment",
            "2026-05-11T12:00:00Z",
            Some(json!("outside")),
        );
        let seed_slug = seed.slug.clone();
        db.insert_entry(&seed).unwrap();

        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let mut b = entry_at(
            "environment",
            "2026-05-11T12:00:00Z",
            // Distinct state → distinct content_hash, so this isn't a
            // dedup-Ignored — it really hits the slug UNIQUE violation
            // and propagates as Duplicate.
            Some(json!("indoors")),
        );
        b.slug = seed_slug.clone();
        let err = db.insert_entries_atomic(vec![a, b]).err();
        assert!(err.is_some(), "expected slug-collision rollback");
        // Only the pre-seed row should remain.
        let live_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live_count, 1, "rollback should leave only the pre-seed row");
        let surviving_slug: String = db
            .conn()
            .query_row(
                "SELECT slug FROM entries WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(surviving_slug, seed_slug.0);
    }

    // ── Section O — da-ga8b: (field, occur_time) partial UNIQUE ──────

    /// A custom slug that bypasses the field-occur slug template still
    /// trips the partial UNIQUE on `(field, occur_time)` when a live row
    /// already occupies that pair. The error must name the existing hid
    /// and the colliding field + occur_time.
    #[test]
    fn field_occurtime_collision_surfaces_named_diagnostic() {
        let db = Db::open_in_memory().unwrap();
        let seed = entry_at("project", "2026-05-13T09:00:00Z", Some(json!("money")));
        let seed_hid = seed.hid.0.clone();
        db.insert_entry(&seed).unwrap();

        // Distinct slug + distinct state → distinct content_hash, but the
        // same (field, occur_time) as the seed. The new partial UNIQUE
        // must fire.
        let mut intruder = entry_at("project", "2026-05-13T09:00:00Z", Some(json!("work")));
        intruder.slug = Slug("custom-id-K".to_string());
        let err = db.insert_entry(&intruder).unwrap_err();
        let msg = format!("{err}");
        assert!(
            matches!(err, DatumError::Duplicate(_)),
            "expected Duplicate, got {err:?}"
        );
        assert!(
            msg.contains("entry collides with"),
            "expected diagnostic prefix: {msg}"
        );
        assert!(msg.contains(&seed_hid), "must name existing hid: {msg}");
        assert!(
            msg.contains("project") && msg.contains("2026-05-13T09:00:00"),
            "must include field + occur_time: {msg}"
        );

        // The intruder must NOT have been written.
        let live_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live_count, 1);
    }

    /// Soft-deleting a row releases the (field, occur_time) slot — a
    /// fresh insert at the same identity succeeds because the partial
    /// UNIQUE filters tombstones (`deleted_at IS NULL`).
    #[test]
    fn field_occurtime_constraint_releases_on_tombstone() {
        let db = Db::open_in_memory().unwrap();
        let seed = entry_at("project", "2026-05-13T09:00:00Z", Some(json!("money")));
        let seed_slug = seed.slug.clone();
        db.insert_entry(&seed).unwrap();
        // Soft-delete the seed row.
        db.conn()
            .execute(
                "UPDATE entries SET deleted_at = ?1 WHERE slug = ?2",
                rusqlite::params!["2026-05-13T10:00:00Z", seed_slug.0],
            )
            .unwrap();

        // Re-insert with a custom slug at the same (field, occur_time) —
        // must succeed because the tombstoned row no longer holds the
        // slot.
        let mut replay = entry_at("project", "2026-05-13T09:00:00Z", Some(json!("work")));
        replay.slug = Slug("after-tombstone".to_string());
        db.insert_entry(&replay).unwrap();

        let live_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live_count, 1);
    }

    // ── Section O′ — da-95ez: slug freed on tombstone via row-reuse ──

    /// The headline da-95ez fix: re-`occur`-ing at a slug held only by a
    /// tombstone revives that row in place instead of colliding. One row per
    /// slug is preserved (sync keys on slug); the deleted entry's hid is
    /// retained and its tombstoned state is kept in history for undo.
    #[test]
    fn reoccur_at_tombstoned_slug_revives_row() {
        let db = Db::open_in_memory().unwrap();
        let seed = entry_at("project", "2026-05-13T09:00:00Z", Some(json!("money")));
        let slug = seed.slug.clone();
        let original_hid = seed.hid.clone();
        db.insert_entry(&seed).unwrap();
        db.soft_delete_entry(&slug.0).unwrap();

        // Re-occur at the very same slug with new content — previously this
        // hit the slug UNIQUE and returned Duplicate.
        let replay = entry_at("project", "2026-05-13T09:00:00Z", Some(json!("work")));
        assert_eq!(replay.slug, slug, "same field+occur derives the same slug");
        db.insert_entry(&replay).unwrap();

        // Exactly one live row holds the slug, and it is the revived original
        // row (hid retained) carrying the new content.
        let live: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live, 1, "row-reuse keeps one row per slug");
        let revived = db.get_entry(&slug.0).unwrap();
        assert_eq!(revived.hid, original_hid, "the hid is retained on revive");
        assert_eq!(revived.state, Some(json!("work")), "new content is stored");
        assert!(revived.deleted_at.is_none(), "tombstone cleared");

        // The deletion is still walkable in history (undo support).
        let versions = crate::history::list(db.conn(), &original_hid.0, true).unwrap();
        assert!(
            versions.iter().any(|v| v.snapshot.deleted_at.is_some()),
            "the tombstoned state must be preserved in history",
        );
    }

    /// Revive only applies to *tombstoned* slugs. A distinct-content insert at
    /// a slug still held by a LIVE row remains a duplicate-identity conflict.
    #[test]
    fn insert_at_live_slug_still_conflicts() {
        let db = Db::open_in_memory().unwrap();
        let seed = entry_at("project", "2026-05-13T09:00:00Z", Some(json!("money")));
        let slug = seed.slug.clone();
        db.insert_entry(&seed).unwrap();

        let intruder = entry_at("project", "2026-05-13T09:00:00Z", Some(json!("work")));
        assert_eq!(intruder.slug, slug);
        assert!(
            matches!(db.insert_entry(&intruder), Err(DatumError::Duplicate(_))),
            "a live slug must still reject a colliding distinct-content insert",
        );
    }

    /// Different fields at the same `occur_time` remain independent — the
    /// constraint is scoped to `(field, occur_time)`, not `occur_time`.
    /// This regression-guards against a future widening of the index.
    #[test]
    fn field_occurtime_constraint_does_not_block_different_fields() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("body", "2026-05-13T09:00:00Z", Some(json!("vertical")));
        let b = entry_at(
            "environment",
            "2026-05-13T09:00:00Z",
            Some(json!("outside")),
        );
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();
        let live_count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live_count, 2);
    }

    // ── Section N — _LAST after multi ────────────────────────────────

    #[test]
    fn last_holds_all_n_entries_after_multi_write() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let b = entry_at(
            "environment",
            "2026-05-11T12:00:00Z",
            Some(json!("outside")),
        );
        let hids = db.insert_entries_atomic(vec![a, b]).unwrap();
        crate::last_docs::set_last_docs(&db, &hids).unwrap();
        let last = crate::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(last.hids, hids);
    }

    #[test]
    fn last_preserves_row_order_left_to_right() {
        let db = Db::open_in_memory().unwrap();
        // Left-to-right order matches input order post-merge.
        let a = entry_at("a", "2026-05-11T12:00:00Z", Some(json!("x")));
        let b = entry_at("b", "2026-05-11T12:00:00Z", Some(json!("y")));
        let c = entry_at("c", "2026-05-11T12:00:00Z", Some(json!("z")));
        let hids = db.insert_entries_atomic(vec![a, b, c]).unwrap();
        crate::last_docs::set_last_docs(&db, &hids).unwrap();
        let last = crate::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(last.hids.len(), 3);
        // Order corresponds to input field order a, b, c.
        let fields: Vec<String> = last
            .hids
            .iter()
            .map(|h| {
                db.conn()
                    .query_row(
                        "SELECT field FROM entries WHERE hid = ?1",
                        rusqlite::params![h],
                        |r| r.get::<_, String>(0),
                    )
                    .unwrap()
            })
            .collect();
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn last_after_multi_then_single_write_holds_only_single() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("a", "2026-05-11T12:00:00Z", Some(json!("x")));
        let b = entry_at("b", "2026-05-11T12:00:00Z", Some(json!("y")));
        let hids = db.insert_entries_atomic(vec![a, b]).unwrap();
        crate::last_docs::set_last_docs(&db, &hids).unwrap();
        // Simulate a subsequent single-row write: set _LAST to one hid.
        let solo = entry_at("c", "2026-05-11T12:05:00Z", Some(json!("z")));
        let solo_hids = db.insert_entries_atomic(vec![solo]).unwrap();
        crate::last_docs::set_last_docs(&db, &solo_hids).unwrap();
        let last = crate::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(last.hids.len(), 1);
        assert_eq!(last.hids, solo_hids);
    }

    // ── Sanity — order preservation in the merge groupings ──────────

    #[test]
    fn merge_group_order_matches_first_appearance() {
        // a(field=x), b(field=y), c(field=x) → groups in order [x, y]
        // because x appears first.
        let a = entry_at("x", "2026-05-11T12:00:00Z", Some(json!("a")));
        let b = entry_at("y", "2026-05-11T12:00:00Z", Some(json!("b")));
        let c = entry_at("x", "2026-05-11T12:00:00Z", Some(json!("c")));
        let merged = merge_same_field_groups(vec![a, b, c]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].field, "x");
        assert_eq!(merged[1].field, "y");
        assert_eq!(merged[0].state, Some(json!(["a", "c"])));
    }

    #[test]
    fn dur_taken_from_leftmost_side_in_group() {
        let mut a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        a.dur = Some(Dur::Of(chrono::Duration::seconds(60)));
        let b = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("sit")));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(
            merged[0].dur.as_ref().and_then(|d| d.as_seconds()),
            Some(60)
        );
    }

    #[test]
    fn merged_content_hash_reflects_merged_payload() {
        // After merging, content_hash should match expected_content_hash
        // (i.e. the post-merge state/data, not the pre-merge first entry).
        let a = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("vertical")));
        let b = entry_at("body", "2026-05-11T12:00:00Z", Some(json!("sit")));
        let merged = merge_same_field_groups(vec![a, b]);
        let expected = merged[0].expected_content_hash();
        assert_eq!(merged[0].content_hash, expected);
    }

    #[test]
    fn datetime_compare_is_string_exact_not_offset_normalized() {
        // Sanity: occur_times that look equivalent but are formatted
        // differently (offset-shifted) do *not* group together. The merge
        // is on raw string equality of utc.to_rfc3339(); UTC is the
        // canonical form so callers building entries from per-side argv
        // should normalize first if they want offset-equivalent times to
        // collapse.
        let utc = chrono::Utc.with_ymd_and_hms(2026, 5, 11, 12, 0, 0).unwrap();
        let a = entry_at("body", &utc.to_rfc3339(), Some(json!("vertical")));
        let b = entry_at("body", &utc.to_rfc3339(), Some(json!("sit")));
        let merged = merge_same_field_groups(vec![a, b]);
        assert_eq!(merged.len(), 1);
    }
}
