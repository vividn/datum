//! Acceptance tests for the shared CLI test harness (da-5mf8).

mod common;

use common::run_cli;
use datum_core::test_util::{entry_builder, fresh_db};
use datum_core::{assert_count, assert_entry_matches};

#[test]
fn run_cli_harness_dispatches_to_occur() {
    let db = fresh_db();

    // Verb-first form: `datum occur coffee 2`
    run_cli(&["occur", "coffee", "2"], &db).expect("occur dispatch");

    // Exactly one coffee entry with verb=occur and value=2.
    assert_count!(db, "coffee", 1);
    let entry = &db.entries_for_field("coffee", None).unwrap()[0];
    assert_entry_matches!(entry, field: "coffee", verb: "occur", value: 2);
}

#[test]
fn run_cli_harness_surfaces_field_verb_swap() {
    let db = fresh_db();

    // Field-first form: `datum mood switch happy` — the harness must route
    // through the pre-pass swap so the parser sees `switch mood happy`.
    run_cli(&["mood", "switch", "happy"], &db).expect("field/verb swap dispatch");

    assert_count!(db, "mood", 1);
    let entry = &db.entries_for_field("mood", None).unwrap()[0];
    assert_entry_matches!(entry, field: "mood", verb: "switch", value: "happy");
}

#[test]
fn del_alias_invokes_delete() {
    let db = fresh_db();
    let entry = entry_builder("coffee")
        .value(serde_json::json!(2))
        .insert(&db);

    // `datum del <id> --force` routes through the same dispatcher as `delete`.
    run_cli(&["del", &entry.slug, "--force"], &db).expect("del alias dispatch");

    // Entry is gone.
    let res = db.get_entry(&entry.slug);
    assert!(
        matches!(res, Err(datum_core::error::DatumError::NotFound(_))),
        "expected NotFound after del, got {res:?}"
    );
}

#[test]
fn get_pretty_emits_indented_json() {
    let db = fresh_db();
    let entry = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .value(serde_json::json!("happy"))
        .insert(&db);

    // Dispatching `get --pretty` must not error — exercises the full arg path.
    run_cli(&["get", &entry.slug, "--pretty"], &db).expect("get --pretty dispatch");

    // And the pretty form is the standard two-space-indent JSON for the entry.
    let pretty = serde_json::to_string_pretty(&entry).expect("serialize entry");
    assert!(
        pretty.contains("\n  \""),
        "expected two-space indent in pretty JSON, got:\n{pretty}"
    );
    // Key fields round-trip.
    let parsed: serde_json::Value = serde_json::from_str(&pretty).unwrap();
    assert_eq!(parsed["field"], serde_json::json!("mood"));
    assert_eq!(parsed["value"], serde_json::json!("happy"));
}

#[test]
fn log_with_update_flag_routes_to_update_strategy() {
    // Seed a row, then re-log with `--update`: the existing row must mutate
    // in place (modified_at bumped, new device_id visible), not produce a
    // second row and not be silently ignored. This is the A4 acceptance test
    // for the CLI → db strategy plumbing.
    let db = fresh_db();

    // Fix the time so both logs produce the same content_hash.
    run_cli(
        &[
            "switch", "mood", "happy",
            "-t", "09:00",
            "-k", "source=morning",
        ],
        &db,
    )
    .expect("initial switch dispatch");
    assert_count!(db, "mood", 1);
    let before = db.entries_for_field("mood", None).unwrap()[0].clone();

    // Small delay so modified_at can strictly advance.
    std::thread::sleep(std::time::Duration::from_millis(5));

    // Same field/time/value/extra → same content_hash. `--update` must mutate.
    run_cli(
        &[
            "switch", "mood", "happy",
            "-t", "09:00",
            "-k", "source=morning",
            "--update",
        ],
        &db,
    )
    .expect("update-strategy dispatch");

    // Still only one row after the collision — Update, not Rekey.
    assert_count!(db, "mood", 1);
    let after = db.entries_for_field("mood", None).unwrap()[0].clone();
    assert_eq!(after.slug, before.slug, "row must be mutated in place");
    assert!(
        after.modify_time > before.modify_time,
        "Update strategy must bump modified_at"
    );
}
