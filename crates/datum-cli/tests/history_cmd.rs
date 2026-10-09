//! D4 — CLI acceptance tests for `datum history`, `datum undo <hid>`, and
//! `datum why --full`.

use crate::common;

use common::run_cli;
use datum_core::test_util::{entry_builder, fresh_db};

#[test]
fn history_command_lists_versions_newest_first() {
    // Dispatching `datum history <hid>` must not error for an entry that has
    // history, and what it reads — the entry's ops, a version each
    // (da-s4erq) — holds the create and the edit.
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .state(serde_json::json!("happy"))
        .insert(&db);

    // Record a user_edit so history has at least two versions to list.
    let mut v2 = v1.clone();
    // v3 (da-5ns): primary state lives in `entry.state`, so a direct
    // EntryBuilder edit round-trips through the v=20 schema.
    v2.state = Some(serde_json::json!("sad"));
    // Record a user_edit via the slug-keyed save path. (The Update
    // strategy is content-hash-based and is the wrong shape here: v1 and
    // v2 carry different state and so have distinct hashes.)
    db.save_existing_entry(&v2).expect("v2 user_edit");

    // The command succeeds end-to-end (exercises args parsing + dispatch).
    run_cli(&["history", &v1.hid], &db).expect("history dispatch");

    // What the command lists, oldest first: the create, then the edit.
    let versions = db.versions(&v1).unwrap();
    let kinds: Vec<&str> = versions.iter().map(|v| v.kind.as_str()).collect();
    assert_eq!(kinds, ["create", "update"]);
    assert_eq!(versions[1].version, 2);
}

#[test]
fn history_diff_between_versions() {
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .state(serde_json::json!("happy"))
        .insert(&db);

    let mut v2 = v1.clone();
    // v3 (da-5ns): primary state lives in `entry.state`, so a direct
    // EntryBuilder edit round-trips through the v=20 schema.
    v2.state = Some(serde_json::json!("sad"));
    // Record a user_edit via the slug-keyed save path. (The Update
    // strategy is content-hash-based and is the wrong shape here: v1 and
    // v2 carry different state and so have distinct hashes.)
    db.save_existing_entry(&v2).expect("v2 user_edit");

    // `--diff 1 2` must dispatch without error.
    run_cli(&["history", &v1.hid, "--diff", "1", "2"], &db).expect("history --diff dispatch");

    // What it diffs: the entry as each version left it.
    let state = |n: u32| db.entry_at_version(&v1, n).unwrap().unwrap().state;
    assert_eq!(state(1), Some(serde_json::json!("happy")));
    assert_eq!(state(2), Some(serde_json::json!("sad")));
}

#[test]
fn undo_restores_entry_state() {
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .state(serde_json::json!("happy"))
        .insert(&db);

    // Mutate the entry: v2 carries value="sad".
    let mut v2 = v1.clone();
    // v3 (da-5ns): primary state lives in `entry.state`, so a direct
    // EntryBuilder edit round-trips through the v=20 schema.
    v2.state = Some(serde_json::json!("sad"));
    // Record a user_edit via the slug-keyed save path. (The Update
    // strategy is content-hash-based and is the wrong shape here: v1 and
    // v2 carry different state and so have distinct hashes.)
    db.save_existing_entry(&v2).expect("v2 user_edit");

    let current = db.get_entry(&v1.slug).unwrap();
    assert_eq!(current.state, Some(serde_json::json!("sad")));

    // `datum undo <hid>` reverts to v1 (value=happy).
    run_cli(&["undo", &v1.hid], &db).expect("undo dispatch");

    let restored = db.get_entry(&v1.slug).unwrap();
    assert_eq!(restored.state, Some(serde_json::json!("happy")));

    // And the undo itself is recorded as a new version with change_kind='undo'.
    let versions = datum_core::history::list(db.conn(), &v1.hid, false).unwrap();
    assert_eq!(versions.len(), 3);
    assert_eq!(
        versions[2].change_kind,
        datum_core::history::ChangeKind::Undo
    );
}

