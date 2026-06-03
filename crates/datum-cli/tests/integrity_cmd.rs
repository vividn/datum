//! D5 + C4e acceptance tests — no double-start error; overlaps route to
//! the integrity-warnings queue; auto-resolver closes obvious cases; the
//! `now` overlap banner and the pending-warning header behave as
//! documented. The `datum integrity` CLI surface was removed in da-z46t;
//! the remaining DB-level tests cover the detection and (internal)
//! resolution paths that still back `datum check`.

mod common;

use std::collections::HashMap;

use common::run_cli;
use datum_cli::output::{render_pending_integrity_warning, render_snapshot, strip_ansi};
use datum_core::db::{NowFieldActive, NowSnapshot, TrackedField, NOW_SCHEMA_ID};
use datum_core::integrity::{IntegrityKind, ListFilter, Resolution};
use datum_core::test_util::fresh_db;
use serde_json::Value as JsonValue;

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
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
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
    assert_eq!(c.kind, IntegrityKind::DoubleOpen);
    assert!(
        c.remote_entry.is_some(),
        "double_open carries the older counterpart"
    );
    assert_eq!(c.local_entry.field, "workout");
}

#[test]
fn deleting_trigger_entry_suppresses_double_open_warning() {
    // da-sidz: after the entry that caused a double_open is soft-deleted,
    // the integrity problem no longer exists in the live dataset, so the
    // stored warning must stop surfacing. Previously `list` ignored
    // `deleted_at`, so `datum check` kept reporting the double-open forever.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-1"], &db).expect("first start");
    run_cli(&["start", "workout"], &db).expect("second start (double-open)");

    let filter = ListFilter {
        kind: Some(IntegrityKind::DoubleOpen),
        pending: Some(true),
    };
    let conflicts = db.list_integrity_warnings(&filter).unwrap();
    assert_eq!(conflicts.len(), 1, "double_open filed before deletion");

    // Delete the trigger entry (the second start) via the CLI, mirroring
    // the bead repro.
    let trigger_slug = conflicts[0].entry_slug.clone();
    run_cli(&["del", &trigger_slug, "--force"], &db).expect("delete trigger entry");

    let after = db.list_integrity_warnings(&filter).unwrap();
    assert!(
        after.is_empty(),
        "double_open warning must not surface once the trigger is soft-deleted; got {}",
        after.len()
    );
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
    assert_eq!(entries[0].verb, datum_core::entry::Verb::Switch);
}

