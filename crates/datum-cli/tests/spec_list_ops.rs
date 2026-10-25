//! `setup`'s lists take three operations each (da-f9bky): `--add-state`,
//! `--remove-state`, `--states`, and the same for tags; keys take `-k`/`-K`
//! as `track` does, and `--remove-key`. Adding what is there and removing
//! what is not write nothing, the second saying so.

use std::process::{Command, Output};

use serde_json::{json, Value};
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

/// Stdout and stderr of a line that must succeed.
fn ok(dir: &TempDir, args: &[&str]) -> (String, String) {
    let out = datum(dir, args);
    assert!(out.status.success(), "{args:?}: {out:?}");
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (text(out.stdout), text(out.stderr))
}

fn spec(dir: &TempDir, field: &str) -> Value {
    serde_json::from_str(&ok(dir, &["setup", field, "--json"]).0).unwrap()
}

/// mood: happy, sad and calm, sad carrying one of everything a state can.
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
            "happy, sad,calm",
            "--state-color",
            "happy=#00ff00",
            "--state-color",
            "sad=#0000ff",
            "--state-glyph",
            "sad=▼",
            "--state-description",
            "sad=low",
            "--state-key",
            "sad=why",
            "--state-assign",
            "sad=kind=blue",
        ],
    );
    dir
}

#[test]
fn add_state_appends_once() {
    let dir = mood();
    ok(&dir, &["setup", "mood", "--add-state", "meh"]);
    assert_eq!(
        spec(&dir, "mood")["allowed_states"],
        json!(["happy", "sad", "calm", "meh"])
    );
    let (out, _) = ok(&dir, &["setup", "mood", "--add-state", "meh"]);
    assert_eq!(out, "no change\n");
}

#[test]
fn remove_state_drops_what_it_carries() {
    let dir = mood();
    ok(&dir, &["setup", "mood", "--remove-state", "sad"]);
    let s = spec(&dir, "mood");
    assert_eq!(s["allowed_states"], json!(["happy", "calm"]));
    for key in [
        "state_colors",
        "state_descriptions",
        "state_keys",
        "state_assign",
    ] {
        assert!(s[key].get("sad").is_none(), "{key}: {s}");
    }
    assert!(
        s["presentation"]["state_glyphs"].get("sad").is_none(),
        "{s}"
    );
    assert_eq!(s["state_colors"]["happy"], "#00ff00");
}

/// Removing what is not there warns and writes nothing: the change before
/// it is still what `undo` takes back.
#[test]
fn removing_what_is_absent_warns_and_writes_nothing() {
    let dir = mood();
    ok(&dir, &["setup", "mood", "--color", "#b87333"]);
    for (flag, what) in [
        ("--remove-state", "state"),
        ("--remove-tag", "tag"),
        ("--remove-key", "key"),
    ] {
        let (out, err) = ok(&dir, &["setup", "mood", flag, "nope"]);
        assert_eq!(out, "no change\n", "{flag}");
        assert!(
            err.contains(&format!("'mood' has no {what} 'nope': nothing removed")),
            "{flag}: {err}"
        );
    }
    ok(&dir, &["undo"]);
    assert!(spec(&dir, "mood")
        .get("color")
        .is_none_or(|c| c != "#b87333"));
}

/// A state `--states` drops and `--remove-state` names is removed once,
/// with no warning that it was not there.
#[test]
fn a_state_dropped_twice_over_is_not_absent() {
    let dir = mood();
    let (_, err) = ok(
        &dir,
        &[
            "setup",
            "mood",
            "--states",
            "happy,calm",
            "--remove-state",
            "sad",
        ],
    );
    assert!(!err.contains("nothing removed"), "{err}");
    let s = spec(&dir, "mood");
    assert_eq!(s["allowed_states"], json!(["happy", "calm"]));
    assert!(s["state_colors"].get("sad").is_none(), "{s}");
}

