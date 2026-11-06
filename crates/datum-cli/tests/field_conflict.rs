//! da-wudrc: a log colliding with one of its field's entries resolves by the
//! field's conflict strategy, `setup F --conflict replace|update|merge|refuse`
//! (v2's useNew, preferNew, merge, fail), update when the spec names none.

use std::process::{Command, Output};

use serde_json::{json, Value};
use tempfile::TempDir;

fn run(dir: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("a.db"))
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path())
        .env("HOME", dir.path())
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn datum")
}

fn ok(dir: &TempDir, args: &[&str]) -> String {
    let out = run(dir, args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "datum {args:?} failed: {stderr}");
    String::from_utf8(out.stdout).unwrap()
}

/// The day's mood, logged with `words` after the state: its echo.
fn mood(dir: &TempDir, words: &[&str]) -> Value {
    let mut argv = vec!["change", "mood"];
    argv.extend(words);
    argv.extend(["-m", "-D", "-d", "2026-11-01", "--show", "json"]);
    let out: Value = serde_json::from_str(&ok(dir, &argv)).unwrap();
    out["docs"][0].clone()
}

/// Every live mood entry, as `tail` lists it: (state, data).
fn moods(dir: &TempDir) -> Vec<(Value, Value)> {
    let out: Value = serde_json::from_str(&ok(dir, &["tail", "mood", "--json"])).unwrap();
    let rows = out["rows"].as_array().unwrap();
    rows.iter()
        .map(|r| (r["state"].clone(), r["data"].clone()))
        .collect()
}

#[test]
fn with_no_strategy_a_collision_updates_and_a_repeat_is_nodiff() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(mood(&dir, &["good", "note=a"])["action"], "ALLDAY");
    let second = mood(&dir, &["bad"]);
    assert_eq!(second["action"], "UPDATE", "{second}");
    assert_eq!(moods(&dir), [(json!("bad"), json!({"note": "a"}))]);
    let again = mood(&dir, &["bad"]);
    assert_eq!(again["action"], "NODIFF", "{again}");
    assert_eq!(moods(&dir), [(json!("bad"), json!({"note": "a"}))]);
}

#[test]
fn replace_merge_and_refuse_resolve_as_set() {
    let dir = tempfile::tempdir().unwrap();
    ok(
        &dir,
        &["setup", "mood", "--kind", "change", "--conflict", "replace"],
    );
    assert!(ok(&dir, &["setup", "mood"]).contains("conflict  replace"));
    mood(&dir, &["good", "note=a"]);
    assert_eq!(mood(&dir, &["bad", "x=1"])["action"], "UPDATE");
    assert_eq!(moods(&dir), [(json!("bad"), json!({"x": 1}))]);

    ok(&dir, &["setup", "mood", "--conflict", "merge"]);
    mood(&dir, &["good", "x=2"]);
    assert_eq!(
        moods(&dir),
        [(json!(["bad", "good"]), json!({"x": [1, 2]}))]
    );

    ok(&dir, &["setup", "mood", "--conflict", "refuse"]);
    let argv = ["change", "mood", "sad", "-m", "-D", "-d", "2026-11-01"];
    let out = run(&dir, &argv);
    assert_eq!(out.status.code(), Some(11), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("already has an entry"), "{stderr}");
    assert_eq!(
        moods(&dir),
        [(json!(["bad", "good"]), json!({"x": [1, 2]}))]
    );

    // A line's own flag wins over the field's; `''` clears the field's.
    let mut over = argv.to_vec();
    over.push("--update");
    ok(&dir, &over);
    assert_eq!(moods(&dir)[0].0, json!("sad"));
    ok(&dir, &["setup", "mood", "--conflict", ""]);
    assert!(!ok(&dir, &["setup", "mood"]).contains("conflict"));
    assert_eq!(mood(&dir, &["calm"])["action"], "UPDATE");
}

/// da-c2nwv: a data key's own strategy, over the field's for that key alone.
#[test]
fn a_key_takes_its_own_strategy_over_the_fields() {
    let dir = tempfile::tempdir().unwrap();
    ok(&dir, &["setup", "mood", "--kind", "change"]);
    assert!(!ok(&dir, &["setup", "mood"]).contains("key conflict"));
    let bad = run(&dir, &["setup", "mood", "--key-conflict", "tag=bogus"]);
    assert!(!bad.status.success());
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("--key-conflict") && stderr.contains("tag=bogus"),
        "{stderr}"
    );
    ok(&dir, &["setup", "mood", "--key-conflict", "tag=merge"]);
    assert!(ok(&dir, &["setup", "mood"]).contains("key conflict  tag=merge"));

    // The field updates; its key `tag` merges.
    mood(&dir, &["good", "tag=a", "x=1"]);
    assert_eq!(mood(&dir, &["bad", "tag=b", "x=2"])["action"], "UPDATE");
    assert_eq!(
        moods(&dir),
        [(json!("bad"), json!({"tag": ["a", "b"], "x": 2}))]
    );

    // The field refuses, but not a change to `tag`.
    ok(&dir, &["setup", "mood", "--conflict", "refuse"]);
    mood(&dir, &["bad", "tag=c", "x=2"]);
    assert_eq!(
        moods(&dir),
        [(json!("bad"), json!({"tag": ["a", "b", "c"], "x": 2}))]
    );
    let argv = [
        "change",
        "mood",
        "bad",
        "tag=d",
        "x=3",
        "-m",
        "-D",
        "-d",
        "2026-11-01",
    ];
    let out = run(&dir, &argv);
    assert_eq!(out.status.code(), Some(11), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("its key x"));

    // A key on refuse refuses a change to it on a field that updates.
    ok(
        &dir,
        &[
            "setup",
            "mood",
            "--conflict",
            "",
            "--key-conflict",
            "x=refuse",
        ],
    );
    let out = run(&dir, &argv);
    assert_eq!(out.status.code(), Some(11), "{out:?}");
    assert_eq!(
        moods(&dir),
        [(json!("bad"), json!({"tag": ["a", "b", "c"], "x": 2}))]
    );

    // A line's own flag wins over the keys' too; `KEY=` clears one.
    let mut over = argv.to_vec();
    over.push("--update");
    ok(&dir, &over);
    assert_eq!(moods(&dir), [(json!("bad"), json!({"tag": "d", "x": 3}))]);
    ok(
        &dir,
        &[
            "setup",
            "mood",
            "--key-conflict",
            "tag=",
            "--key-conflict",
            "x=",
        ],
    );
    assert!(!ok(&dir, &["setup", "mood"]).contains("key conflict"));
}
