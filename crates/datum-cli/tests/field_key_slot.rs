//! `-F KEY` is the compound-field slot (da-0csuh): a `-k` slot whose value
//! is appended to the field with `__` and names it only (da-vk747): the key
//! is not stored, so no `update` of it moves the entry. `%-key%`, which did
//! the same in a field template (da-n1rcs), is refused for it, and in an id
//! template, where a stored one still reads as `%key%`. `--fieldless` is
//! gone: all data has a field.

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::json;

fn err_of(args: &[&str]) -> String {
    let db = fresh_db();
    format!("{:#}", run_cli(args, &db).unwrap_err())
}

#[test]
fn value_is_appended_to_the_field_and_not_stored() {
    let db = fresh_db();
    run_cli(
        &["switch", "issue", "pending", "-F", "issue_id", "da_abcd"],
        &db,
    )
    .unwrap();
    let e = &logged_entries(&db, "issue__da_abcd")[0];
    assert_eq!(e.state, Some(json!("pending")));
    assert_eq!(e.data, None);
    // Nothing is left to re-derive the name from: it is a plain field.
    assert_eq!(e.field_structure, None);
}

#[test]
fn a_stage_change_carries_no_id_its_field_names() {
    let db = fresh_db();
    let istart = "switch issue -F issue_id in_progress -k by";
    run_cli(&["alias", "set", "istart", istart], &db).unwrap();
    let iqueue = "switch issue -F issue_id queued";
    run_cli(&["alias", "set", "iqueue", iqueue], &db).unwrap();
    run_cli(&["iqueue", "ex7nmrmh"], &db).unwrap();
    run_cli(&["istart", "ex7nmrmh", "jade"], &db).unwrap();
    let mut seen: Vec<_> = logged_entries(&db, "issue__ex7nmrmh")
        .into_iter()
        .map(|e| (e.state, e.data))
        .collect();
    seen.sort_by_key(|(state, _)| state.as_ref().map(ToString::to_string));
    assert_eq!(
        seen,
        [
            (Some(json!("in_progress")), Some(json!({"by": "jade"}))),
            (Some(json!("queued")), None),
        ]
    );
}

#[test]
fn reparse_finds_no_drift() {
    let db = fresh_db();
    run_cli(&["occur", "issue", "-F", "issue_id", "da_abcd"], &db).unwrap();
    let hid = logged_entries(&db, "issue__da_abcd")[0].hid.to_string();
    run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
    let e = db.get_entry(&hid).unwrap();
    assert_eq!((e.field.as_str(), e.data), ("issue__da_abcd", None));
}

#[test]
fn a_k_of_the_same_name_keeps_it_as_data() {
    let db = fresh_db();
    run_cli(&["occur", "issue", "-F", "a", "-k", "a", "x"], &db).unwrap();
    let e = &logged_entries(&db, "issue__x")[0];
    assert_eq!(e.data, Some(json!({"a": "x"})));
    assert_eq!(e.field_structure.as_deref(), Some("issue__%a%"));
}

#[test]
fn the_other_placeholders_stay_a_template() {
    let db = fresh_db();
    run_cli(&["occur", "w__%b%", "-F", "a", "x", "b=y"], &db).unwrap();
    let e = &logged_entries(&db, "w__y__x")[0];
    assert_eq!(e.data, Some(json!({"b": "y"})));
    assert_eq!(e.field_structure.as_deref(), Some("w__%b%__x"));

    run_cli(&["update", e.slug.as_str(), "b=z"], &db).unwrap();
    assert_eq!(logged_entries(&db, "w__z__x").len(), 1);
}

#[test]
fn a_name_only_placeholder_is_refused_naming_f() {
    for field in ["issue__%-issue_id%", "issue__%-?issue_id%"] {
        let err = err_of(&["occur", field, "issue_id=x"]);
        assert!(err.contains("is retired"), "{err}");
        assert!(err.contains("`-F issue_id`"), "{err}");
    }
}

#[test]
fn a_name_only_placeholder_is_refused_in_an_id_template() {
    let err = err_of(&["occur", "idt", "--id", "%-a%", "a=q"]);
    assert!(err.contains("%-a% is retired. Write %a%."), "{err}");
    let err = err_of(&["setup", "idt", "--verb", "occur", "--id-template", "%-?a%"]);
    assert!(err.contains("%-?a% is retired. Write %a%."), "{err}");
}

#[test]
fn update_stores_the_key_as_data_and_moves_nothing() {
    let db = fresh_db();
    run_cli(&["occur", "issue", "-F", "issue_id", "da_abcd"], &db).unwrap();
    let slug = logged_entries(&db, "issue__da_abcd")[0].slug.clone();
    run_cli(&["update", slug.as_str(), "issue_id=da_xyz"], &db).unwrap();
    assert!(logged_entries(&db, "issue__da_xyz").is_empty());
    let e = &logged_entries(&db, "issue__da_abcd")[0];
    assert_eq!(e.slug, slug);
    assert_eq!(e.data, Some(json!({"issue_id": "da_xyz"})));
}

#[test]
fn several_keys_append_in_order() {
    let db = fresh_db();
    run_cli(&["occur", "issue", "-F", "a", "-F", "b", "x", "y"], &db).unwrap();
    let e = &logged_entries(&db, "issue__x__y")[0];
    assert_eq!(e.data, None);
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
    assert_eq!(e.data, None);
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
