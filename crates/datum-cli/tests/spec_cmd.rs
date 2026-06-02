//! `datum spec set` integration tests for da-dqb position flags
//! (`--y`, `--height`, `--z-index`).

mod common;

use common::run_cli;
use datum_core::field_spec::DEFAULT_FIELD_HEIGHT;
use datum_core::test_util::fresh_db;

#[test]
fn spec_set_persists_position_flags() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "sleep",
            "--default-verb",
            "start",
            "--y",
            "0.05",
            "--height",
            "0.20",
            "--z-index",
            "1",
        ],
        &db,
    )
    .expect("spec set with position flags");

    let spec = db.load_spec("sleep").unwrap().expect("spec persisted");
    assert_eq!(spec.presentation.y, Some(0.05));
    assert_eq!(spec.presentation.height, Some(0.20));
    assert_eq!(spec.presentation.z_index, Some(1.0));

    let pos = spec.presentation.resolve_position("sleep");
    assert_eq!(pos.y, 0.05);
    assert_eq!(pos.height, 0.20);
    assert_eq!(pos.z_index, 1.0);
}

#[test]
fn spec_set_position_flags_merge_into_existing_spec() {
    let db = fresh_db();

    // First call: set default-verb only. Position fields stay None.
    run_cli(&["spec", "set", "focus", "--default-verb", "switch"], &db).expect("first spec set");
    let spec = db.load_spec("focus").unwrap().unwrap();
    assert!(spec.presentation.y.is_none());
    assert!(spec.presentation.height.is_none());
    assert!(spec.presentation.z_index.is_none());

    // Second call: only --y. Existing default_verb must survive; the other
    // position fields must remain None (so the resolver continues to apply
    // defaults).
    run_cli(&["spec", "set", "focus", "--y", "0.42"], &db).expect("merge --y");
    let spec = db.load_spec("focus").unwrap().unwrap();
    assert_eq!(spec.presentation.y, Some(0.42));
    assert!(spec.presentation.height.is_none());
    assert!(spec.presentation.z_index.is_none());

    // Resolver applies defaults for the missing fields.
    let pos = spec.presentation.resolve_position("focus");
    assert_eq!(pos.y, 0.42);
    assert_eq!(pos.height, DEFAULT_FIELD_HEIGHT);
    assert_eq!(pos.z_index, 0.42);
}

#[test]
fn spec_set_overlap_two_fields_share_y_with_distinct_z_index() {
    let db = fresh_db();

    run_cli(
        &[
            "spec",
            "set",
            "sleep",
            "--default-verb",
            "start",
            "--y",
            "0.05",
            "--height",
            "0.20",
            "--z-index",
            "1",
        ],
        &db,
    )
    .unwrap();
    run_cli(
        &[
            "spec",
            "set",
            "focus",
            "--default-verb",
            "switch",
            "--y",
            "0.05",
            "--height",
            "0.20",
            "--z-index",
            "2",
        ],
        &db,
    )
    .unwrap();

    let sleep = db.load_spec("sleep").unwrap().unwrap();
    let focus = db.load_spec("focus").unwrap().unwrap();
    let s = sleep.presentation.resolve_position("sleep");
    let f = focus.presentation.resolve_position("focus");
    // Both fields occupy the same vertical region (intentional, artistic).
    assert_eq!(s.y, f.y);
    assert_eq!(s.height, f.height);
    // focus stacks above sleep.
    assert!(f.z_index > s.z_index);
}

#[test]
fn spec_set_attached_value_form_works_for_position_flags() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "energy",
            "--default-verb",
            "switch",
            "--y=0.30",
            "--height=0.10",
            "--z-index=3",
        ],
        &db,
    )
    .expect("attached-value form");

    let spec = db.load_spec("energy").unwrap().unwrap();
    assert_eq!(spec.presentation.y, Some(0.30));
    assert_eq!(spec.presentation.height, Some(0.10));
    assert_eq!(spec.presentation.z_index, Some(3.0));
}

// ─── da-zwqk: per-state descriptions ──────────────────────────────────

#[test]
fn spec_set_persists_state_descriptions() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "mood",
            "--default-verb",
            "switch",
            "--state",
            "happy",
            "--state-description",
            "happy=feeling good",
            "--state",
            "sad",
            "--state-description=sad=down or flat",
        ],
        &db,
    )
    .expect("spec set with state descriptions");

    let spec = db.load_spec("mood").unwrap().expect("spec persisted");
    assert_eq!(
        spec.state_descriptions.get("happy").map(String::as_str),
        Some("feeling good"),
    );
    assert_eq!(
        spec.state_descriptions.get("sad").map(String::as_str),
        Some("down or flat"),
    );
}

#[test]
fn spec_set_state_description_with_empty_text_clears_entry() {
    let db = fresh_db();
    run_cli(
        &[
            "spec",
            "set",
            "mood",
            "--default-verb",
            "switch",
            "--state",
            "happy",
            "--state-description",
            "happy=initial meaning",
        ],
        &db,
    )
    .expect("seed description");
    run_cli(
        &["spec", "set", "mood", "--state-description", "happy="],
        &db,
    )
    .expect("clear with STATE=");

    let spec = db.load_spec("mood").unwrap().expect("spec persisted");
    assert!(
        !spec.state_descriptions.contains_key("happy"),
        "STATE= form should clear the description",
    );
}
