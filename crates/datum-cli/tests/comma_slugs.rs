//! An array in an `--id` slot slugs as relaxed JSON, a path indexes it, and
//! a slug holding a comma is one id before a comma list splits (da-7nv8r).

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::json;

fn slugs(db: &datum_core::db::Db, field: &str) -> Vec<String> {
    logged_entries(db, field)
        .into_iter()
        .map(|e| e.slug.to_string())
        .collect()
}

#[test]
fn two_comments_slug_as_relaxed_json() {
    let db = fresh_db();
    let args = [
        "occur",
        "tx",
        "--id",
        "%comment%",
        "-c",
        "electricity",
        "-c",
        "paid",
    ];
    run_cli(&args, &db).unwrap();
    assert_eq!(slugs(&db, "tx"), ["tx:[electricity,paid]"]);
}

#[test]
fn a_path_indexes_the_comments() {
    let db = fresh_db();
    let args = ["-c", "a", "-c", "b", "-c", "c"];
    run_cli(
        &[&["occur", "x", "--id", "%comment.0%"][..], &args].concat(),
        &db,
    )
    .unwrap();
    run_cli(
        &[&["occur", "y", "--id", "%comment.-1%"][..], &args].concat(),
        &db,
    )
    .unwrap();
    assert_eq!(
        (slugs(&db, "x"), slugs(&db, "y")),
        (vec!["x:a".to_string()], vec!["y:c".to_string()])
    );
}

/// The whole argument is tried as a slug before it splits, in every verb
/// that reads a comma list; a list that is no slug still splits.
#[test]
fn a_slug_with_commas_is_one_id() {
    let db = fresh_db();
    let tx = [
        "occur",
        "tx",
        "--id",
        "%comment%",
        "-c",
        "electricity",
        "-c",
        "paid",
    ];
    run_cli(&tx, &db).unwrap();
    run_cli(&["occur", "a"], &db).unwrap();
    run_cli(&["occur", "b"], &db).unwrap();
    let slug = "tx:[electricity,paid]";
    let ok = |args: &[&str]| run_cli(args, &db).is_ok();
    assert_eq!(
        [
            ok(&["get", slug]),
            ok(&["get", "a:_,b:_"]),
            ok(&["history", slug]),
            ok(&["why", slug]),
            ok(&["update", slug, "n=1"]),
            ok(&["undo", slug]),
            ok(&["redo", slug]),
            ok(&["delete", slug]),
        ],
        [true; 8],
    );
    assert!(
        logged_entries(&db, "tx").is_empty(),
        "delete found the slug"
    );
}

/// A stored `"comment": null` is no comment: `update -c` starts the list.
#[test]
fn update_c_on_a_null_comment() {
    let db = fresh_db();
    let e = datum_core::test_util::entry_builder("x")
        .extra_data(json!({"comment": null}))
        .insert(&db);
    run_cli(&["update", e.hid.as_str(), "-c", "x"], &db).unwrap();
    assert_eq!(
        logged_entries(&db, "x")[0].data,
        Some(json!({"comment": ["x"]}))
    );
}

/// The id follows the data: `update -c` re-slugs the entry.
#[test]
fn update_re_slugs_the_list() {
    let db = fresh_db();
    let tx = [
        "occur",
        "tx",
        "--id",
        "%comment%",
        "-c",
        "electricity",
        "-c",
        "paid",
    ];
    run_cli(&tx, &db).unwrap();
    run_cli(&["update", "tx:[electricity,paid]", "-c", "late"], &db).unwrap();
    assert_eq!(slugs(&db, "tx"), ["tx:[electricity,paid,late]"]);
}

/// `reparse` reads its ids through the same list as every other verb.
#[test]
fn reparse_takes_a_slug_with_commas() {
    let db = fresh_db();
    let tx = [
        "occur",
        "tx",
        "--id",
        "%comment%",
        "-c",
        "electricity",
        "-c",
        "paid",
    ];
    run_cli(&tx, &db).unwrap();
    run_cli(&["reparse", "tx:[electricity,paid]"], &db).unwrap();
}
