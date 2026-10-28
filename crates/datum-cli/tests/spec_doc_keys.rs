//! A spec document's keys are the keys a spec carries (da-las9r): one
//! outside them warns, naming it and the field, and is kept; an old symbol
//! key (da-yc5vi) is read as its new name and warns as deprecated; a known
//! key never warns.

use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

/// Stdout and stderr of a line that must succeed.
fn ok(dir: &TempDir, args: &[&str]) -> (String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("s.db"))
        .env("HOME", dir.path())
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (text(out.stdout), text(out.stderr))
}

fn spec(dir: &TempDir, field: &str) -> Value {
    serde_json::from_str(&ok(dir, &["setup", field, "--json"]).0).unwrap()
}

#[test]
fn a_key_no_spec_has_warns_on_setup_and_survives_a_read_and_write() {
    let dir = tempfile::tempdir().unwrap();
    let json = r##"{"verb": "occur", "symbl": "x", "glyph": "●", "color": "#123456"}"##;
    let (_, err) = ok(&dir, &["setup", "coffee", "--from-json", json]);
    assert_eq!(
        err,
        "⚠ spec 'coffee': `glyph` is deprecated, read as `symbol`\n\
         ⚠ spec 'coffee': `symbl` is no key a spec has, kept as written\n"
    );
    let unknown = "⚠ spec 'coffee': `symbl` is no key a spec has, kept as written\n";
    for show in [&["setup", "coffee"][..], &["setup", "coffee", "--json"]] {
        assert_eq!(ok(&dir, show).1, unknown, "{show:?}");
    }
    assert_eq!(
        ok(&dir, &["setup", "coffee", "--description", "hot"]).1,
        unknown
    );
    let data = spec(&dir, "coffee");
    assert_eq!(
        (&data["symbl"], &data["symbol"]),
        (&"x".into(), &"●".into())
    );
    assert!(data.get("glyph").is_none(), "{data}");
    assert_eq!(data["description"], "hot");

    // Every key the spec just shown carries is known: as a spec of its
    // own, none of them warns.
    let mut shown = spec(&dir, "coffee");
    shown.as_object_mut().unwrap().remove("symbl");
    let (_, err) = ok(&dir, &["setup", "tea", "--from-json", &shown.to_string()]);
    assert_eq!(err, "");
    assert_eq!(ok(&dir, &["setup", "tea"]).1, "");
}

/// The kit field that sets every key a kit can warns only of the old
/// `glyph_by` it spells; spelled `symbol_by`, neither the install nor the
/// spec it stores says anything.
#[test]
fn a_kit_warns_of_an_old_symbol_key_and_of_no_known_one() {
    let dir = tempfile::tempdir().unwrap();
    let kit = crate::kit_install::whole_field_kit()
        .replace("states.calm = {}", "states.calm = { glyph = \"c\" }");
    assert!(kit.contains("glyph_by = "), "{kit}");
    let path = dir.path().join("k.toml");
    std::fs::write(&path, &kit).unwrap();
    let (_, err) = ok(&dir, &["kit", "install", &path.to_string_lossy()]);
    assert_eq!(
        err,
        "⚠ kit field 'mood': `glyph_by` is deprecated, read as `symbol_by`\n\
         ⚠ kit field 'mood' state 'calm': `glyph` is deprecated, read as `symbol`\n"
    );

    let dir = tempfile::tempdir().unwrap();
    let kit = crate::kit_install::whole_field_kit().replace("glyph_by = ", "symbol_by = ");
    let path = dir.path().join("k.toml");
    std::fs::write(&path, &kit).unwrap();
    assert_eq!(ok(&dir, &["kit", "install", &path.to_string_lossy()]).1, "");
    assert_eq!(ok(&dir, &["setup", "mood"]).1, "");
}
