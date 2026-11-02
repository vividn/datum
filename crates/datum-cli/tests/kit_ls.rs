//! `datum kit ls` is `datum kit list` (da-fsn0e; vividn, 2026-11-02: «`datum
//! kit ls` should be aliased to `datum kit list`»): the parser's word, not a
//! user alias, so help, completion and `help --json` all know it.

use std::process::Command;

use tempfile::TempDir;

fn ok(dir: &TempDir, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("k.db"))
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn kit_ls_is_kit_list_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let kit = dir.path().join("kit.toml");
    let toml =
        "[kit]\nname = \"xa\"\n\n[[alias]]\nname = \"spent\"\nexpands_to = \"occur money\"\n";
    std::fs::write(&kit, toml).unwrap();
    ok(&dir, &["kit", "install", &kit.to_string_lossy()]);

    let listed = ok(&dir, &["kit", "list"]);
    assert!(listed.contains("xa"), "{listed}");
    assert_eq!(ok(&dir, &["kit", "ls"]), listed);

    assert!(ok(&dir, &["help", "kit"]).contains("datum kit list (ls)"));
    assert_eq!(
        ok(&dir, &["kit", "ls", "--help"]),
        ok(&dir, &["kit", "list", "--help"])
    );

    let json: serde_json::Value = serde_json::from_str(&ok(&dir, &["help", "--json"])).unwrap();
    let kit = json.as_array().unwrap().iter().find(|c| c["name"] == "kit");
    let list = kit.unwrap()["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "list")
        .unwrap();
    assert_eq!(list["aliases"], serde_json::json!(["ls"]));

    // As a command's alias: offered once a word is begun, not on a bare TAB.
    let complete = |line: &[&str]| {
        ok(
            &dir,
            &[&["__complete", "2", "--", "datum"][..], line].concat(),
        )
    };
    assert!(complete(&["kit", "l"]).lines().any(|w| w == "ls"));
    assert!(!complete(&["kit", ""]).lines().any(|w| w == "ls"));
    assert!(complete(&["kit", ""]).lines().any(|w| w == "list"));
}
