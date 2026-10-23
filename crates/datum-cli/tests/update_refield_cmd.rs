//! da-5t94: `update <hid> field=X` re-fields (and `--id X` re-slugs,
//! da-uf6it) in place. Before the fix, `field=` leaked silently into `extra_data`
//! because the update path only peeled the other reserved columns; the
//! shared `extract_reserved_columns` boundary now routes it onto the
//! entry's field for both insert and update.

use crate::common;

use common::run_cli;
use datum_core::test_util::fresh_db;
use serde_json::json;

/// `update <slug> field=Y` moves the entry to field `Y`, re-derives the
/// slug as `Y:<occur_time>` (the `%occur_time%` id_structure interpolated
/// against the unchanged time), preserves hid + occur_time + create_time,
/// and does NOT leak `field` into `data`.
#[test]
fn update_field_refields_and_reslugs_in_place() {
    let db = fresh_db();

    run_cli(&["occur", "gym", "-k", "reps=10"], &db).expect("seed occur");
    let before = datum_core::test_util::logged_entries(&db, "gym");
    assert_eq!(before.len(), 1, "one gym row seeded");
    let before = &before[0];
    let (hid, occur, create) = (
        before.hid.clone(),
        before.occur_time.clone(),
        before.create_time,
    );

    run_cli(&["update", before.slug.as_str(), "field=cardio"], &db)
        .unwrap_or_else(|e| panic!("re-field must succeed: {e:#}"));

    // Old field no longer has a live row; the row moved to `cardio`.
    assert!(
        datum_core::test_util::logged_entries(&db, "gym").is_empty(),
        "gym row must have been re-fielded away",
    );
    let after = datum_core::test_util::logged_entries(&db, "cardio");
    assert_eq!(after.len(), 1, "row now lives under cardio");
    let after = &after[0];

    assert_eq!(after.field, "cardio", "field column re-fielded");
    assert_eq!(
        after.slug.as_str(),
        format!(
            "cardio:{}",
            occur.as_ref().unwrap().utc.format("%Y-%m-%dT%H:%M:%S%.3fZ")
        ),
        "slug re-derived under the new field, time body preserved",
    );
    assert_eq!(
        after.hid, hid,
        "hid preserved (in-place, not delete+recreate)"
    );
    assert_eq!(after.occur_time, occur, "occur_time preserved");
    assert_eq!(after.create_time, create, "create_time preserved");

    let data = after.data.as_ref().expect("data payload survives");
    assert_eq!(data["reps"], json!(10), "pre-existing data preserved");
    assert!(
        data.get("field").is_none(),
        "`field=` must NOT leak into data, got {data:?}",
    );
}

/// `update <slug> --id Y` re-derives the slug body to Y and keeps the hid
/// (da-uf6it).
#[test]
fn update_dash_dash_id_reslugs_in_place() {
    let db = fresh_db();

    run_cli(&["occur", "habit", "--id", "morning"], &db).expect("seed occur with id");
    let hid = db
        .get_entry("habit:morning")
        .expect("seeded under --id")
        .hid;

    run_cli(&["update", "habit:morning", "--id", "evening"], &db)
        .unwrap_or_else(|e| panic!("re-id must succeed: {e:#}"));

    assert!(db.get_entry("habit:morning").is_err(), "old slug gone");
    let after = db
        .get_entry("habit:evening")
        .expect("row re-slugged to new id");
    assert_eq!(after.hid, hid, "hid preserved across re-slug");
    assert_eq!(after.field, "habit", "field unchanged");
    assert_eq!(after.data, None, "--id is not data");
}

/// `update <slug> id=Y` is data: it changes `data.id` and not the slug.
#[test]
fn update_id_key_is_data_and_keeps_the_slug() {
    let db = fresh_db();
    run_cli(&["occur", "habit", "--id", "morning"], &db).expect("seed");
    run_cli(&["update", "habit:morning", "id=evening"], &db).expect("update id=");
    let after = db.get_entry("habit:morning").expect("slug unchanged");
    assert_eq!(after.data, Some(serde_json::json!({"id": "evening"})));
    assert!(db.get_entry("habit:evening").is_err());
}