#[test]
fn bare_undo_of_just_created_entry_deletes_it() {
    // Regression (da-5hdz): `undo` on a freshly-created entry that has no
    // prior version must soft-delete it (undoing creation = deletion), not
    // error with "no prior version recorded".
    let db = fresh_db();

    // Create via the CLI so the command is on the undo stack, exercising the
    // bare-`undo` path (da-z0zh3).
    run_cli(&["occur", "testfield"], &db).expect("occur dispatch");
    let tombstone = |db: &datum_core::db::Db| {
        datum_core::test_util::logged_all(db)
            .into_iter()
            .find(|e| e.deleted_at.is_some())
    };
    assert!(tombstone(&db).is_none(), "no tombstone before undo");

    // Bare `undo` must succeed and tombstone the just-created entry.
    run_cli(&["undo"], &db).expect("undo of just-created entry must not error");

    let tomb = tombstone(&db).expect("entry is tombstoned after undo");

    // Symmetry: a second `undo` on the same hid restores it.
    run_cli(&["undo", &tomb.hid], &db).expect("re-undo restores");
    let restored = db.get_entry(&tomb.slug).unwrap();
    assert!(
        restored.deleted_at.is_none(),
        "second undo restores the entry"
    );
}

#[test]
fn why_full_traces_through_history() {
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .state(serde_json::json!("happy"))
        .insert(&db);

    // Record a second version so --full has something to walk.
    let mut v2 = v1.clone();
    // v3 (da-5ns): primary state lives in `entry.state`, so a direct
    // EntryBuilder edit round-trips through the v=20 schema.
    v2.state = Some(serde_json::json!("sad"));
    // Record a user_edit via the slug-keyed save path. (The Update
    // strategy is content-hash-based and is the wrong shape here: v1 and
    // v2 carry different state and so have distinct hashes.)
    db.save_existing_entry(&v2).expect("v2 user_edit");

    // The command must dispatch cleanly; its output goes to stdout, which
    // we don't capture here — we assert what it walks is there to walk.
    run_cli(&["why", &v1.hid, "--full"], &db).expect("why --full dispatch");

    let versions = db.versions(&v1).unwrap();
    let kinds: Vec<&str> = versions.iter().map(|v| v.kind.as_str()).collect();
    assert_eq!(kinds, ["create", "update"], "why --full walks the versions");
}

/// da-96yy: `undo --to-version` must bounds-check the target against the
/// entry's version range [1, latest]. Out-of-range versions used to misbehave
/// silently — v>latest no-op'd, v=0 reverted to the create-state — instead of
/// erroring. Both edges must now bail without touching the entry.
#[test]
fn undo_to_version_out_of_range_errors_and_leaves_entry_untouched() {
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .state(serde_json::json!("happy"))
        .insert(&db);

    // Give the entry a second version so latest=2.
    let mut v2 = v1.clone();
    v2.state = Some(serde_json::json!("sad"));
    db.save_existing_entry(&v2).expect("v2 user_edit");

    let live_before = db.get_entry(&v1.slug).unwrap();
    assert_eq!(live_before.state, Some(serde_json::json!("sad")));

    // v > latest: used to silently no-op. Must error now.
    let err = run_cli(&["undo", &v1.hid, "--to-version", "99"], &db)
        .expect_err("--to-version above latest must error");
    assert!(
        err.to_string().contains("doesn't exist"),
        "error should name the missing version, got: {err}"
    );

    // v = 0: used to silently revert to the create-state. Must error now.
    let err0 = run_cli(&["undo", &v1.hid, "--to-version", "0"], &db)
        .expect_err("--to-version 0 must error");
    assert!(
        err0.to_string().contains("doesn't exist"),
        "error should name the missing version, got: {err0}"
    );

    // Neither failed attempt mutated the entry or its trail.
    let live_after = db.get_entry(&v1.slug).unwrap();
    assert_eq!(
        live_after.state,
        Some(serde_json::json!("sad")),
        "out-of-range undo must not change the entry"
    );
    let versions = datum_core::history::list(db.conn(), &v1.hid, false).unwrap();
    assert_eq!(
        versions.len(),
        2,
        "no new version recorded on a failed undo"
    );
}

/// da-96yy: the in-range edge `--to-version <latest>` must keep working — it
/// resolves to the live row and reverts to that (current) state.
#[test]
fn undo_to_version_at_latest_is_in_range() {
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .state(serde_json::json!("happy"))
        .insert(&db);
    let mut v2 = v1.clone();
    v2.state = Some(serde_json::json!("sad"));
    db.save_existing_entry(&v2).expect("v2 user_edit");

    // latest == 2 is in range and must dispatch without error.
    run_cli(&["undo", &v1.hid, "--to-version", "2"], &db)
        .expect("--to-version at latest must succeed");

    // Reverting to v1 must still work too.
    run_cli(&["undo", &v1.hid, "--to-version", "1"], &db).expect("--to-version 1 must succeed");
    let restored = db.get_entry(&v1.slug).unwrap();
    assert_eq!(restored.state, Some(serde_json::json!("happy")));
}