#[test]
fn states_sets_the_order_and_keeps_what_kept_states_carry() {
    let dir = mood();
    ok(&dir, &["setup", "mood", "--states", "calm,happy"]);
    let s = spec(&dir, "mood");
    assert_eq!(s["allowed_states"], json!(["calm", "happy"]));
    assert_eq!(s["state_colors"], json!({"happy": "#00ff00"}));
    ok(&dir, &["setup", "mood", "--states", ""]);
    assert!(spec(&dir, "mood")
        .get("allowed_states")
        .is_none_or(Value::is_null));
}

#[test]
fn tags_take_the_same_three_operations() {
    let dir = mood();
    ok(
        &dir,
        &["setup", "mood", "--add-tag", "mind", "--add-tag", "body"],
    );
    assert_eq!(
        ok(&dir, &["setup", "mood", "--add-tag", "mind"]).0,
        "no change\n"
    );
    assert_eq!(spec(&dir, "mood")["tags"], json!(["mind", "body"]));
    ok(&dir, &["setup", "mood", "--remove-tag", "mind"]);
    assert_eq!(spec(&dir, "mood")["tags"], json!(["body"]));
    ok(&dir, &["setup", "mood", "--tags", "b, a"]);
    assert_eq!(spec(&dir, "mood")["tags"], json!(["b", "a"]));
    assert_eq!(ok(&dir, &["setup", "--tag", "a"]).0.trim(), "mood");
}

/// The flags that replaced a list are refused, naming the three that do it.
#[test]
fn state_and_a_fields_tag_are_gone() {
    let dir = mood();
    for (args, now) in [
        (
            &["setup", "mood", "--state", "x"][..],
            "--add-state S, --remove-state S, or --states a,b,c",
        ),
        (
            &["setup", "mood", "--tag", "x"],
            "--add-tag T, --remove-tag T, or --tags a,b",
        ),
    ] {
        let out = datum(&dir, args);
        let err = String::from_utf8(out.stderr).unwrap();
        assert!(
            !out.status.success() && err.contains(now),
            "{args:?}: {err}"
        );
    }
}

/// `-k`/`-K` add a slot as `track`'s do, a key already there taking its new
/// spelling in its place; `--remove-key` takes one away, a subfield too.
#[test]
fn keys_add_in_place_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    ok(
        &dir,
        &[
            "setup",
            "juice",
            "--verb",
            "occur",
            "-k",
            "cups",
            "-k",
            "temp=cold",
        ],
    );
    ok(
        &dir,
        &["setup", "juice", "-k", "size=", "-k", "cups=1", "-K", "day"],
    );
    let s = spec(&dir, "juice");
    assert_eq!(s["data_keys"], json!(["cups=1", "temp=cold", "size="]));
    assert_eq!(s["id_keys"], json!(["day"]));
    // brand is a subfield alone (a kit's, or `--from-json`'s): removed, with
    // no warning.
    let mut s = spec(&dir, "juice");
    s["subfields"]["brand"] = json!({"value_type": "any"});
    ok(&dir, &["setup", "juice", "--from-json", &s.to_string()]);
    let (_, err) = ok(
        &dir,
        &[
            "setup",
            "juice",
            "--remove-key",
            "temp",
            "--remove-key",
            "brand",
        ],
    );
    assert!(!err.contains("nothing removed"), "{err}");
    let s = spec(&dir, "juice");
    assert_eq!(s["data_keys"], json!(["cups=1", "size="]));
    assert!(
        s.get("subfields").is_none_or(|f| f.get("brand").is_none()),
        "{s}"
    );
}

/// `-R`, `-S` and `-C` say where the leftover words go, as on `track`.
#[test]
fn remainder_short_flags() {
    let dir = tempfile::tempdir().unwrap();
    ok(&dir, &["setup", "note", "--verb", "occur", "-SR", "text"]);
    let s = spec(&dir, "note");
    assert_eq!(
        (&s["remainder"], &s["string_remainder"]),
        (&json!("text"), &json!(true))
    );
    ok(&dir, &["setup", "note", "-C"]);
    let s = spec(&dir, "note");
    assert!(s.get("remainder").is_none_or(Value::is_null), "{s}");
    assert_eq!(s["comment_remainder"], json!(true));
    assert!(!datum(&dir, &["setup", "note", "-S"]).status.success());
}

