//! A state refers to another entry (da-f9jyv): one holding `:` is a full
//! slug `field:id`, a bare one on a field whose spec names a
//! `state_lookup_field` F refers to `F:<state>`, and `datum check` warns of
//! a reference that names no live entry. Nothing is refused at write.

use crate::common;

use common::run_cli;
use datum_commands::commands::check::collect_dangling_refs;
use datum_core::db::Db;
use datum_core::kit::KitToml;
use datum_core::test_util::fresh_db;
use serde_json::json;

/// The (detail-free) findings as `field → slug xcount`, sorted.
fn dangling(db: &Db) -> Vec<String> {
    collect_dangling_refs(db, &db.list_including_deleted().unwrap())
        .unwrap()
        .iter()
        .map(|i| {
            assert_eq!(i.category, "dangling_state_ref");
            assert_eq!(
                i.severity,
                datum_commands::commands::check::Severity::Warning
            );
            let slug = i
                .message
                .split(" to ")
                .nth(1)
                .unwrap()
                .split(',')
                .next()
                .unwrap();
            format!(
                "{} → {slug} x{}",
                i.example.as_ref().unwrap().field.as_ref().unwrap(),
                i.count
            )
        })
        .collect()
}

fn issue_db() -> Db {
    let db = fresh_db();
    run_cli(
        &[
            "setup",
            "issue",
            "--verb",
            "switch",
            "--state-lookup-field",
            "issue_state",
        ],
        &db,
    )
    .unwrap();
    db
}

#[test]
fn a_full_slug_dangles_until_its_entry_exists() {
    let db = fresh_db();
    run_cli(&["switch", "media", "book:great_gatsby", "-q"], &db).expect("never refused");
    run_cli(&["switch", "media", "book:great_gatsby"], &db).unwrap();
    run_cli(&["switch", "clock", "12:30"], &db).unwrap();
    // Control: a plain state on a field with no lookup field is no reference.
    run_cli(&["switch", "mood", "happy"], &db).unwrap();
    assert_eq!(
        dangling(&db),
        ["clock → 12:30 x1", "media → book:great_gatsby x2"],
        "one finding per (field, slug), with its count"
    );

    run_cli(&["add", "book", "id=great_gatsby"], &db).unwrap();
    assert_eq!(dangling(&db), ["clock → 12:30 x1"]);

    // A tombstoned document is missing again.
    run_cli(&["delete", "book:great_gatsby"], &db).unwrap();
    assert_eq!(
        dangling(&db),
        ["clock → 12:30 x1", "media → book:great_gatsby x2"]
    );
}

#[test]
fn a_bare_state_resolves_through_the_lookup_field() {
    let db = issue_db();
    // Each write at its own minute: two at one millisecond share a slug.
    run_cli(&["switch", "issue", "queued", "-t", "-50"], &db).expect("never refused");
    // An array state per element; `:` inside wins over the lookup field.
    run_cli(&["switch", "issue", "done,book:x", "-t", "-40"], &db).unwrap();
    // Sentinels and non-strings never refer.
    run_cli(&["switch", "issue", "IGNORED", "-t", "-30"], &db).unwrap();
    run_cli(&["switch", "issue", "TRACKED", "-t", "-20"], &db).unwrap();
    run_cli(&["end", "issue", "-t", "-10"], &db).unwrap();
    // Control: the same bare state on a field without a lookup field.
    run_cli(&["switch", "other", "queued"], &db).unwrap();
    assert_eq!(
        dangling(&db),
        [
            "issue → book:x x1",
            "issue → issue_state:done x1",
            "issue → issue_state:queued x1",
        ]
    );

    run_cli(&["add", "issue_state", "id=queued", "class=waiting"], &db).unwrap();
    run_cli(&["add", "issue_state", "id=done"], &db).unwrap();
    run_cli(&["add", "book", "id=x"], &db).unwrap();
    assert!(dangling(&db).is_empty(), "{:?}", dangling(&db));
}

/// `setup --state-lookup-field` writes the key, `setup F --json`'s JSON and
/// `--from-json` carry it, a kit installs and exports it, and `''` clears.
#[test]
fn state_lookup_field_round_trip() {
    let db = issue_db();
    let spec = db.load_spec("issue").unwrap().unwrap();
    let value = serde_json::to_value(&spec).unwrap();
    assert_eq!(value["state_lookup_field"], json!("issue_state"));

    let from_json = fresh_db();
    run_cli(
        &["setup", "issue", "--from-json", &value.to_string()],
        &from_json,
    )
    .unwrap();
    let back = from_json.load_spec("issue").unwrap().unwrap();
    assert_eq!(back.state_lookup_field.as_deref(), Some("issue_state"));

    let dir = tempfile::tempdir().unwrap();
    let kit = dir.path().join("k.toml");
    let toml = KitToml::from_field_specs("k", None, std::slice::from_ref(&spec), &[])
        .to_toml()
        .unwrap();
    assert!(
        toml.contains("state_lookup_field = \"issue_state\""),
        "{toml}"
    );
    std::fs::write(&kit, toml).unwrap();
    let installed = fresh_db();
    run_cli(&["kit", "install", &kit.to_string_lossy()], &installed).unwrap();
    let out = dir.path().join("out.toml");
    run_cli(&["kit", "export", "k", &out.to_string_lossy()], &installed).unwrap();
    let again = fresh_db();
    run_cli(&["kit", "install", &out.to_string_lossy()], &again).unwrap();
    let spec = again.load_spec("issue").unwrap().unwrap();
    assert_eq!(spec.state_lookup_field.as_deref(), Some("issue_state"));

    run_cli(&["setup", "issue", "--state-lookup-field", ""], &db).unwrap();
    let cleared = serde_json::to_value(db.load_spec("issue").unwrap().unwrap()).unwrap();
    assert_eq!(
        cleared.get("state_lookup_field"),
        None,
        "absent once cleared"
    );
}