#[test]
fn orphan_end_creates_conflict_row() {
    // Orphan stop files an `orphan_end` conflict. `remote_entry` is None
    // because there's no counterpart to point at.
    let db = fresh_db();
    run_cli(&["stop", "workout"], &db).expect("orphan stop");

    let conflicts = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::OrphanEnd),
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
    // immediately eligible). The older ongoing should get a dur covering
    // the gap up to the newer's occur_time, and the conflict should be
    // marked auto_close_older.
    //
    // Under v3 verbs (da-nm5b): older was Switch state=true; after close
    // it becomes Block (state still true, dur set). Newer remains Switch
    // — under v3 a "still open" event is just a Switch with no dur, so
    // entry_shape = Point (the legacy `Ongoing` shape no longer exists).
    use datum_core::entry::{EntryShape, Verb};

    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    // Sanity: one conflict pending.
    let pending = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(pending.len(), 1, "pre-resolve: one pending conflict");

    // min_age_secs = 0 — conflict is eligible immediately.
    let applied = db.auto_resolve_integrity(chrono::Utc::now(), 0).unwrap();
    assert_eq!(applied.len(), 1, "auto-resolver applied once");
    assert_eq!(applied[0].kind, IntegrityKind::DoubleOpen);

    // The older ongoing is now a block (has dur). The query order
    // isn't guaranteed, so identify newer/older by occur_time.
    let entries = db.entries_for_field("workout", None).unwrap();
    let mut sorted = entries.clone();
    sorted.sort_by_key(|e| e.occur_time.as_ref().unwrap().utc);
    let older = sorted[0].clone();
    let newer = sorted[1].clone();
    assert_eq!(
        older.shape(),
        EntryShape::Block,
        "older open should be closed into a block; got shape={:?}",
        older.shape()
    );
    assert!(older.dur.is_some(), "older open must have dur");
    assert_eq!(older.verb, Verb::Block, "older's verb re-derives to Block");
    // dur is stored at second precision under v3 (parse_iso_duration
    // returns whole-second chrono spans), so equality between computed_end and the sub-second
    // newer.occur_time can be off by < 1s. The contract being tested is that
    // the older block ends "at the newer's start moment" — verified by
    // bounding the gap rather than exact equality.
    let older_end = older.computed_end_time().unwrap().utc;
    let newer_start = newer.occur_time.as_ref().unwrap().utc;
    let delta_ms = (older_end - newer_start).num_milliseconds().abs();
    assert!(
        delta_ms < 1000,
        "older's computed end ({older_end}) must be within 1s of newer's start ({newer_start}); got {delta_ms}ms"
    );
    // Newer remains a Switch with no dur — under v3 that's entry_shape=Point.
    assert_eq!(newer.verb, Verb::Switch);
    assert!(
        newer.dur.is_none(),
        "newer must remain dur-less (still 'open')"
    );
    assert_eq!(newer.shape(), EntryShape::Point);

    // Pending set should be empty now.
    let pending = db
        .list_integrity_warnings(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    assert!(
        pending.is_empty(),
        "pending queue drained after auto-resolve"
    );
}

