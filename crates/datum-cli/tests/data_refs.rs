//! A data value refers to another entry (da-s5k2s): a string beginning
//! `ref://` is a reference, stored as typed; a literal one is stored
//! `str://ref://…`; on a key the spec types `ref` a bare slug is one; and
//! `datum check refs` warns of one that names no live entry.

use crate::common;

use common::run_cli;
use datum_commands::commands::check::collect_dangling_refs;
use datum_core::db::Db;
use datum_core::field_spec::SubfieldType;
use datum_core::kit::KitToml;
use datum_core::test_util::fresh_db;
use serde_json::json;

/// The data-ref findings as `field.key → slug xcount`, sorted.
fn dangling(db: &Db) -> Vec<String> {
    collect_dangling_refs(db)
        .unwrap()
        .iter()
        .filter(|i| i.category == "dangling_data_ref")
        .map(|i| {
            let slug = i.message.split(" to ").nth(1).unwrap();
            let slug = slug.split(',').next().unwrap();
            let key = i.message.rsplit("(key ").next().unwrap();
            format!(
                "{}.{} → {slug} x{}",
                i.example.as_ref().unwrap().field.as_ref().unwrap(),
                key.trim_end_matches(')'),
                i.count
            )
        })
        .collect()
}

fn data(db: &Db, slug: &str) -> serde_json::Value {
    db.get_entry(slug).unwrap().data.unwrap()
}

/// A `task` spec whose `blocker` key is typed `ref`.
fn typed_db() -> Db {
    let db = fresh_db();
    let spec =
        json!({"field": "task", "verb": "occur", "subfields": {"blocker": {"value_type": "ref"}}});
    run_cli(
        &["spec", "set", "task", "--from-json", &spec.to_string()],
        &db,
    )
    .unwrap();
    db
}

#[test]
fn the_marker_is_stored_as_typed_and_both_literals_as_str() {
    let db = fresh_db();
    run_cli(
        &[
            "add",
            "x",
            "id=1",
            "needs=ref://issue:k7f2a",
            "lit=str://ref://issue:k7f2a",
            r#"quoted="ref://issue:k7f2a""#,
            "plain=issue:k7f2a",
            "many=,ref://a:1,ref://b:2,",
        ],
        &db,
    )
    .unwrap();
    assert_eq!(
        data(&db, "x:1"),
        json!({
            "needs": "ref://issue:k7f2a",
            "lit": "str://ref://issue:k7f2a",
            "quoted": "str://ref://issue:k7f2a",
            "plain": "issue:k7f2a",
            "many": ["ref://a:1", "ref://b:2"],
        })
    );
}

#[test]
fn check_flags_a_missing_target_until_it_exists() {
    let db = typed_db();
    // Each write at its own minute: two at one millisecond share a slug.
    run_cli(&["occur", "x", "needs=ref://issue:k7f2a", "-t", "-50"], &db).expect("never refused");
    run_cli(&["occur", "x", "needs=ref://issue:k7f2a", "-t", "-40"], &db).unwrap();
    run_cli(
        &["occur", "x", "many=,ref://a:1,ref://b:2,", "-t", "-30"],
        &db,
    )
    .unwrap();
    run_cli(&["occur", "task", "blocker=issue:k7f2a"], &db).unwrap();
    // Controls: a literal, and a bare slug on a key with no ref type — on
    // another field, and on another key of the typed one.
    run_cli(
        &["occur", "x", r#"lit="ref://issue:gone""#, "-t", "-20"],
        &db,
    )
    .unwrap();
    run_cli(&["occur", "y", "blocker=issue:gone"], &db).unwrap();
    run_cli(&["occur", "task", "other=issue:gone", "-t", "-10"], &db).unwrap();
    assert_eq!(
        dangling(&db),
        [
            "task.blocker → issue:k7f2a x1",
            "x.many → a:1 x1",
            "x.many → b:2 x1",
            "x.needs → issue:k7f2a x2",
        ],
        "one finding per (field, key, slug), with its count"
    );
    // A data ref does not show as a state ref.
    assert!(collect_dangling_refs(&db)
        .unwrap()
        .iter()
        .all(|i| i.category == "dangling_data_ref"));

    run_cli(&["add", "issue", "id=k7f2a"], &db).unwrap();
    run_cli(&["add", "a", "id=1"], &db).unwrap();
    assert_eq!(dangling(&db), ["x.many → b:2 x1"]);
    run_cli(&["add", "b", "id=2"], &db).unwrap();
    assert!(dangling(&db).is_empty(), "{:?}", dangling(&db));
}

/// `value_type = "ref"` goes through `--from-json` and a kit's install and
/// export like any subfield type.
#[test]
fn value_type_ref_round_trip() {
    let db = typed_db();
    let spec = db.load_spec("task").unwrap().unwrap();
    assert!(matches!(
        spec.subfields["blocker"].value_type,
        SubfieldType::Ref
    ));
    let dir = tempfile::tempdir().unwrap();
    let kit = dir.path().join("k.toml");
    let toml = KitToml::from_field_specs("k", None, std::slice::from_ref(&spec), &[])
        .to_toml()
        .unwrap();
    assert!(toml.contains("value_type = \"ref\""), "{toml}");
    std::fs::write(&kit, toml).unwrap();
    let installed = fresh_db();
    run_cli(&["kit", "install", &kit.to_string_lossy()], &installed).unwrap();
    let back = installed.load_spec("task").unwrap().unwrap();
    assert!(matches!(
        back.subfields["blocker"].value_type,
        SubfieldType::Ref
    ));
}
