//! D5 + C4e acceptance tests — no double-start error; overlaps route to
//! the conflict queue; auto-resolver closes obvious cases; the C4e
//! conflict CLI surface (list filtering, take_local/take_remote/edit
//! resolution paths, and the `now` warning) behaves as documented.

mod common;

use std::collections::HashMap;

use common::run_cli;
use datum_cli::output::{
    render_conflict_show, render_now_view, render_pending_conflict_warning, strip_ansi,
};
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
    assert!(
        c.remote_entry.is_some(),
        "double_open carries the older counterpart"
    );
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
    assert_eq!(entries[0].verb, datum_core::entry::Verb::Switch);
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
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(pending.len(), 1, "pre-resolve: one pending conflict");

    // min_age_secs = 0 — conflict is eligible immediately.
    let applied = db.auto_resolve_conflicts(chrono::Utc::now(), 0).unwrap();
    assert_eq!(applied.len(), 1, "auto-resolver applied once");
    assert_eq!(applied[0].conflict_kind, ConflictKind::DoubleOpen);

    // The older ongoing is now a block (has dur). The query order
    // isn't guaranteed, so identify newer/older by occur_time.
    let entries = db.entries_for_field("workout", None).unwrap();
    let mut sorted = entries.clone();
    sorted.sort_by_key(|e| e.occur_time.as_ref().unwrap().utc);
    let older = sorted[0].clone();
    let newer = sorted[1].clone();
    assert_eq!(
        older.entry_shape,
        EntryShape::Block,
        "older open should be closed into a block; got shape={:?}",
        older.entry_shape
    );
    assert!(older.dur.is_some(), "older open must have dur");
    assert_eq!(older.verb, Verb::Block, "older's verb re-derives to Block");
    // dur is stored at second precision under v3 (parse_iso_duration_seconds
    // returns i64 secs), so equality between computed_end and the sub-second
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
    assert_eq!(newer.entry_shape, EntryShape::Point);

    // Pending set should be empty now.
    let pending = db
        .list_conflicts(&ListFilter {
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
    let applied = db.auto_resolve_conflicts(chrono::Utc::now(), 0).unwrap();
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

    // Pull the overlap counts the way cmd_now does.
    let tf = [tracked("workout")];
    let mut latest = HashMap::new();
    let entry = db.latest_entry_for_now("workout").unwrap().unwrap();
    latest.insert("workout".to_string(), entry);
    let mut overlap = HashMap::new();
    overlap.insert(
        "workout".to_string(),
        db.open_count_for_field("workout").unwrap(),
    );

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
        .list_conflicts(&ListFilter {
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
    db.resolve_conflict(&target.hid, Resolution::KeepBoth)
        .unwrap();

    let pending = db
        .list_conflicts(&ListFilter {
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
        .list_conflicts(&ListFilter {
            kind: None,
            pending: None,
        })
        .unwrap();
    let target = all[0].clone();
    db.resolve_conflict(&target.hid, Resolution::KeepBoth)
        .unwrap();

    let after = db
        .list_conflicts(&ListFilter {
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
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    assert_eq!(pending.len(), 1);
    let hid = pending[0].hid.clone();
    let local_slug = pending[0].local_entry.slug.clone();

    let resolved = db.resolve_conflict(&hid, Resolution::TakeLocal).unwrap();
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
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::DoubleOpen),
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

    let resolved = db.resolve_conflict(&hid, Resolution::TakeRemote).unwrap();
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
    // The `--edit` flow ultimately calls `db.resolve_conflict_with_edited`
    // with the (possibly mutated) Entry parsed from the editor's saved
    // file. This test exercises that data path directly: build a
    // modified Entry, hand it to the API, and verify the live row was
    // overwritten with the edited content.
    use datum_core::entry::Verb;

    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    let pending = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    let conflict = pending[0].clone();

    // Edit: same slug, but stamp a unique value into `data` so we can
    // tell it landed.
    let mut edited = conflict.local_entry.clone();
    edited.data = Some(serde_json::json!({"edited_marker": "c4e"}));

    let resolved = db
        .resolve_conflict_with_edited(&conflict.hid, &edited)
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

// ─── C4e: show ───────────────────────────────────────────────────────────────

#[test]
fn conflicts_show_renders_diff() {
    // `datum conflicts show <hid>` renders a side-by-side LOCAL/REMOTE
    // diff of the two snapshots, plus a footer pointing at the
    // resolution flags.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older start");
    run_cli(&["start", "workout"], &db).expect("newer start");

    let pending = db
        .list_conflicts(&ListFilter {
            kind: Some(ConflictKind::DoubleOpen),
            pending: Some(true),
        })
        .unwrap();
    let conflict = pending[0].clone();

    let out = render_conflict_show(&conflict);
    assert!(
        out.contains("LOCAL"),
        "header must label LOCAL column:\n{out}"
    );
    assert!(
        out.contains("REMOTE"),
        "header must label REMOTE column:\n{out}"
    );
    assert!(out.contains("workout"), "field name must appear:\n{out}");
    assert!(
        out.contains("--take local|remote"),
        "footer must hint at resolution flags:\n{out}"
    );
    assert!(
        out.contains(&conflict.hid),
        "footer must include the hid for copy/paste:\n{out}"
    );
}

// ─── C4e: now warning ────────────────────────────────────────────────────────

#[test]
fn now_view_shows_conflict_warning_when_pending() {
    // The header helper renders a warning line for non-zero counts and
    // a blank for zero; both forms must satisfy callers that always
    // print `render_pending_conflict_warning(n)`.
    assert_eq!(render_pending_conflict_warning(0), "");

    let one = render_pending_conflict_warning(1);
    assert!(one.contains("1 unresolved conflict"));
    // The pluralized noun "conflicts" must NOT appear in the count
    // phrase — but the CLI hint at the end ("'datum conflicts list'")
    // legitimately uses the plural form. Anchor the singular check on
    // the count-prefix substring.
    assert!(
        !one.contains("1 unresolved conflicts"),
        "singular form for n=1, got: {one:?}"
    );
    assert!(one.contains("datum conflicts list"));

    let many = render_pending_conflict_warning(3);
    assert!(many.contains("3 unresolved conflicts"));
    assert!(many.contains("datum conflicts list"));
}

// ─── C4e: pending_conflict_count ─────────────────────────────────────────────

#[test]
fn pending_conflict_count_excludes_resolved() {
    // The `now` view calls `db.pending_conflict_count` to decide whether
    // to print the header. Make sure resolved rows don't count.
    let db = fresh_db();
    assert_eq!(db.pending_conflict_count().unwrap(), 0);

    run_cli(&["start", "workout", "-t=-5"], &db).expect("older");
    run_cli(&["start", "workout"], &db).expect("newer");
    assert_eq!(db.pending_conflict_count().unwrap(), 1);

    // Resolve it; count drops to 0.
    let pending = db
        .list_conflicts(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    db.resolve_conflict(&pending[0].hid, Resolution::KeepBoth)
        .unwrap();
    assert_eq!(db.pending_conflict_count().unwrap(), 0);
}

// ─── C4e: parser flag plumbing ───────────────────────────────────────────────

#[test]
fn conflicts_show_subcommand_dispatches_via_cli() {
    // End-to-end smoke: the parser routes `conflicts show <hid>`
    // through to render_conflict_show. We can't capture stdout here
    // (run_cli writes to the test process's stdout), but the absence
    // of a parse error is the gate the previous code was missing.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older");
    run_cli(&["start", "workout"], &db).expect("newer");

    let pending = db
        .list_conflicts(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    let hid = pending[0].hid.clone();
    run_cli(&["conflicts", "show", &hid], &db).expect("show dispatch");
}

#[test]
fn conflicts_resolve_take_flag_dispatches_via_cli() {
    // `--take local` round-trips through the parser, dispatcher, and
    // db; we verify the resolution string was written.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older");
    run_cli(&["start", "workout"], &db).expect("newer");

    let pending = db
        .list_conflicts(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    let hid = pending[0].hid.clone();
    run_cli(&["conflicts", "resolve", &hid, "--take", "local"], &db)
        .expect("resolve --take local dispatch");

    let after = db.get_conflict(&hid).unwrap().expect("hid still queryable");
    assert_eq!(after.resolution.as_deref(), Some("take_local"));
}

#[test]
fn conflicts_resolve_skip_leaves_pending() {
    // `--skip` is an explicit acknowledgement that the user is leaving
    // the conflict pending. The row must NOT be marked resolved.
    let db = fresh_db();
    run_cli(&["start", "workout", "-t=-5"], &db).expect("older");
    run_cli(&["start", "workout"], &db).expect("newer");

    let pending = db
        .list_conflicts(&ListFilter {
            kind: None,
            pending: Some(true),
        })
        .unwrap();
    let hid = pending[0].hid.clone();
    run_cli(&["conflicts", "resolve", &hid, "--skip"], &db).expect("resolve --skip dispatch");

    let after = db.get_conflict(&hid).unwrap().expect("hid still queryable");
    assert!(
        after.resolution.is_none(),
        "--skip must leave conflict pending; got {:?}",
        after.resolution
    );
}
