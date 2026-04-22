//! D4 — CLI acceptance tests for `datum history`, `datum undo <hid>`, and
//! `datum why --full`.

mod common;

use common::run_cli;
use datum_core::db::ConflictStrategy;
use datum_core::test_util::{entry_builder, fresh_db};

#[test]
fn history_command_lists_versions_newest_first() {
    // Dispatching `datum history <hid>` must not error for an entry that has
    // a recorded trail, and the history list must come back newest-first at
    // the library layer (which the command mirrors verbatim).
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .value(serde_json::json!("happy"))
        .insert(&db);

    // Record a user_edit so history has at least two versions to list.
    let mut v2 = v1.clone();
    v2.value = Some(serde_json::json!("sad"));
    db.insert_entry_with_strategy(&v2, ConflictStrategy::Update)
        .expect("v2 user_edit");

    // The command succeeds end-to-end (exercises args parsing + dispatch).
    run_cli(&["history", &v1.hid], &db).expect("history dispatch");

    // Library-level view: newest-first ordering is v2, v1.
    let versions = datum_core::history::list(db.conn(), &v1.hid, true).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].version, 2);
    assert_eq!(versions[1].version, 1);
    assert_eq!(
        versions[0].change_kind,
        datum_core::history::ChangeKind::UserEdit
    );
    assert_eq!(
        versions[1].change_kind,
        datum_core::history::ChangeKind::Create
    );
}

#[test]
fn history_diff_between_versions() {
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .value(serde_json::json!("happy"))
        .insert(&db);

    let mut v2 = v1.clone();
    v2.value = Some(serde_json::json!("sad"));
    db.insert_entry_with_strategy(&v2, ConflictStrategy::Update)
        .expect("v2 user_edit");

    // `--diff 1 2` must dispatch without error.
    run_cli(&["history", &v1.hid, "--diff", "1", "2"], &db).expect("history --diff dispatch");

    // Library-level verification: v1 snapshot has value="happy", v2 pre-edit
    // snapshot also has "happy" (state at v2 is what's in entries: "sad").
    let snap_v1 = datum_core::history::state_at_version(db.conn(), &v1.hid, 1, None)
        .unwrap()
        .expect("v1 state");
    let current = db.get_entry(&v1.slug).unwrap();
    let snap_v2 = datum_core::history::state_at_version(db.conn(), &v1.hid, 2, Some(&current))
        .unwrap()
        .expect("v2 state");
    assert_eq!(snap_v1.value, Some(serde_json::json!("happy")));
    assert_eq!(snap_v2.value, Some(serde_json::json!("sad")));
}

#[test]
fn undo_restores_entry_state() {
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .value(serde_json::json!("happy"))
        .insert(&db);

    // Mutate the entry: v2 carries value="sad".
    let mut v2 = v1.clone();
    v2.value = Some(serde_json::json!("sad"));
    db.insert_entry_with_strategy(&v2, ConflictStrategy::Update)
        .expect("v2 user_edit");

    let current = db.get_entry(&v1.slug).unwrap();
    assert_eq!(current.value, Some(serde_json::json!("sad")));

    // `datum undo <hid>` reverts to v1 (value=happy).
    run_cli(&["undo", &v1.hid], &db).expect("undo dispatch");

    let restored = db.get_entry(&v1.slug).unwrap();
    assert_eq!(restored.value, Some(serde_json::json!("happy")));

    // And the undo itself is recorded as a new version with change_kind='undo'.
    let versions = datum_core::history::list(db.conn(), &v1.hid, false).unwrap();
    assert_eq!(versions.len(), 3);
    assert_eq!(
        versions[2].change_kind,
        datum_core::history::ChangeKind::Undo
    );
}

#[test]
fn why_full_traces_through_history() {
    let db = fresh_db();
    let v1 = entry_builder("mood")
        .verb(datum_core::entry::Verb::Switch)
        .value(serde_json::json!("happy"))
        .insert(&db);

    // Record a second version so --full has something to walk.
    let mut v2 = v1.clone();
    v2.value = Some(serde_json::json!("sad"));
    db.insert_entry_with_strategy(&v2, ConflictStrategy::Update)
        .expect("v2 user_edit");

    // The command must dispatch cleanly; its output goes to stdout, which
    // we don't capture here — we assert the plumbing (library view) is in
    // the shape the command consumes.
    run_cli(&["why", &v1.hid, "--full"], &db).expect("why --full dispatch");

    let versions = datum_core::history::list(db.conn(), &v1.hid, true).unwrap();
    assert_eq!(
        versions.len(),
        2,
        "why --full walks the trail; it must not be empty"
    );
    // Newest-first: v2 is the user_edit, v1 is the create.
    assert_eq!(
        versions[0].change_kind,
        datum_core::history::ChangeKind::UserEdit
    );
    assert_eq!(
        versions[1].change_kind,
        datum_core::history::ChangeKind::Create
    );
}
