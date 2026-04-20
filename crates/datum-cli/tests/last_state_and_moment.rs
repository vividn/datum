//! da-1mof — A6: Apply --last-state and --moment to start/end/switch.

mod common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::entry::Verb;
use datum_core::test_util::{fresh_db, spec_builder};

#[test]
fn stop_last_state_closes_most_recent_open_block() {
    let db = fresh_db();

    // Open a block, then close it with `stop --last-state`.
    run_cli(&["start", "working"], &db).expect("start");
    run_cli(&["stop", "working", "--last-state"], &db).expect("stop --last-state");

    // Two entries: the start and the closing stop.
    let entries = db.entries_for_field("working", None).unwrap();
    assert_eq!(entries.len(), 2, "expected start + stop");

    // The stop entry records the prior active state under extra_data.last_state.
    let stop = entries
        .iter()
        .find(|e| e.verb == Verb::Stop)
        .expect("stop entry exists");
    let extra = stop.extra_data.as_ref().expect("extra_data present");
    assert_eq!(
        extra.get("last_state"),
        Some(&serde_json::json!(true)),
        "start opens state=true; stop --last-state records that",
    );

    // After the stop, the active state is false (block closed).
    let active = db
        .get_active_state("working", chrono::Utc::now() + chrono::Duration::seconds(1))
        .unwrap();
    assert_eq!(active, serde_json::json!(false));
}

#[test]
fn stop_last_state_errors_when_no_open_block() {
    let db = fresh_db();
    let err = run_cli(&["stop", "mood", "--last-state"], &db)
        .expect_err("no open block → error");
    assert!(
        err.to_string().contains("No open block"),
        "error should mention missing open block, got: {err}"
    );
}

#[test]
fn switch_last_state_dedupes_if_state_unchanged() {
    let db = fresh_db();

    run_cli(&["switch", "mood", "happy"], &db).expect("first switch");

    // Re-switching to the same state with --last-state errors.
    let err = run_cli(&["switch", "mood", "happy", "--last-state"], &db)
        .expect_err("dedup should fail");
    assert!(
        err.to_string().contains("already in state"),
        "error should mention dedup, got: {err}"
    );

    // Switching to a different state is allowed.
    run_cli(&["switch", "mood", "sad", "--last-state"], &db).expect("switch to sad");

    let entries = db.entries_for_field("mood", None).unwrap();
    assert_eq!(entries.len(), 2, "only two successful switches recorded");

    // `--moment` bypasses the dedup even when the requested state matches.
    run_cli(&["switch", "mood", "sad", "--last-state", "--moment"], &db)
        .expect("moment bypasses dedup");
}

#[test]
fn switch_moment_records_point_in_time() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy", "--moment"], &db).expect("moment switch");

    let entries = db.entries_for_field("mood", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];

    let ts = e.time_start.as_ref().expect("time_start set");
    let te = e.time_end.as_ref().expect("time_end set on --moment");
    assert_eq!(ts.utc, te.utc, "--moment: time_start == time_end");

    // Short-flag form (-m) also works.
    let db2 = fresh_db();
    run_cli(&["switch", "mood", "happy", "-m"], &db2).expect("short -m");
    let e2 = &db2.entries_for_field("mood", None).unwrap()[0];
    assert_eq!(
        e2.time_start.as_ref().unwrap().utc,
        e2.time_end.as_ref().unwrap().utc,
    );
}

#[test]
fn derived_alias_from_spec_routes_to_switch() {
    let db = fresh_db();

    // Define a spec with allowed states — regenerate_derived_aliases creates
    // one alias per state, each expanding to `switch <field> <state>`.
    spec_builder("mood").default_verb(Verb::Switch).save(&db);
    let mut spec = db.load_spec("mood").unwrap().unwrap();
    spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
    spec.regenerate_derived_aliases();
    db.save_field_spec(&spec).unwrap();

    // Sanity: the derived alias landed in the aliases table.
    let happy_alias = db.lookup_alias("happy").unwrap().expect("happy alias");
    assert_eq!(happy_alias.expands_to, "switch mood happy");

    // `datum happy` → `datum switch mood happy` — routed through the CLI pre-pass.
    run_cli(&["happy"], &db).expect("derived alias dispatch");

    let entries = db.entries_for_field("mood", None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].verb, Verb::Switch);
    assert_eq!(entries[0].value.as_ref(), Some(&serde_json::json!("happy")));
}

#[test]
fn find_open_block_returns_none_when_no_state() {
    let db: Db = fresh_db();
    assert!(db
        .find_open_block("nothing", chrono::Utc::now())
        .unwrap()
        .is_none());
}
