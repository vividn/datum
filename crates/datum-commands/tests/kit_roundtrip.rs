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

states.happy = { color = "#00ff00" }
states.sad = {}
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
    let happy = mood.states.get("happy").unwrap();
    assert_eq!(happy.color.as_deref(), Some("#00ff00"));
    // "sad" carries no overrides — it round-trips as an empty inline table.
    assert!(mood.states.contains_key("sad"));
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
fn install_anchored_field_round_trips_relative_to() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("anchored.toml");
    let out_path = dir.path().join("export.toml");
    fs::write(
        &in_path,
        r##"[kit]
name = "anchored"

[[field]]
name = "body"
default_verb = "occur"
pos = 0.20
size = 0.40

[[field]]
name = "contacts"
default_verb = "occur"
relative_to = "body"
pos = 0.10
size = 0.05
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

    let contacts = db.load_spec("contacts").unwrap().unwrap();
    assert_eq!(contacts.relative_to.as_deref(), Some("body"));
    assert_eq!(contacts.anchor_pos_ratio, Some(0.10));
    assert_eq!(contacts.anchor_size_ratio, Some(0.05));
    assert!(contacts.y.is_none());
    assert!(contacts.height.is_none());

    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "export".into(),
            "anchored".into(),
            out_path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();
    let exported = fs::read_to_string(&out_path).unwrap();
    let kit = datum_core::kit::KitToml::from_str(&exported).unwrap();
    let contacts = kit.fields.iter().find(|f| f.name == "contacts").unwrap();
    assert_eq!(contacts.relative_to.as_deref(), Some("body"));
    assert_eq!(contacts.pos, Some(0.10));
    assert_eq!(contacts.size, Some(0.05));
}

#[test]
fn install_rejects_anchor_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("cycle.toml");
    fs::write(
        &in_path,
        r##"[kit]
name = "cyclic"

[[field]]
name = "a"
default_verb = "occur"
relative_to = "b"
pos = 0.1
size = 0.5

[[field]]
name = "b"
default_verb = "occur"
relative_to = "c"
pos = 0.1
size = 0.5

[[field]]
name = "c"
default_verb = "occur"
relative_to = "a"
pos = 0.1
size = 0.5
"##,
    )
    .unwrap();
    let db = fresh_db();
    let err = dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "install".into(),
            in_path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("Cycle detected"),
        "expected cycle error, got: {msg}"
    );
    // Nothing should have been installed.
    assert!(db.list_kits().unwrap().is_empty());
}

#[test]
fn install_warns_on_missing_anchor_but_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("missing.toml");
    fs::write(
        &in_path,
        r##"[kit]
name = "missing-anchor"

[[field]]
name = "contacts"
default_verb = "occur"
relative_to = "body"
pos = 0.10
size = 0.05
"##,
    )
    .unwrap();
    let db = fresh_db();
    // Install must succeed despite the dangling anchor — slice 2 specifies
    // missing anchors as a warning, not an error.
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
    assert!(db.load_spec("contacts").unwrap().is_some());
}

#[test]
fn install_kit_with_aliases_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("chores.toml");
    let out_path = dir.path().join("export.toml");

    fs::write(
        &in_path,
        r##"[kit]
name = "chores"

[[field]]
name = "dishes"
default_verb = "occur"

[[alias]]
name = "chore"
expands_to = "occur -k next_date"

[[alias]]
name = "done"
expands_to = "occur"
description = "mark a chore done"
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

    // Aliases are present in the live aliases table tagged with the kit's id.
    let chore = db.lookup_alias("chore").unwrap().unwrap();
    assert_eq!(chore.expands_to, "occur -k next_date");
    let done = db.lookup_alias("done").unwrap().unwrap();
    assert_eq!(done.description.as_deref(), Some("mark a chore done"));

    // list_kits reports the alias count.
    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].kit_id, "chores");
    assert_eq!(kits[0].field_count, 1);
    assert_eq!(kits[0].alias_count, 2);

    // Export round-trips both the field and the aliases.
    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "export".into(),
            "chores".into(),
            out_path.to_string_lossy().into(),
        ],
        &db,
    )
    .unwrap();

    let exported = fs::read_to_string(&out_path).unwrap();
    let kit = datum_core::kit::KitToml::from_str(&exported).unwrap();
    assert_eq!(kit.kit.name, "chores");
    assert_eq!(kit.fields.len(), 1);
    assert_eq!(kit.aliases.len(), 2);

    // Expansion text is preserved byte-for-byte (the "round-trip" invariant
    // for aliases per the slice-3 bead).
    let chore_e = kit.aliases.iter().find(|a| a.name == "chore").unwrap();
    assert_eq!(chore_e.expands_to, "occur -k next_date");
    let done_e = kit.aliases.iter().find(|a| a.name == "done").unwrap();
    assert_eq!(done_e.description.as_deref(), Some("mark a chore done"));

    // Uninstall removes the kit's aliases without touching unrelated rows.
    dispatch_argv(
        vec![
            "datum".into(),
            "kit".into(),
            "uninstall".into(),
            "chores".into(),
        ],
        &db,
    )
    .unwrap();
    assert!(db.lookup_alias("chore").unwrap().is_none());
    assert!(db.lookup_alias("done").unwrap().is_none());
}

#[test]
fn install_kit_alias_skips_user_alias_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let in_path = dir.path().join("conflict.toml");
    fs::write(
        &in_path,
        r##"[kit]
name = "intruder"

[[alias]]
name = "vbp"
expands_to = "kit_expansion"
"##,
    )
    .unwrap();

    let db = fresh_db();

    // Pre-existing user-declared alias.
    db.save_alias(&datum_core::field_spec::Alias {
        name: "vbp".into(),
        expands_to: "user_expansion".into(),
        description: None,
    })
    .unwrap();

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

    let loaded = db.lookup_alias("vbp").unwrap().unwrap();
    assert_eq!(
        loaded.expands_to, "user_expansion",
        "user alias must win without --force"
    );
    // The kit installed nothing — list_kits should be empty.
    assert!(db.list_kits().unwrap().is_empty());

    // --force re-tags the row with the kit's id.
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
    let loaded = db.lookup_alias("vbp").unwrap().unwrap();
    assert_eq!(loaded.expands_to, "kit_expansion");
    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].alias_count, 1);
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
