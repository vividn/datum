//! CLI coverage for the `_A-_B` range shorthand (da-6n25).
//!
//! `_3` names one recent row; `_2-_10` names the run between two of
//! them. The point of the range is bulk action, so these tests drive it
//! through the batch commands (`delete`, `update`, `retime`) rather than
//! through the resolver alone — the resolver's own unit coverage lives in
//! `datum-core::ids::resolve`.
//!
//! Every fixture below is stamped at an explicit minute so the recency
//! ordering (`ORDER BY occur_time DESC`) — and therefore each row's
//! ordinal — is deterministic.

mod common;

use chrono::{DateTime, Utc};
use common::run_cli;
use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::test_util::{entry_builder, fresh_db};
use serde_json::json;

fn utc(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

/// Seed `n` entries on `field`, one per minute from 12:00, and return
/// them **newest first** — so `out[0]` is `_1`, `out[1]` is `_2`, and the
/// index into the vec is the ordinal minus one.
fn seed_newest_first(db: &Db, field: &str, n: u32) -> Vec<Entry> {
    let mut entries: Vec<Entry> = (0..n)
        .map(|i| {
            entry_builder(field)
                .time_start_utc(utc(&format!("2026-05-01T12:{i:02}:00Z")))
                .state(json!(i))
                .insert(db)
        })
        .collect();
    entries.reverse();
    entries
}

fn is_tombstoned(db: &Db, entry: &Entry) -> bool {
    db.get_entry_including_deleted(entry.hid.as_str())
        .expect("row must still exist after a soft delete")
        .deleted_at
        .is_some()
}

/// The headline case: one `del _2-_4` tombstones the whole run and
/// nothing outside it.
#[test]
fn range_delete_tombstones_the_whole_run() {
    let db = fresh_db();
    let rows = seed_newest_first(&db, "mood", 6);

    run_cli(&["delete", "_2-_4", "--force"], &db)
        .unwrap_or_else(|err| panic!("range delete must succeed: {err:#}"));

    for ordinal in [2usize, 3, 4] {
        assert!(
            is_tombstoned(&db, &rows[ordinal - 1]),
            "_{ordinal} must be tombstoned by `del _2-_4`",
        );
    }
    for ordinal in [1usize, 5, 6] {
        assert!(
            !is_tombstoned(&db, &rows[ordinal - 1]),
            "_{ordinal} sits outside the run and must survive",
        );
    }
}

/// The endpoints may be typed in either order — `_4-_2` is the same run.
#[test]
fn reversed_range_deletes_the_same_run() {
    let db = fresh_db();
    let rows = seed_newest_first(&db, "mood", 5);

    run_cli(&["delete", "_4-_2", "--force"], &db)
        .unwrap_or_else(|err| panic!("reversed range delete must succeed: {err:#}"));

    assert!(!is_tombstoned(&db, &rows[0]), "_1 must survive");
    for ordinal in [2usize, 3, 4] {
        assert!(
            is_tombstoned(&db, &rows[ordinal - 1]),
            "_{ordinal} must be tombstoned by `del _4-_2`",
        );
    }
    assert!(!is_tombstoned(&db, &rows[4]), "_5 must survive");
}

/// A batch mutation applies the same payload to every row in the run.
#[test]
fn range_update_applies_payload_to_each_row() {
    let db = fresh_db();
    let rows = seed_newest_first(&db, "gym", 4);

    run_cli(&["update", "_2-_3", "reps=20"], &db)
        .unwrap_or_else(|err| panic!("range update must succeed: {err:#}"));

    for ordinal in [2usize, 3] {
        let row = db.get_entry(rows[ordinal - 1].hid.as_str()).unwrap();
        assert_eq!(
            row.data.as_ref().and_then(|d| d.get("reps")),
            Some(&json!(20)),
            "_{ordinal} must carry the updated payload",
        );
    }
    for ordinal in [1usize, 4] {
        let row = db.get_entry(rows[ordinal - 1].hid.as_str()).unwrap();
        assert!(
            row.data.as_ref().and_then(|d| d.get("reps")).is_none(),
            "_{ordinal} sits outside the run and must be untouched",
        );
    }
}

/// The bead's dependency (da-bgq2) put `retime` on the shared batch
/// resolver, so a run of recent entries shifts in one invocation.
#[test]
fn range_retime_shifts_every_row_in_the_run() {
    let db = fresh_db();
    let rows = seed_newest_first(&db, "coffee", 4);

    run_cli(&["retime", "_2-_3", "--offset", "1h"], &db)
        .unwrap_or_else(|err| panic!("range retime must succeed: {err:#}"));

    // hid survives a retime; the slug is re-derived from the new time.
    for ordinal in [2usize, 3] {
        let before = rows[ordinal - 1].occur_time.as_ref().unwrap().utc;
        let after = db
            .get_entry(rows[ordinal - 1].hid.as_str())
            .unwrap()
            .occur_time
            .unwrap()
            .utc;
        assert_eq!(
            after - before,
            chrono::Duration::hours(1),
            "_{ordinal} must have shifted by an hour",
        );
    }
    for ordinal in [1usize, 4] {
        let before = rows[ordinal - 1].occur_time.as_ref().unwrap().utc;
        let after = db
            .get_entry(rows[ordinal - 1].hid.as_str())
            .unwrap()
            .occur_time
            .unwrap()
            .utc;
        assert_eq!(after, before, "_{ordinal} must not have moved");
    }
}

/// `<field>:_A-_B` counts ordinals inside that field only.
#[test]
fn field_scoped_range_stays_inside_its_field() {
    let db = fresh_db();
    let mood = seed_newest_first(&db, "mood", 3);
    let coffee = seed_newest_first(&db, "coffee", 3);

    run_cli(&["delete", "mood:_1-_2", "--force"], &db)
        .unwrap_or_else(|err| panic!("field-scoped range delete must succeed: {err:#}"));

    assert!(is_tombstoned(&db, &mood[0]), "mood:_1 must be tombstoned");
    assert!(is_tombstoned(&db, &mood[1]), "mood:_2 must be tombstoned");
    assert!(!is_tombstoned(&db, &mood[2]), "mood:_3 must survive");
    for row in &coffee {
        assert!(
            !is_tombstoned(&db, row),
            "no coffee row may be touched by a mood-scoped range",
        );
    }
}

/// A range is just another token in the comma list, so it composes with
/// the single-ordinal shorthands the user already types.
#[test]
fn range_composes_with_comma_separated_tokens() {
    let db = fresh_db();
    let rows = seed_newest_first(&db, "mood", 6);

    run_cli(&["delete", "_1,_3-_4", "--force"], &db)
        .unwrap_or_else(|err| panic!("mixed comma/range delete must succeed: {err:#}"));

    for ordinal in [1usize, 3, 4] {
        assert!(
            is_tombstoned(&db, &rows[ordinal - 1]),
            "_{ordinal} must be tombstoned by `del _1,_3-_4`",
        );
    }
    for ordinal in [2usize, 5, 6] {
        assert!(
            !is_tombstoned(&db, &rows[ordinal - 1]),
            "_{ordinal} is outside every token and must survive",
        );
    }
}

/// A run reaching past the end of the store acts on the rows that do
/// exist rather than failing — the command prints every row it touched,
/// which is the guard against a range doing more than the user expected.
#[test]
fn range_past_the_end_acts_on_what_exists() {
    let db = fresh_db();
    let rows = seed_newest_first(&db, "mood", 3);

    run_cli(&["delete", "_2-_20", "--force"], &db)
        .unwrap_or_else(|err| panic!("overflowing range delete must succeed: {err:#}"));

    assert!(!is_tombstoned(&db, &rows[0]), "_1 must survive");
    assert!(is_tombstoned(&db, &rows[1]), "_2 must be tombstoned");
    assert!(is_tombstoned(&db, &rows[2]), "_3 must be tombstoned");
}
