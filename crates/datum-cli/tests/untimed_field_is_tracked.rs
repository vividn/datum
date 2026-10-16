//! da-m1l8: a field with no timed state is TRACKED unless something says
//! to ignore it.
//!
//! `fields --json` and `get_tracked_fields` used to disagree about the
//! field whose state map entry is ABSENT — the all-timeless field
//! (`add`/`note`/`-T`) and the field that is nothing but a spec. `fields`
//! called it tracked and drew its button; `info` and `now` called it
//! ignored. One field, counted both ways one command over. These three
//! surfaces now read the one rule, so each of these tests asserts all
//! three against the same db.

use crate::common;

use common::run_cli;
use datum_commands::commands::fields::compute_fields;
use datum_commands::commands::info::tracked_ignored_counts;
use datum_core::db::{Db, NowFilter};
use datum_core::test_util::fresh_db;

/// `datum fields --json`'s `ignored` for `field`.
fn fields_json_ignored(db: &Db, field: &str) -> bool {
    let listing = compute_fields(db).unwrap();
    listing
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["field"] == field)
        .unwrap_or_else(|| panic!("`fields` does not list {field}"))["ignored"]
        .as_bool()
        .unwrap()
}

/// True when `datum now --json -A` puts `field` in its `ignored` list.
fn now_json_calls_it_ignored(db: &Db, field: &str) -> bool {
    let filter = NowFilter {
        include_tracked_false: true,
        include_ignored: true,
        ..NowFilter::default()
    };
    db.current_now(&filter)
        .unwrap()
        .ignored
        .iter()
        .any(|f| f.field == field)
}

#[test]
fn a_timeless_only_field_is_tracked_on_every_surface() {
    let db = fresh_db();
    // `-T` writes the entry with no occur_time, so nothing of `idea`
    // participates in the state map — the shape that used to read as
    // IGNORED.
    run_cli(&["add", "idea", "-T"], &db).unwrap();

    assert!(!fields_json_ignored(&db, "idea"));
    assert_eq!(
        tracked_ignored_counts(&db).unwrap(),
        (1, 0),
        "`info`'s tally"
    );
    assert!(!now_json_calls_it_ignored(&db, "idea"));
}

#[test]
fn a_declared_but_unlogged_field_is_tracked_on_every_surface() {
    let db = fresh_db();
    // A spec with no entry behind it: `fields` has always listed it, and
    // the other two surfaces did not see it at all.
    run_cli(&["spec", "sleep", "--states", "in,out"], &db).unwrap();

    assert!(!fields_json_ignored(&db, "sleep"));
    assert_eq!(
        tracked_ignored_counts(&db).unwrap(),
        (1, 0),
        "`info`'s tally"
    );
    assert!(!now_json_calls_it_ignored(&db, "sleep"));
}

#[test]
fn ignoring_a_timeless_only_field_still_ignores_it() {
    let db = fresh_db();
    run_cli(&["add", "idea", "-T"], &db).unwrap();
    // The rule keeps its teeth: `ignore` writes a timed IGNORED row, and
    // that row is the one the state map now finds.
    run_cli(&["ignore", "idea"], &db).unwrap();

    assert!(fields_json_ignored(&db, "idea"));
    assert_eq!(
        tracked_ignored_counts(&db).unwrap(),
        (0, 1),
        "`info`'s tally"
    );
    assert!(now_json_calls_it_ignored(&db, "idea"));
}

/// da-n0dr still decides MEMBERSHIP. Tracked-by-default is not a licence
/// to invent a field at an anchor it did not exist at: a timeless entry
/// enters the world when it is written, so `create_time` is the bound.
#[test]
fn a_timeless_field_written_after_the_anchor_is_not_in_the_snapshot() {
    let db = fresh_db();
    run_cli(&["add", "idea", "-T"], &db).unwrap();

    let anchor = chrono::Utc::now() - chrono::Duration::hours(2);
    let past = db.get_tracked_fields(anchor).unwrap();
    let names: Vec<&str> = past.iter().map(|f| f.field.as_str()).collect();
    assert!(
        !names.contains(&"idea"),
        "idea had not been written at the anchor: {names:?}"
    );
    // And it is there at wall-clock now, tracked.
    let now = db.get_tracked_fields(chrono::Utc::now()).unwrap();
    assert!(now.iter().any(|f| f.field == "idea" && !f.ignored));
}
