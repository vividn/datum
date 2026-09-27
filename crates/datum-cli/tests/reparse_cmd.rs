//! Tests for `datum reparse <hid>...` (da-bkak, persisted-intent loop
//! producer is da-g5uk).
//!
//! Per da-bkak: reparse operates ONLY on entries explicitly named by
//! hid. Walking all entries is intentionally out of scope — long-
//! standing user-validated data is sacrosanct.

use crate::common;

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
        let entries = datum_core::test_util::logged_all(&db);
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
        let entries = datum_core::test_util::logged_all(&db);
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

/// da-jbzmb: a relative `*_date` / `*_time` key replays anchored where
/// the insert anchored it — the row's occur time, or a timeless row's
/// create time — not at the replay's "now" and not as the raw text. Run
/// months later, `--fix` must leave the resolved values (and the field)
/// as they were.
#[test]
fn reparse_fix_keeps_relative_temporal_keys_resolved() {
    let db = fresh_db();
    // Logged on the real clock: `create_time` is always the wall clock.
    run_cli(
        &["occur", "dishes", "-t", "2026-09-20T10:00", "next_date=+1"],
        &db,
    )
    .unwrap();
    run_cli(
        &["occur", "vacuum", "-T", "next_date=+7", "next_time=+2h"],
        &db,
    )
    .unwrap();
    let _g = datum_core::test_util::freeze_time("2030-01-01T12:00:00");

    for field in ["dishes", "vacuum"] {
        let before = datum_core::test_util::logged_entries(&db, field)[0].clone();
        let hid = before.hid.to_string();
        run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
        let after = db.get_entry(&hid).unwrap();
        assert_eq!(after.field, field);
        assert_eq!(after.data, before.data, "{field}");
    }
    let dishes = &datum_core::test_util::logged_entries(&db, "dishes")[0];
    assert_eq!(
        dishes.data,
        Some(serde_json::json!({"next_date": "2026-09-21"}))
    );
}

/// da-9p48y: a field's first row declares the field in its own
/// `following_state` (da-gvcen), which `log_entry` adds from a spec
/// lookup and not from the line. Replay must keep that declaration —
/// and only that: a trailing state the line never spelled is still
/// drift.
#[test]
fn reparse_fix_keeps_the_field_declaration() {
    let db = fresh_db();
    run_cli(&["occur", "plain", "x=1"], &db).unwrap();
    let before = datum_core::test_util::logged_entries(&db, "plain")[0].clone();
    assert_eq!(before.following_state, Some(serde_json::json!("TRACKED")));
    let hid = before.hid.to_string();

    run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
    let after = db.get_entry(&hid).unwrap();
    assert_eq!(after.following_state, before.following_state);
    assert_eq!(
        after.content_hash, before.content_hash,
        "a clean row is not rewritten"
    );

    db.conn()
        .execute(
            "UPDATE entries SET following_state = '\"sit\"' WHERE hid = ?1",
            params![hid.as_str()],
        )
        .unwrap();
    run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
    assert_eq!(db.get_entry(&hid).unwrap().following_state, None);
}

/// da-0rx53: `start_time=` / `end_time=` (da-mjwq) place the block, so
/// replay lifts them as the insert did — nothing of them in `data`, the
/// pair's span as the dur, `end_time=` alone on `start` an `end` — and a
/// relative key anchors where the insert anchored it: at `start_time=`
/// when a duration follows it (a3 crosses midnight: stored at 00:00 on
/// the 2nd, anchored at 23:00 on the 1st), at the stored time otherwise.
#[test]
fn reparse_fix_lifts_start_time_and_end_time() {
    let db = fresh_db();
    let lines: [&[&str]; 4] = [
        &[
            "start",
            "a2",
            "start_time=2026-01-01T10:00",
            "end_time=2026-01-01T12:00",
        ],
        &[
            "occur",
            "a7",
            "start_time=2026-01-01T10:00",
            "end_time=2026-01-01T11:00",
        ],
        &["start", "a5", "end_time=2026-01-01T23:00"],
        &["start", "a3", "start_time=2026-01-01T23:00", "1h"],
    ];
    for line in lines {
        run_cli(&[line, &["next_date=+1"]].concat(), &db).unwrap();
    }
    let _g = datum_core::test_util::freeze_time("2030-01-01T12:00:00");

    for field in ["a2", "a7", "a5", "a3"] {
        let before = datum_core::test_util::logged_entries(&db, field)[0].clone();
        assert_eq!(
            before.data,
            Some(serde_json::json!({"next_date": "2026-01-02"})),
            "{field}"
        );
        let hid = before.hid.to_string();
        run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
        let after = db.get_entry(&hid).unwrap();
        assert_eq!(after.content_hash, before.content_hash, "{field} is clean");
    }
}
