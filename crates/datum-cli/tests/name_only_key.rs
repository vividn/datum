//! A field template's `%-key%` fills the name only (da-n1rcs): the key
//! builds the field and is not stored, its value being in the name.

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::json;

#[test]
fn a_stage_change_carries_no_id_its_field_names() {
    let db = fresh_db();
    let start = "switch issue__%-issue_id% in_progress -k issue_id -k by";
    run_cli(&["alias", "set", "istart", start], &db).unwrap();
    run_cli(
        &[
            "alias",
            "set",
            "iqueue",
            "switch issue__%-issue_id% queued -k issue_id",
        ],
        &db,
    )
    .unwrap();
    run_cli(&["iqueue", "ex7nmrmh"], &db).unwrap();
    run_cli(&["istart", "ex7nmrmh", "garnet"], &db).unwrap();

    let rows = logged_entries(&db, "issue__ex7nmrmh");
    let mut seen: Vec<_> = rows
        .iter()
        .map(|e| (e.state.clone(), e.data.clone()))
        .collect();
    seen.sort_by_key(|(state, _)| state.as_ref().map(ToString::to_string));
    assert_eq!(
        seen,
        [
            (Some(json!("in_progress")), Some(json!({"by": "garnet"}))),
            (Some(json!("queued")), None),
        ]
    );
    // Nothing is left to re-derive the name from: it is a plain field.
    assert!(rows.iter().all(|e| e.field_structure.is_none()));
}

#[test]
fn the_other_placeholders_stay_a_template() {
    let db = fresh_db();
    run_cli(&["occur", "w__%-a%__%b%", "a=x", "b=y"], &db).unwrap();
    let e = &logged_entries(&db, "w__x__y")[0];
    assert_eq!(e.data, Some(json!({"b": "y"})));
    assert_eq!(e.field_structure.as_deref(), Some("w__x__%b%"));

    run_cli(&["update", e.slug.as_str(), "b=z"], &db).unwrap();
    assert_eq!(logged_entries(&db, "w__x__z").len(), 1);
}
