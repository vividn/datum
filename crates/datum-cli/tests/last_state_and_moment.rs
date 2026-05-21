//! da-1mof — A6: Apply --last-state and --moment to start/end/switch.

mod common;

use common::run_cli;
use datum_core::action::Action;
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
        .find(|e| e.verb == Verb::Switch)
        .expect("stop entry exists");
    let extra = stop.data.as_ref().expect("extra_data present");
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
    let err = run_cli(&["stop", "mood", "--last-state"], &db).expect_err("no open block → error");
    assert!(
        err.to_string().contains("No open block"),
        "error should mention missing open block, got: {err}"
    );
}

#[test]
fn switch_to_same_state_files_same_state_repeat_conflict() {
    // S6 supersedes the old dedup-error behavior: a switch whose value
    // equals the field's current active state still lands, and a
    // `same_state_repeat` conflict is filed for later review.
    use datum_core::integrity::{IntegrityKind, ListFilter};
    let db = fresh_db();

    // Space the two switches so their timestamps (and content hashes) differ.
    run_cli(&["switch", "mood", "happy", "-t=-1"], &db).expect("first switch");
    run_cli(&["switch", "mood", "happy"], &db).expect("repeat switch must not error");

    let entries = db.entries_for_field("mood", None).unwrap();
    assert_eq!(entries.len(), 2, "both switches landed");

    let conflicts = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::SameStateRepeat),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(conflicts.len(), 1, "one same_state_repeat conflict filed");
    assert!(
        conflicts[0].remote_entry.is_some(),
        "same_state_repeat should carry the prior state-defining counterpart"
    );
}

#[test]
fn switch_to_different_state_does_not_file_conflict() {
    use datum_core::integrity::{IntegrityKind, ListFilter};
    let db = fresh_db();

    run_cli(&["switch", "mood", "happy", "-t=-1"], &db).expect("first switch");
    run_cli(&["switch", "mood", "sad"], &db).expect("switch to sad");

    let conflicts = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::SameStateRepeat),
            pending: Some(true),
        })
        .unwrap();
    assert!(
        conflicts.is_empty(),
        "genuine state change should not file same_state_repeat"
    );
}

#[test]
fn switch_last_state_assertion_match_files_no_conflict() {
    // S6: `-L=<v>` is an assertion. When it matches the computed prior state,
    // no mismatch is filed.
    use datum_core::integrity::{IntegrityKind, ListFilter};
    let db = fresh_db();

    run_cli(&["switch", "mood", "happy", "-t=-2"], &db).expect("seed happy");
    run_cli(&["switch", "mood", "sad", "-L=happy"], &db).expect("assertion matches");

    let conflicts = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::LastStateMismatch),
            pending: Some(true),
        })
        .unwrap();
    assert!(conflicts.is_empty(), "matching assertion files no conflict");

    // The assertion is persisted on the triggering entry regardless.
    let sad = db
        .entries_for_field("mood", None)
        .unwrap()
        .into_iter()
        .find(|e| e.state.as_ref() == Some(&serde_json::json!("sad")))
        .expect("sad entry present");
    assert_eq!(
        sad.user_asserted_last_state,
        Some(serde_json::json!("happy"))
    );
}

#[test]
fn switch_last_state_assertion_mismatch_files_conflict() {
    // S6: `-L=<v>` asserts something that disagrees with history → file a
    // `last_state_mismatch` conflict, but still land the entry.
    use datum_core::integrity::{IntegrityKind, ListFilter};
    let db = fresh_db();

    run_cli(&["switch", "mood", "happy", "-t=-2"], &db).expect("seed happy");
    // User thinks prior state was "sad", but history says "happy".
    run_cli(&["switch", "mood", "grumpy", "-L=sad"], &db).expect("assertion still lands");

    let entries = db.entries_for_field("mood", None).unwrap();
    assert_eq!(entries.len(), 2, "grumpy entry still lands");

    let conflicts = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::LastStateMismatch),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(conflicts.len(), 1, "one last_state_mismatch filed");
    assert_eq!(
        conflicts[0].local_entry.user_asserted_last_state,
        Some(serde_json::json!("sad"))
    );
}

#[test]
fn bare_switch_last_state_is_noop_assertion_wise() {
    // Bare `--last-state` on switch carries no value → no user_asserted_last_state,
    // no mismatch, no dedup error (unless the value itself repeats).
    let db = fresh_db();

    run_cli(&["switch", "mood", "happy", "-t=-1"], &db).expect("first switch");
    run_cli(&["switch", "mood", "sad", "--last-state"], &db)
        .expect("bare --last-state no longer errors");

    let sad = db
        .entries_for_field("mood", None)
        .unwrap()
        .into_iter()
        .find(|e| e.state.as_ref() == Some(&serde_json::json!("sad")))
        .expect("sad entry present");
    assert!(
        sad.user_asserted_last_state.is_none(),
        "bare -L records no assertion"
    );
}

#[test]
fn switch_moment_records_point_in_time() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy", "--moment"], &db).expect("moment switch");

    let entries = db.entries_for_field("mood", None).unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];

    let ts = e.occur_time.as_ref().expect("time_start set");
    let te = e.computed_end_time().expect("time_end set on --moment");
    assert_eq!(ts.utc, te.utc, "--moment: time_start == time_end");

    // Short-flag form (-m) also works.
    let db2 = fresh_db();
    run_cli(&["switch", "mood", "happy", "-m"], &db2).expect("short -m");
    let e2 = &db2.entries_for_field("mood", None).unwrap()[0];
    assert_eq!(
        e2.occur_time.as_ref().unwrap().utc,
        e2.computed_end_time().unwrap().utc,
    );
}

#[test]
fn derived_alias_from_spec_routes_to_switch() {
    let db = fresh_db();

    // Define a spec with allowed states — regenerate_derived_aliases creates
    // one alias per state, each expanding to `switch <field> <state>`.
    spec_builder("mood").default_verb(Action::Switch).save(&db);
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
    assert_eq!(entries[0].state.as_ref(), Some(&serde_json::json!("happy")));
}

#[test]
fn find_open_block_returns_none_when_no_state() {
    let db: Db = fresh_db();
    assert!(db
        .find_open_block("nothing", chrono::Utc::now())
        .unwrap()
        .is_none());
}
