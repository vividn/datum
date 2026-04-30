//! End-to-end kit install → export round-trip test (da-d4t.1).
//!
//! Drives the CLI dispatch path with the actual TOML-on-disk so the
//! parser, color resolution, install conflict handling, and export
//! serializer all run together.

use std::fs;

use datum_commands::cmd::dispatch_argv;
use datum_core::test_util::fresh_db;

#[test]
fn install_then_export_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("core.toml");
    let out_path = dir.path().join("export.toml");

    fs::write(
        &in_path,
        r##"[kit]
name = "core"
version = "0.1"

[[field]]
name = "sleep"
default_verb = "start"
pos = 0.10
size = 0.30
color = "#1a3d6e"

[[field]]
name = "mood"
default_verb = "switch"
color = "blue"

[[field.states]]
name = "happy"
color = "#00ff00"

[[field.states]]
name = "sad"
"##,
    )
    .unwrap();

    let db = fresh_db();

    // Install.
    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "install".into(),
            in_path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();

    // Two fields installed under kit_id "core".
    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].kit_id, "core");
    assert_eq!(kits[0].field_count, 2);

    // Export.
    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "export".into(),
            "core".into(),
            out_path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();

    let exported = fs::read_to_string(&out_path).unwrap();
    // Re-parse the exported TOML and confirm the structural invariants
    // round-trip cleanly. (We don't compare strings byte-for-byte because
    // toml-rs may re-order tables; the structural diff is what matters.)
    let kit = datum_core::kit::KitToml::from_str(&exported).unwrap();
    assert_eq!(kit.kit.name, "core");
    assert_eq!(kit.fields.len(), 2);

    let sleep = kit.fields.iter().find(|f| f.name == "sleep").unwrap();
    assert_eq!(sleep.default_verb.as_deref(), Some("start"));
    assert_eq!(sleep.pos, Some(0.10));
    assert_eq!(sleep.size, Some(0.30));
    assert_eq!(sleep.color.as_deref(), Some("#1a3d6e"));

    let mood = kit.fields.iter().find(|f| f.name == "mood").unwrap();
    assert_eq!(mood.default_verb.as_deref(), Some("switch"));
    // "blue" was resolved to a hex at install time and stored that way.
    assert!(mood.color.as_ref().unwrap().starts_with('#'));
    assert_eq!(mood.states.len(), 2);
    let happy = mood.states.iter().find(|s| s.name == "happy").unwrap();
    assert_eq!(happy.color.as_deref(), Some("#00ff00"));
}

#[test]
fn install_then_uninstall_clears_owned_rows() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("k.toml");
    fs::write(
        &in_path,
        r##"[kit]
name = "test-kit"

[[field]]
name = "f1"
default_verb = "occur"

[[field]]
name = "f2"
default_verb = "start"
"##,
    )
    .unwrap();
    let db = fresh_db();
    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "install".into(),
            in_path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();
    assert!(db.load_spec("f1").unwrap().is_some());
    assert!(db.load_spec("f2").unwrap().is_some());

    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "uninstall".into(),
            "test-kit".into(),
        ],
        &db,
    )
    .unwrap();

    assert!(db.load_spec("f1").unwrap().is_none());
    assert!(db.load_spec("f2").unwrap().is_none());
    assert!(db.list_kits().unwrap().is_empty());
}

#[test]
fn install_skips_existing_user_field_without_force() {
    use datum_core::entry::Verb;
    use datum_core::field_spec::FieldSpec;

    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("k.toml");
    fs::write(
        &in_path,
        r##"[kit]
name = "intruder"

[[field]]
name = "sleep"
default_verb = "start"
color = "#000000"
"##,
    )
    .unwrap();

    let db = fresh_db();

    // Pre-existing user-declared row.
    let mut existing = FieldSpec::new("sleep", Verb::Start);
    existing.color = Some("#aaaaaa".into());
    db.save_field_spec(&existing).unwrap();

    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "install".into(),
            in_path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();

    let loaded = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(loaded.color.as_deref(), Some("#aaaaaa"));
    assert!(db.list_kits().unwrap().is_empty());

    // Re-run with --force overwrites and tags the row.
    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "install".into(),
            "--force".into(),
            in_path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();

    let loaded = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(loaded.color.as_deref(), Some("#000000"));
    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].kit_id, "intruder");
}
