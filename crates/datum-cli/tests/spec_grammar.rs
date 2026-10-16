//! `setup` has no subcommands (da-ez40f): `setup` lists, `setup F` prints,
//! `setup F <changes>` changes, `setup F --delete` deletes. It was `spec`
//! until da-a4ptr, and `spec` is now its alias (da-3ie71).

use std::process::{Command, Output};

use tempfile::TempDir;

fn datum(dir: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("s.db"))
        .env("HOME", dir.path())
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .unwrap()
}

fn ok(dir: &TempDir, args: &[&str]) -> String {
    let out = datum(dir, args);
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

fn refused(dir: &TempDir, args: &[&str]) -> String {
    let out = datum(dir, args);
    assert!(!out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stderr).unwrap()
}

/// Each line, its runs of spaces made one, so a test reads the words.
fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

fn mood() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    ok(
        &dir,
        &[
            "setup",
            "mood",
            "--verb",
            "switch",
            "--states",
            "happy,sad",
            "--color",
            "#888888",
            "--add-tag",
            "mind",
            "--glyph",
            "●",
        ],
    );
    ok(&dir, &["setup", "coffee", "--verb", "occur"]);
    dir
}

#[test]
fn spec_alone_lists_the_specs_and_tag_filters_them() {
    let dir = mood();
    assert_eq!(lines(&ok(&dir, &["setup"])), ["coffee", "mood"]);
    assert_eq!(lines(&ok(&dir, &["setup", "--tag", "mind"])), ["mood"]);
    let err = refused(&dir, &["setup", "--tag", "mind", "--tag", "drink"]);
    assert!(err.contains("`setup` lists by one --tag"), "{err}");
}

#[test]
fn spec_field_prints_its_block_and_json_its_document() {
    let dir = mood();
    let block = lines(&ok(&dir, &["setup", "mood"]));
    assert_eq!(block[0], "mood switch #888888 tracked");
    assert!(
        block.contains(&"states happy · sad".to_string()),
        "{block:?}"
    );
    assert!(block.contains(&"tags mind".to_string()), "{block:?}");
    let json: serde_json::Value =
        serde_json::from_str(&ok(&dir, &["setup", "mood", "--json"])).unwrap();
    assert_eq!(json["allowed_states"], serde_json::json!(["happy", "sad"]));
}

/// A change prints the whole new spec under its action, each changed line
/// marked; `--show json` echoes the spec entry, which is what the web's
/// bar shows.
#[test]
fn a_change_prints_the_spec_with_what_changed_marked() {
    let dir = mood();
    let out = ok(
        &dir,
        &[
            "setup",
            "mood",
            "--color",
            "#b87333",
            "--states",
            "happy,calm",
            "--glyph",
            "",
        ],
    );
    let out = lines(&out);
    assert!(
        out[0].starts_with("UPDATE ") && out[0].ends_with(" spec:mood"),
        "{out:?}"
    );
    assert_eq!(out[1], "mood switch #b87333 tracked");
    for line in [
        "~ color #888888 → #b87333",
        "~ states happy · sad → happy · calm",
        "− glyph ●",
        "tags mind",
    ] {
        assert!(out.contains(&line.to_string()), "{line}: {out:?}");
    }

    let echo = ok(&dir, &["setup", "mood", "--glyph", "▲", "--json"]);
    assert!(!echo.contains("\n~"), "the echo alone: {echo}");
    let echo: serde_json::Value = serde_json::from_str(&echo).unwrap();
    assert_eq!(echo["docs"][0]["action"], "UPDATE");
    assert_eq!(echo["docs"][0]["id"], "spec:mood");
}

/// No change writes nothing, so the change before it is what `undo` takes
/// back.
#[test]
fn no_change_writes_nothing_and_leaves_no_undo_step() {
    let dir = mood();
    ok(&dir, &["setup", "mood", "--color", "#b87333"]);
    assert_eq!(
        ok(&dir, &["setup", "mood", "--color", "#b87333"]),
        "no change\n"
    );
    ok(&dir, &["undo"]);
    let json: serde_json::Value =
        serde_json::from_str(&ok(&dir, &["setup", "mood", "--json"])).unwrap();
    assert_eq!(json["color"], "#888888");
}

#[test]
fn delete_deletes_the_spec_and_a_second_says_there_is_none() {
    let dir = mood();
    assert!(ok(&dir, &["setup", "mood", "--delete"]).starts_with("DELETE "));
    assert_eq!(lines(&ok(&dir, &["setup"])), ["coffee"]);
    let again = datum(&dir, &["setup", "mood", "--delete"]);
    assert!(again.status.success());
    let err = String::from_utf8(again.stderr).unwrap();
    assert!(err.contains("'mood' has no spec: nothing deleted"), "{err}");
    assert!(
        refused(&dir, &["setup", "mood", "--delete", "--color", "red"])
            .contains("--delete takes the field alone")
    );
}

#[test]
fn a_new_field_needs_a_verb() {
    let dir = mood();
    let err = refused(&dir, &["setup", "tea", "--color", "red"]);
    assert!(err.contains("--verb"), "{err}");
    assert!(ok(&dir, &["setup", "tea", "--verb", "occur"]).starts_with("CREATE "));
}

/// `spec` is `setup`'s alias (da-3ie71), as `rt` is `retime`'s: every form
/// says and does what `setup`'s does, refusals included, and its help is
/// `setup`'s page.
#[test]
fn spec_is_setups_alias() {
    // Two databases mint two hids: the word after an action tag is masked.
    let text = |bytes: Vec<u8>| -> Vec<String> {
        let text = String::from_utf8(bytes).unwrap();
        text.lines()
            .map(|l| match l.split_once(' ') {
                Some((tag, rest)) if tag.chars().all(|c| c.is_ascii_uppercase()) => {
                    format!(
                        "{tag} <hid>{}",
                        &rest[rest.find(' ').unwrap_or(rest.len())..]
                    )
                }
                _ => l.to_string(),
            })
            .collect()
    };
    let same = |argv: &[&str]| {
        let (a, b) = (mood(), mood());
        let spec = datum(&a, &[&["spec"], argv].concat());
        let setup = datum(&b, &[&["setup"], argv].concat());
        assert_eq!(spec.status.code(), setup.status.code(), "{argv:?}");
        assert_eq!(text(spec.stdout), text(setup.stdout), "{argv:?}");
        assert_eq!(text(spec.stderr), text(setup.stderr), "{argv:?}");
        assert_eq!(
            ok(&a, &["setup", "mood", "--json"]),
            ok(&b, &["setup", "mood", "--json"]),
            "{argv:?}"
        );
    };
    same(&[]);
    same(&["mood"]);
    same(&["mood", "--json"]);
    same(&["mood", "--add-state", "meh"]);
    same(&["mood", "--delete"]);
    same(&["tea", "--color", "red"]);
    same(&["mood", "--show", "none"]);
    same(&["--tag", "mind", "--tag", "drink"]);

    let dir = mood();
    let help = ok(&dir, &["help", "setup"]);
    assert!(
        help.starts_with("datum setup — ") && help.contains("(alias: spec)"),
        "{help}"
    );
    assert_eq!(ok(&dir, &["help", "spec"]), help);
    assert_eq!(ok(&dir, &["spec", "--help"]), help);
    // The field `spec` stays the specs' own.
    assert!(refused(&dir, &["occur", "spec"]).contains("reserved for field specs"));
}