/// `--archive-state S` (da-tdpjf) takes S out of the list and keeps what it
/// carries, so its past rows draw as they did, and `setup F` says it is
/// archived.
#[test]
fn archive_state_keeps_what_it_carries() {
    let dir = mood();
    ok(&dir, &["switch", "mood", "sad", "why=x", "-t", "-5"]);
    let tail = |dir: &TempDir| {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", dir.path().join("s.db"))
            .env("HOME", dir.path())
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .env("FORCE_COLOR", "1")
            .args(["tail", "mood"])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let before = tail(&dir);
    assert!(before.contains("48;2;0;0;255"), "sad's blue: {before}");
    let before_spec = spec(&dir, "mood");
    let (out, _) = ok(&dir, &["setup", "mood", "--archive-state", "sad"]);
    assert!(
        out.lines()
            .any(|l| l.starts_with("+ archived") && l.ends_with(" sad")),
        "{out}"
    );
    let s = spec(&dir, "mood");
    assert_eq!(s["allowed_states"], json!(["happy", "calm"]));
    for key in [
        "state_colors",
        "state_descriptions",
        "state_keys",
        "state_assign",
        "state_glyphs",
    ] {
        assert_eq!(s[key], before_spec[key], "{key}");
    }
    assert_eq!(s["state_glyphs"]["sad"], "▼", "{s}");
    assert_eq!(tail(&dir), before);
    assert_eq!(
        ok(&dir, &["setup", "mood", "--archive-state", "sad"]).0,
        "no change\n"
    );
    let (_, err) = ok(&dir, &["setup", "mood", "--archive-state", "nope"]);
    assert!(
        err.contains("'mood' has no state 'nope': nothing archived"),
        "{err}"
    );
}

/// Under `--only-listed-states` an archived state is refused as any
/// unlisted one is.
#[test]
fn an_archived_state_is_not_listed() {
    let dir = mood();
    ok(&dir, &["setup", "mood", "--only-listed-states"]);
    ok(&dir, &["switch", "mood", "sad", "why=x"]);
    ok(&dir, &["setup", "mood", "--archive-state", "sad"]);
    let out = datum(&dir, &["switch", "mood", "sad", "why=x"]);
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(!out.status.success() && err.contains("sad"), "{err}");
}

/// `--add-state` brings an archived state back with what it carries, and
/// `--remove-state` drops it and all of that.
#[test]
fn an_archived_state_comes_back_or_goes() {
    let dir = mood();
    ok(&dir, &["setup", "mood", "--archive-state", "sad"]);
    let (out, _) = ok(&dir, &["setup", "mood", "--add-state", "sad"]);
    assert!(out.lines().any(|l| l.starts_with("− archived")), "{out}");
    let s = spec(&dir, "mood");
    assert_eq!(s["allowed_states"], json!(["happy", "calm", "sad"]));
    assert_eq!(s["state_descriptions"]["sad"], "low");
    ok(&dir, &["setup", "mood", "--archive-state", "sad"]);
    let (_, err) = ok(&dir, &["setup", "mood", "--remove-state", "sad"]);
    assert!(!err.contains("nothing removed"), "{err}");
    let s = spec(&dir, "mood");
    assert!(s["state_colors"].get("sad").is_none(), "{s}");
    assert!(s["state_descriptions"].get("sad").is_none(), "{s}");
    assert!(!ok(&dir, &["setup", "mood"]).0.contains("archived"));
}

/// A field that lists no states may be in any, so archiving one of its
/// states is said to do nothing.
#[test]
fn archiving_without_a_list_says_nothing_is_archived() {
    let dir = tempfile::tempdir().unwrap();
    ok(
        &dir,
        &[
            "setup",
            "mood",
            "--verb",
            "switch",
            "--state-color",
            "sad=#0000ff",
        ],
    );
    let (out, err) = ok(&dir, &["setup", "mood", "--archive-state", "sad"]);
    assert_eq!(out, "no change\n");
    assert!(
        err.contains("'mood' lists no states, so it may be in any: nothing archived"),
        "{err}"
    );
}