#[test]
fn auto_resolver_skips_if_kept_both() {
    // A conflict that the user has explicitly resolved with `--keep both`
    // must NOT be re-touched by the auto-resolver.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    let pending = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(pending.len(), 1);
    let hid = pending[0].hid.clone();

    // User explicitly keeps both.
    db.resolve_integrity_warning(&hid, Resolution::KeepBoth)
        .unwrap();

    // Auto-resolver runs — should find nothing eligible (resolution is
    // no longer NULL).
    let applied = db.auto_resolve_integrity(chrono::Utc::now(), 0).unwrap();
    assert!(applied.is_empty(), "resolved conflict must not be reopened");

    // And the older entry should STILL be ongoing (no end_time was
    // written by anyone).
    let entries = db.entries_for_field("workout", None).unwrap();
    for e in &entries {
        assert!(
            e.dur.is_none(),
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

    // Pull the overlap counts the way cmd_now does, then build a
    // NowSnapshot directly — bypassing the (retired in da-adoi)
    // `render_now_view(tracked, latest_map, …)` legacy signature.
    let entry = db.latest_entry_for_now("workout", None).unwrap().unwrap();
    let since = entry.occur_time.as_ref().map(|t| t.utc);
    let state = entry.state.clone().unwrap_or(JsonValue::Null);
    let snap = NowSnapshot {
        schema: NOW_SCHEMA_ID,
        now: chrono::Utc::now(),
        active: vec![NowFieldActive {
            field: "workout".to_string(),
            state,
            since,
            verb: entry.verb.to_string(),
            color: None,
            entry,
        }],
        tracked_false: Vec::new(),
        ignored: Vec::new(),
    };
    let mut overlap = HashMap::new();
    overlap.insert(
        "workout".to_string(),
        db.open_count_for_field("workout").unwrap(),
    );
    let _ = tracked; // unused helper now; kept for symmetry across the file.

    let out = render_snapshot(&snap, &HashMap::new(), &overlap);
    let plain = strip_ansi(&out);
    assert!(
        plain.contains("overlapping opens"),
        "expected overlap warning in now view:\n{plain}"
    );
    assert!(
        plain.contains("datum check"),
        "warning must point user to `datum check`:\n{plain}"
    );
}

// ─── C4e: list filtering ─────────────────────────────────────────────────────

#[test]
fn conflicts_list_shows_only_unresolved_by_default() {
    // File two conflicts, resolve one. The default list (no `--all`) must
    // only return the still-pending row.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");
    run_cli(&["start", "shower", "-t=-5"], &db).expect("shower older");
    run_cli(&["start", "shower"], &db).expect("shower newer");

    let all = db
        .list_integrity_warnings(&ListFilter {
            kind: None,
            pending: None,
        })
        .unwrap();
    assert_eq!(all.len(), 2, "two conflicts filed pre-resolution");

    // Resolve one explicitly so the other stays pending.
    let target = all
        .iter()
        .find(|c| c.local_entry.field == "workout")
        .expect("workout conflict")
        .clone();
    db.resolve_integrity_warning(&target.hid, Resolution::KeepBoth)
        .unwrap();

    let pending = db
        .list_integrity_warnings(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(pending.len(), 1, "default list should only show pending");
    assert_eq!(pending[0].local_entry.field, "shower");
}

#[test]
fn conflicts_list_all_includes_resolved() {
    // Same setup as above, but with `pending: None` (the `--all` form)
    // we should see both rows including the resolved one.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");
    run_cli(&["start", "shower", "-t=-5"], &db).expect("shower older");
    run_cli(&["start", "shower"], &db).expect("shower newer");

    let all = db
        .list_integrity_warnings(&ListFilter {
            kind: None,
            pending: None,
        })
        .unwrap();
    let target = all[0].clone();
    db.resolve_integrity_warning(&target.hid, Resolution::KeepBoth)
        .unwrap();

    let after = db
        .list_integrity_warnings(&ListFilter {
            kind: None,
            pending: None,
        })
        .unwrap();
    assert_eq!(after.len(), 2, "--all list returns both rows");
    let resolved_count = after.iter().filter(|c| c.resolution.is_some()).count();
    let pending_count = after.iter().filter(|c| c.resolution.is_none()).count();
    assert_eq!(resolved_count, 1);
    assert_eq!(pending_count, 1);
}

// ─── C4e: take_local / take_remote ───────────────────────────────────────────

#[test]
fn conflicts_resolve_take_local_updates_entry_and_marks_resolved() {
    // File a double-open, resolve `--take local`. The Db API must return
    // the conflict row with `resolution = "take_local"` and the entry
    // matching `local_entry.slug` must equal the snapshot. (For
    // double-open the snapshot equals the live row, so we verify the
    // row is present and untouched-but-confirmed.)
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    let pending = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(pending.len(), 1);
    let hid = pending[0].hid.clone();
    let local_slug = pending[0].local_entry.slug.clone();

    let resolved = db
        .resolve_integrity_warning(&hid, Resolution::TakeLocal)
        .unwrap();
    assert_eq!(resolved.resolution.as_deref(), Some("take_local"));
    assert!(
        resolved.resolved_at.is_some(),
        "resolved_at must be stamped"
    );

    // The entry referenced by local_entry.slug is still in the table
    // (rewrite_entry_to_snapshot is idempotent here).
    let entries = db.entries_for_field("workout", None).unwrap();
    assert!(
        entries.iter().any(|e| e.slug == local_slug),
        "local_entry slug must remain in entries table"
    );
}

#[test]
fn conflicts_resolve_take_remote_updates_entry_and_marks_resolved() {
    // For sync_pull-style conflicts where remote_entry was applied to
    // the table, `--take remote` is a no-op on data; here we just
    // confirm the resolution string round-trips. The "rewrite a
    // diverged row" case is exercised by
    // `conflicts_resolve_edit_uses_saved_content`.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    let pending = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    let hid = pending[0].hid.clone();
    let remote_slug = pending[0]
        .remote_entry
        .as_ref()
        .expect("double_open carries remote")
        .slug
        .clone();

    let resolved = db
        .resolve_integrity_warning(&hid, Resolution::TakeRemote)
        .unwrap();
    assert_eq!(resolved.resolution.as_deref(), Some("take_remote"));

    let entries = db.entries_for_field("workout", None).unwrap();
    assert!(
        entries.iter().any(|e| e.slug == remote_slug),
        "remote_entry slug must remain in entries table"
    );
}

// ─── C4e: --edit ─────────────────────────────────────────────────────────────

#[test]
fn conflicts_resolve_edit_uses_saved_content() {
    // The `--edit` flow ultimately calls `db.resolve_integrity_warning_with_edited`
    // with the (possibly mutated) Entry parsed from the editor's saved
    // file. This test exercises that data path directly: build a
    // modified Entry, hand it to the API, and verify the live row was
    // overwritten with the edited content.
    use datum_core::entry::Verb;

    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    let pending = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    let conflict = pending[0].clone();

    // Edit: same slug, but stamp a unique value into `data` so we can
    // tell it landed.
    let mut edited = conflict.local_entry.clone();
    edited.data = Some(serde_json::json!({"edited_marker": "c4e"}));

    let resolved = db
        .resolve_integrity_warning_with_edited(&conflict.hid, &edited)
        .unwrap();
    assert_eq!(resolved.resolution.as_deref(), Some("edit"));

    let entries = db.entries_for_field("workout", None).unwrap();
    let row = entries
        .iter()
        .find(|e| e.slug == edited.slug)
        .expect("edited slug must still be in table");
    let data = row.data.as_ref().expect("data should be set after edit");
    assert_eq!(
        data.get("edited_marker").and_then(|v| v.as_str()),
        Some("c4e"),
        "edited content must replace the live row's data"
    );

    // Sanity: the conflict's verb didn't drift through the round-trip.
    assert_eq!(row.verb, Verb::Switch);
}

// ─── C4e: now warning ────────────────────────────────────────────────────────

#[test]
fn now_view_shows_conflict_warning_when_pending() {
    // The header helper renders a warning line for non-zero counts and
    // a blank for zero; both forms must satisfy callers that always
    // print `render_pending_integrity_warning(n)`.
    assert_eq!(render_pending_integrity_warning(0), "");

    let one = render_pending_integrity_warning(1);
    assert!(one.contains("1 unresolved integrity warning"));
    assert!(
        !one.contains("1 unresolved integrity warnings"),
        "singular form for n=1, got: {one:?}"
    );
    assert!(one.contains("datum check"));

    let many = render_pending_integrity_warning(3);
    assert!(many.contains("3 unresolved integrity warnings"));
    assert!(many.contains("datum check"));
}

// ─── C4e: pending_integrity_count ─────────────────────────────────────────────

#[test]
fn pending_integrity_count_excludes_resolved() {
    // The `now` view calls `db.pending_integrity_count` to decide whether
    // to print the header. Make sure resolved rows don't count.
    let db = fresh_db();
    assert_eq!(db.pending_integrity_count().unwrap(), 0);

    run_cli(&["start", "workout", "-t=-5"], &db).expect("older");
    run_cli(&["start", "workout"], &db).expect("newer");
    assert_eq!(db.pending_integrity_count().unwrap(), 1);

    // Resolve it; count drops to 0.
    let pending = db
        .list_integrity_warnings(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    db.resolve_integrity_warning(&pending[0].hid, Resolution::KeepBoth)
        .unwrap();
    assert_eq!(db.pending_integrity_count().unwrap(), 0);
}

#[test]
fn pending_integrity_count_excludes_soft_deleted_trigger() {
    // da-sidz: the `now`-view count must agree with `datum check` — a
    // double-open whose trigger entry was deleted no longer counts.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older");
    run_cli(&["start", "workout"], &db).expect("newer (double-open)");
    assert_eq!(db.pending_integrity_count().unwrap(), 1);

    let pending = db
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    run_cli(&["del", &pending[0].entry_slug, "--force"], &db).expect("delete trigger");
    assert_eq!(
        db.pending_integrity_count().unwrap(),
        0,
        "deleting the trigger entry must drop the pending count"
    );
}
