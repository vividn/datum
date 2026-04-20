//! Acceptance tests for the shared CLI test harness (da-5mf8).

mod common;

use common::run_cli;
use datum_core::test_util::fresh_db;
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
    // through the pre-pass swap so clap sees `switch mood happy`.
    run_cli(&["mood", "switch", "happy"], &db).expect("field/verb swap dispatch");

    assert_count!(db, "mood", 1);
    let entry = &db.entries_for_field("mood", None).unwrap()[0];
    assert_entry_matches!(entry, field: "mood", verb: "switch", value: "happy");
}
