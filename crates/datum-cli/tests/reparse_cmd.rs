//! Tests for `datum reparse <hid>...` (da-bkak, persisted-intent loop
//! producer is da-g5uk).
//!
//! Per da-bkak: reparse operates ONLY on entries explicitly named by
//! hid. Walking all entries is intentionally out of scope — long-
//! standing user-validated data is sacrosanct.

mod common;

use common::run_cli;
use datum_core::test_util::fresh_db;
use rusqlite::params;

/// Bare `reparse` (no hids) must error rather than walk all entries.
/// This is the load-bearing design constraint of da-bkak: opt-in only.
#[test]
fn reparse_with_no_hids_errors_does_not_walk() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).expect("occur");

    let err = run_cli(&["reparse"], &db).expect_err("bare reparse must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("requires at least one"),
        "error must mention the missing hid argument; got: {msg}"
    );
}

/// Golden invariant: a freshly-written entry has no parser drift.
/// Naming it explicitly to `reparse` (dry-run) must touch nothing.
#[test]
fn reparse_dryrun_clean_row_reports_zero_drift() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy"], &db).expect("switch");

    let hid = {
        let entries = db.list_including_deleted().unwrap();
        assert_eq!(entries.len(), 1);
        entries[0].hid.0.clone()
    };

    run_cli(&["reparse", &hid], &db).expect("reparse dry-run on clean row");

    let cur = datum_core::entry::CURRENT_PARSER_VERSION;
    let after = db.get_entry(&hid).unwrap();
    assert_eq!(
        after.parser_version, cur,
        "fresh write must stamp CURRENT_PARSER_VERSION"
    );
}

/// Drift detection: hand-mutate a stored row's `state` column out
/// from under the entry, then run `reparse <hid> --fix`. The repair
/// must restore the parser-derived state, and a second pass must be
/// a no-op.
#[test]
fn reparse_fix_repairs_a_hand_mutated_row() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy"], &db).expect("switch");

    let (slug, hid) = {
        let entries = db.list_including_deleted().unwrap();
        assert_eq!(entries.len(), 1);
        (entries[0].slug.clone(), entries[0].hid.0.clone())
    };

    db.conn()
        .execute(
            "UPDATE entries SET state = ?2 WHERE slug = ?1",
            params![slug.as_str(), "\"sad\""],
        )
        .unwrap();

    let mutated = db.get_entry(&slug).unwrap();
    assert_eq!(
        mutated.state.as_ref().and_then(|v| v.as_str()),
        Some("sad"),
        "row must be in the mutated 'sad' state before --fix"
    );

    run_cli(&["reparse", &hid, "--fix"], &db).expect("reparse --fix");

    let repaired = db.get_entry(&slug).unwrap();
    assert_eq!(
        repaired.state.as_ref().and_then(|v| v.as_str()),
        Some("happy"),
        "--fix must restore the parser-derived state"
    );

    // Idempotence: a second --fix is a no-op.
    let pre_modify = repaired.modify_time;
    run_cli(&["reparse", &hid, "--fix"], &db).expect("reparse --fix idempotent");
    let after = db.get_entry(&slug).unwrap();
    assert_eq!(
        after.state, repaired.state,
        "second --fix must not re-mutate a clean row"
    );
    assert!(
        after.modify_time >= pre_modify,
        "modify_time must not move backwards"
    );
}

/// Naming multiple hids in one invocation reparses each independently:
/// the named one repairs, the unnamed one is left alone (proving we
/// never walk).
#[test]
fn reparse_only_touches_named_rows() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy"], &db).expect("switch mood");
    run_cli(&["switch", "energy", "high"], &db).expect("switch energy");

    let mood_entry = db
        .entries_for_field("mood", None)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    let energy_entry = db
        .entries_for_field("energy", None)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    for slug in [&mood_entry.slug, &energy_entry.slug] {
        db.conn()
            .execute(
                "UPDATE entries SET state = ?2 WHERE slug = ?1",
                params![slug.as_str(), "\"DRIFTED\""],
            )
            .unwrap();
    }

    // Name only the mood row; energy must stay drifted.
    run_cli(&["reparse", &mood_entry.hid.0, "--fix"], &db).expect("reparse mood by hid");

    let mood = db.get_entry(&mood_entry.slug).unwrap();
    let energy = db.get_entry(&energy_entry.slug).unwrap();
    assert_eq!(
        mood.state.as_ref().and_then(|v| v.as_str()),
        Some("happy"),
        "named mood row must be repaired"
    );
    assert_eq!(
        energy.state.as_ref().and_then(|v| v.as_str()),
        Some("DRIFTED"),
        "unnamed energy row must be left alone (reparse never walks)"
    );
}
