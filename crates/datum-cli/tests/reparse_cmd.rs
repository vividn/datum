//! Tests for `dm reparse` (da-g5uk).
//!
//! Closes the persisted-intent loop: producer (insert path stamps
//! `raw_input` + `alias_chain` + `parser_version`) + consumer (this
//! command replays the parser against the stored argv and reports /
//! repairs drift).

mod common;

use common::run_cli;
use datum_core::test_util::fresh_db;
use rusqlite::params;

/// Golden invariant: a freshly populated DB has no parser drift. After
/// inserting a handful of representative log-verb entries through the
/// CLI, `dm reparse` (dry-run) must report zero would-change rows. If
/// this ever fails it means the insert path and the replay path
/// disagree about how to interpret `raw_input` — the contract is broken.
#[test]
fn reparse_dryrun_reports_zero_drift_on_clean_db() {
    let db = fresh_db();

    run_cli(&["occur", "coffee"], &db).expect("occur");
    run_cli(&["switch", "mood", "happy"], &db).expect("switch");
    run_cli(&["start", "sleep"], &db).expect("start");
    run_cli(&["stop", "sleep"], &db).expect("stop");

    // Bare reparse (no --fix). Must not error and must touch nothing.
    run_cli(&["reparse"], &db).expect("reparse dry-run");

    // Sanity: the entries are still where we left them, with their
    // original parser_version stamped (i.e. CURRENT_PARSER_VERSION).
    let cur = datum_core::entry::CURRENT_PARSER_VERSION;
    let entries = db.all_entries().unwrap();
    assert_eq!(entries.len(), 4);
    for e in &entries {
        assert_eq!(
            e.parser_version, cur,
            "fresh write must stamp CURRENT_PARSER_VERSION"
        );
    }
}

/// Drift detection: hand-mutate a stored row's `state` column out from
/// under the entry, then run `dm reparse`. A subsequent pass with
/// `--fix` must repair the row, and a third pass must report zero
/// drift again. This is the parser-bug-fix retroactive-repair flow the
/// command exists to power.
#[test]
fn reparse_fix_repairs_a_hand_mutated_row() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy"], &db).expect("switch");

    let slug = {
        let entries = db.all_entries().unwrap();
        assert_eq!(entries.len(), 1);
        entries[0].slug.clone()
    };

    // Simulate the parser having produced a different state value at
    // some past write. We rewrite the state column directly via SQL —
    // this stands in for a parser-bug-fix world where re-parsing
    // `raw_input` today yields something different from what was
    // persisted. The replay path should detect the divergence.
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

    // --fix repairs the row — the replay says state should be
    // "happy" (the literal positional from raw_input).
    run_cli(&["reparse", "--fix"], &db).expect("reparse --fix");

    let repaired = db.get_entry(&slug).unwrap();
    assert_eq!(
        repaired.state.as_ref().and_then(|v| v.as_str()),
        Some("happy"),
        "--fix must restore the parser-derived state"
    );

    // Idempotence: a second --fix is a no-op (no drift remaining).
    let pre_modify = repaired.modify_time;
    run_cli(&["reparse", "--fix"], &db).expect("reparse --fix idempotent");
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

/// `--field` filter narrows the walk to one field's rows. Other rows'
/// drift is ignored.
#[test]
fn reparse_field_filter_ignores_other_fields() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy"], &db).expect("switch mood");
    run_cli(&["switch", "energy", "high"], &db).expect("switch energy");

    let mood_slug = db
        .entries_for_field("mood", None)
        .unwrap()
        .first()
        .unwrap()
        .slug
        .clone();
    let energy_slug = db
        .entries_for_field("energy", None)
        .unwrap()
        .first()
        .unwrap()
        .slug
        .clone();

    // Mutate both rows.
    for slug in [&mood_slug, &energy_slug] {
        db.conn()
            .execute(
                "UPDATE entries SET state = ?2 WHERE slug = ?1",
                params![slug.as_str(), "\"DRIFTED\""],
            )
            .unwrap();
    }

    // --field=mood --fix must only repair mood; energy stays drifted.
    run_cli(&["reparse", "--field", "mood", "--fix"], &db).expect("reparse mood");

    let mood = db.get_entry(&mood_slug).unwrap();
    let energy = db.get_entry(&energy_slug).unwrap();
    assert_eq!(
        mood.state.as_ref().and_then(|v| v.as_str()),
        Some("happy"),
        "mood must be repaired"
    );
    assert_eq!(
        energy.state.as_ref().and_then(|v| v.as_str()),
        Some("DRIFTED"),
        "energy must be left alone (filter excluded it)"
    );
}
