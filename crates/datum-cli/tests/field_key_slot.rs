//! `-F KEY` is the compound-field slot (da-0csuh): a `-k` slot whose value
//! is also appended to the field with `__`, built as the field template
//! `issue__%issue_id%` so the key stays in the data and `update` re-derives
//! field and slug. `--fieldless` is gone: all data has a field.

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::json;

fn err_of(args: &[&str]) -> String {
    let db = fresh_db();
    format!("{:#}", run_cli(args, &db).unwrap_err())
}

#[test]
fn value_is_appended_to_the_field_and_kept_in_data() {
    let db = fresh_db();
    run_cli(
        &["switch", "issue", "pending", "-F", "issue_id", "da_abcd"],
        &db,
    )
    .unwrap();
    let e = &logged_entries(&db, "issue__da_abcd")[0];
    assert_eq!(e.state, Some(json!("pending")));
    assert_eq!(e.data.as_ref().unwrap()["issue_id"], json!("da_abcd"));
    assert_eq!(e.field_structure.as_deref(), Some("issue__%issue_id%"));
}

#[test]
fn update_re_derives_field_and_slug() {
    let db = fresh_db();
    run_cli(&["occur", "issue", "-F", "issue_id", "da_abcd"], &db).unwrap();
    let slug = logged_entries(&db, "issue__da_abcd")[0].slug.clone();
    run_cli(&["update", slug.as_str(), "issue_id=da_xyz"], &db).unwrap();
    assert!(logged_entries(&db, "issue__da_abcd").is_empty());
    let e = &logged_entries(&db, "issue__da_xyz")[0];
    assert!(
        e.slug.as_str().starts_with("issue__da_xyz:"),
        "{}",
        e.slug.as_str()
    );
}

#[test]
fn several_keys_append_in_order() {
    let db = fresh_db();
    run_cli(&["occur", "issue", "-F", "a", "-F", "b", "x", "y"], &db).unwrap();
    let e = &logged_entries(&db, "issue__x__y")[0];
    assert_eq!(e.data, Some(json!({"a": "x", "b": "y"})));
}

#[test]
fn absent_optional_leaves_the_field() {
    let db = fresh_db();
    run_cli(&["occur", "issue", "-F", "a="], &db).unwrap();
    // An empty value is absent too, as it is for `-k a=`.
    run_cli(&["occur", "issue", "-F", "a=", "", "-t", "-1"], &db).unwrap();
    assert_eq!(logged_entries(&db, "issue").len(), 2);
}

#[test]
fn default_fills_the_field_and_a_positional_overrides_it() {
    let db = fresh_db();
    run_cli(&["occur", "issue", "-F", "a=val"], &db).unwrap();
    run_cli(&["occur", "issue", "-F", "a=val", "zz", "-t", "-1"], &db).unwrap();
    assert_eq!(logged_entries(&db, "issue__val").len(), 1);
    assert_eq!(logged_entries(&db, "issue__zz").len(), 1);
}

#[test]
fn missing_required_is_refused() {
    let err = err_of(&["occur", "issue", "-F", "a"]);
    assert!(err.contains("Missing required key: a"), "{err}");
}

#[test]
fn alias_positional_fills_the_slot() {
    let db = fresh_db();
    run_cli(&["alias", "set", "lg", "occur issue -F issue_id"], &db).unwrap();
    run_cli(&["lg", "zz"], &db).unwrap();
    let e = &logged_entries(&db, "issue__zz")[0];
    assert_eq!(e.data, Some(json!({"issue_id": "zz"})));
}

#[test]
fn colon_or_comma_value_is_refused() {
    let err = err_of(&["occur", "issue", "-F", "a", "b:c"]);
    assert!(err.contains("cannot contain ':'"), "{err}");
    let err = err_of(&["occur", "issue", "-F", "a", "b,c"]);
    assert!(err.contains("cannot contain ','"), "{err}");
}

#[test]
fn fieldless_is_gone() {
    let err = err_of(&["occur", "issue", "--fieldless"]);
    assert!(err.contains("unexpected flag: --fieldless"), "{err}");
}
