//! `spec` has no subcommands (da-ez40f): `spec` lists, `spec F` prints,
//! `spec F <changes>` changes, `spec F --delete` deletes, and the old
//! subcommands are refused with the line that does it now.

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
            "spec",
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
    ok(&dir, &["spec", "coffee", "--verb", "occur"]);
    dir
}

#[test]
fn spec_alone_lists_the_specs_and_tag_filters_them() {
    let dir = mood();
    assert_eq!(lines(&ok(&dir, &["spec"])), ["coffee", "mood"]);
    assert_eq!(lines(&ok(&dir, &["spec", "--tag", "mind"])), ["mood"]);
    let err = refused(&dir, &["spec", "--tag", "mind", "--tag", "drink"]);
    assert!(err.contains("`spec` lists by one --tag"), "{err}");
}

#[test]
fn spec_field_prints_its_block_and_json_its_document() {
    let dir = mood();
    let block = lines(&ok(&dir, &["spec", "mood"]));
    assert_eq!(block[0], "mood switch #888888 tracked");
    assert!(
        block.contains(&"states happy · sad".to_string()),
        "{block:?}"
    );
    assert!(block.contains(&"tags mind".to_string()), "{block:?}");
    let json: serde_json::Value =
        serde_json::from_str(&ok(&dir, &["spec", "mood", "--json"])).unwrap();
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
            "spec",
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

    let echo = ok(&dir, &["spec", "mood", "--glyph", "▲", "--json"]);
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
    ok(&dir, &["spec", "mood", "--color", "#b87333"]);
    assert_eq!(
        ok(&dir, &["spec", "mood", "--color", "#b87333"]),
        "no change\n"
    );
    ok(&dir, &["undo"]);
    let json: serde_json::Value =
        serde_json::from_str(&ok(&dir, &["spec", "mood", "--json"])).unwrap();
    assert_eq!(json["color"], "#888888");
}

#[test]
fn delete_deletes_the_spec_and_a_second_says_there_is_none() {
    let dir = mood();
    assert!(ok(&dir, &["spec", "mood", "--delete"]).starts_with("DELETE "));
    assert_eq!(lines(&ok(&dir, &["spec"])), ["coffee"]);
    let again = datum(&dir, &["spec", "mood", "--delete"]);
    assert!(again.status.success());
    let err = String::from_utf8(again.stderr).unwrap();
    assert!(err.contains("'mood' has no spec: nothing deleted"), "{err}");
    assert!(
        refused(&dir, &["spec", "mood", "--delete", "--color", "red"])
            .contains("--delete takes the field alone")
    );
}

#[test]
fn a_new_field_needs_a_verb() {
    let dir = mood();
    let err = refused(&dir, &["spec", "tea", "--color", "red"]);
    assert!(err.contains("--verb"), "{err}");
    assert!(ok(&dir, &["spec", "tea", "--verb", "occur"]).starts_with("CREATE "));
}

/// No aliases: a field may be called `set`, so only the second positional
/// no `spec` line takes makes one the old form, refused with the new line.
#[test]
fn the_old_subcommands_are_refused_with_the_line_that_does_it_now() {
    let dir = mood();
    for (old, now) in [
        (
            &["spec", "set", "mood", "--color", "red"][..],
            "`spec set` is gone: `spec mood --color red`",
        ),
        (
            &["spec", "show", "mood"],
            "`spec show` is gone: `spec mood`",
        ),
        (
            &["spec", "remove", "mood"],
            "`spec remove` is gone: `spec mood --delete`",
        ),
        (
            &["spec", "add-state", "mood", "meh", "-", "so-so"],
            "`spec add-state` is gone: `spec mood --add-state meh --state-description meh=so-so`",
        ),
        (
            &["spec", "add-state", "mood"],
            "`spec add-state` is gone: `spec mood --add-state NAME`",
        ),
    ] {
        let err = refused(&dir, old);
        assert!(err.contains(now), "{old:?}: {err}");
    }
    // A field called `set` is a field.
    ok(&dir, &["spec", "set", "--verb", "occur"]);
    assert_eq!(lines(&ok(&dir, &["spec", "set"]))[0], "set occur tracked");
    // `spec list` is the field `list`, which most likely means the listing.
    assert!(ok(&dir, &["spec", "list"]).contains("`spec` alone lists the specs"));
}
