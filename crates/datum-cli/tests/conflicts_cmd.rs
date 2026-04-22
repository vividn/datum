//! D5 acceptance tests — no double-start error; overlaps route to the
//! conflict queue; auto-resolver closes obvious cases.
//!
//! These mirror the acceptance list in `da-gnj.5`:
//! - `double_start_does_not_error`
//! - `double_start_creates_conflict_row`
//! - `orphan_end_does_not_error`
//! - `orphan_end_creates_conflict_row`
//! - `auto_resolver_closes_older_open_after_delay`
//! - `auto_resolver_skips_if_kept_both`
//! - `now_view_shows_overlap_warning`

mod common;

use std::collections::HashMap;

use common::run_cli;
use datum_cli::output::{render_now_view, strip_ansi};
use datum_core::conflicts::{ConflictKind, ListFilter, Resolution};
use datum_core::db::TrackedField;
use datum_core::test_util::fresh_db;

fn tracked(field: &str) -> TrackedField {
    TrackedField {
        field: field.to_string(),
        ignored: false,
        color: None,
    }
}

// ─── double-start ────────────────────────────────────────────────────────────

#[test]
fn double_start_does_not_error() {
    // Two `datum start` invocations for the same field back-to-back must
    // both succeed — D5's core guarantee: never refuse data entry.
    let db = fresh_db();
    run_cli(&["start", "workout"], &db).expect("first start dispatch");
    run_cli(&["start", "workout"], &db).expect("second start must not error");

    let entries = db.entries_for_field("workout", None).unwrap();
    assert_eq!(entries.len(), 2, "both start entries must be present");
}

#[test]
fn double_start_creates_conflict_row() {
    // The second `start` on an already-open field lands, AND a
    // `double_open` conflict row is filed for later review.
    let db = fresh_db();
    // Space the starts by 1 minute so their content_hashes differ and
    // neither path trips dedup.
    run_cli(&["start", "workout", "-t=-1"], &db).expect("first start");
    run_cli(&["start", "workout"], &db).expect("second start");

    let conflicts = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(
        conflicts.len(),
        1,
        "one double_open conflict filed; got {}",
        conflicts.len()
    );
    let c = &conflicts[0];
    assert_eq!(c.conflict_kind, ConflictKind::DoubleOpen);
    assert!(c.remote_entry.is_some(), "double_open carries the older counterpart");
    assert_eq!(c.local_entry.field, "workout");
}

// ─── orphan end ──────────────────────────────────────────────────────────────

#[test]
fn orphan_end_does_not_error() {
    // A `stop` on a field that has never been started must still write
    // the stop entry.
    let db = fresh_db();
    run_cli(&["stop", "workout"], &db).expect("orphan stop must not error");
    let entries = db.entries_for_field("workout", None).unwrap();
    assert_eq!(entries.len(), 1, "orphan stop entry lands");
    assert_eq!(entries[0].verb, datum_core::entry::Verb::Stop);
}

#[test]
fn orphan_end_creates_conflict_row() {
    // Orphan stop files an `orphan_end` conflict. `remote_entry` is None
    // because there's no counterpart to point at.
    let db = fresh_db();
    run_cli(&["stop", "workout"], &db).expect("orphan stop");

    let conflicts = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::OrphanEnd),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(conflicts.len(), 1, "one orphan_end conflict filed");
    let c = &conflicts[0];
    assert!(
        c.remote_entry.is_none(),
        "orphan_end has no remote counterpart"
    );
}

// ─── auto-resolver ───────────────────────────────────────────────────────────

#[test]
fn auto_resolver_closes_older_open_after_delay() {
    // Two opens, then auto-resolver (with min_age=0 so the conflict is
    // immediately eligible). The older ongoing should get an end_time
    // equal to the newer one's start_time, and the conflict should be
    // marked auto_close_older.
    use datum_core::entry::{EntryShape, Verb};

    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    // Sanity: one conflict pending.
    let pending = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(pending.len(), 1, "pre-resolve: one pending conflict");

    // min_age_secs = 0 — conflict is eligible immediately.
    let applied = db
        .auto_resolve_conflicts(chrono::Utc::now(), 0)
        .unwrap();
    assert_eq!(applied.len(), 1, "auto-resolver applied once");
    assert_eq!(applied[0].conflict_kind, ConflictKind::DoubleOpen);

    // The older ongoing is now a block (has end_time). The query order
    // isn't guaranteed, so identify newer/older by start_time.
    let entries = db.entries_for_field("workout", None).unwrap();
    let mut sorted = entries.clone();
    sorted.sort_by_key(|e| e.start_time.as_ref().unwrap().utc);
    let older = sorted[0].clone();
    let newer = sorted[1].clone();
    assert_eq!(
        older.entry_shape,
        EntryShape::Block,
        "older open should be closed into a block; got shape={:?}",
        older.entry_shape
    );
    assert!(older.end_time.is_some(), "older open must have end_time");
    assert_eq!(
        older.end_time.as_ref().unwrap().utc,
        newer.start_time.as_ref().unwrap().utc,
        "older's end_time should match newer's start_time"
    );
    // Newer remains ongoing.
    assert_eq!(newer.verb, Verb::Start);
    assert_eq!(newer.entry_shape, EntryShape::Ongoing);

    // Pending set should be empty now.
    let pending = db
        .list_conflicts(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    assert!(pending.is_empty(), "pending queue drained after auto-resolve");
}

#[test]
fn auto_resolver_skips_if_kept_both() {
    // A conflict that the user has explicitly resolved with `--keep both`
    // must NOT be re-touched by the auto-resolver.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    let pending = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(pending.len(), 1);
    let hid = pending[0].hid.clone();

    // User explicitly keeps both.
    db.resolve_conflict(&hid, Resolution::KeepBoth).unwrap();

    // Auto-resolver runs — should find nothing eligible (resolution is
    // no longer NULL).
    let applied = db
        .auto_resolve_conflicts(chrono::Utc::now(), 0)
        .unwrap();
    assert!(applied.is_empty(), "resolved conflict must not be reopened");

    // And the older entry should STILL be ongoing (no end_time was
    // written by anyone).
    let entries = db.entries_for_field("workout", None).unwrap();
    for e in &entries {
        assert!(
            e.end_time.is_none(),
            "keep_both must not have closed either entry"
        );
    }
}

// ─── datum now overlap warning ───────────────────────────────────────────────

#[test]
fn now_view_shows_overlap_warning() {
    // Two live ongoing entries on the same field produce an overlap
    // warning line above the field's `now` row.
    let db = fresh_db();
    run_cli(&["track", "workout"], &db).expect("track");
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    // Pull the overlap counts the way cmd_now does.
    let tf = [tracked("workout")];
    let mut latest = HashMap::new();
    let entry = db.latest_entry_for_now("workout").unwrap().unwrap();
    latest.insert("workout".to_string(), entry);
    let mut overlap = HashMap::new();
    overlap.insert("workout".to_string(), db.open_count_for_field("workout").unwrap());

    let out = render_now_view(&tf, &latest, &HashMap::new(), &overlap, false);
    let plain = strip_ansi(&out);
    assert!(
        plain.contains("overlapping opens"),
        "expected overlap warning in now view:\n{plain}"
    );
    assert!(
        plain.contains("datum conflicts list"),
        "warning must point user to the conflicts CLI:\n{plain}"
    );
}
