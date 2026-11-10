//! da-w48r3: `tail --state` lists only the entries in one of the states
//! named (vividn, 2026-11-09: «when you have a state (or states) selected,
//! filter the tail to only show entries of that state/s»): several are any
//! of them, read as a state on the line is, and an array state is in each of
//! its elements. `-n` counts the rows that match.

use std::process::Command;
use tempfile::TempDir;

fn datum(dir: &TempDir, args: &[&str]) -> String {
    let d = dir.path();
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .args(args)
        .env("HOME", d)
        .env("DATUM_HOME", d.join("home"))
        .env("DATUM_CONFIG_HOME", d.join("config"))
        .env("XDG_CONFIG_HOME", d.join("xc"))
        .env("XDG_DATA_HOME", d.join("xd"))
        .env("XDG_STATE_HOME", d.join("xs"))
        .env("DATUM_DB", d.join("data.db"))
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The states `tail mood --json` lists with `extra` after it, oldest first.
fn states(dir: &TempDir, extra: &[&str]) -> Vec<serde_json::Value> {
    let args = [&["tail", "mood", "--json"], extra].concat();
    let doc: serde_json::Value = serde_json::from_str(&datum(dir, &args)).unwrap();
    let rows = doc["rows"].as_array().unwrap();
    rows.iter().map(|r| r["state"].clone()).collect()
}

#[test]
fn state_narrows_the_tail_and_several_are_any_of_them() {
    let dir = TempDir::new().unwrap();
    for (t, state) in [
        ("-300", "happy"),
        ("-240", "a,b"),
        ("-180", "sad"),
        ("-150", "true"),
        ("-120", r#"["x,y","z"]"#),
        ("-60", "happy"),
    ] {
        datum(&dir, &["change", "mood", state, "-t", t]);
    }
    datum(&dir, &["moment", "tea", "-t", "-30"]);
    let s = |v: &str| serde_json::json!(v);

    assert_eq!(
        states(&dir, &["--state", "happy"]),
        [s("happy"), s("happy")]
    );
    // Two states, as one array token or as two flags, are either.
    let either = [s("happy"), s("sad"), s("happy")];
    assert_eq!(states(&dir, &["--state", "sad,happy"]), either);
    assert_eq!(
        states(&dir, &["--state", "sad", "--state", "happy"]),
        either
    );
    // An array state is in each of its elements; the bracket form keeps a comma.
    assert_eq!(
        states(&dir, &["--state", "b"]),
        [serde_json::json!(["a", "b"])]
    );
    assert_eq!(
        states(&dir, &["--state", r#"["x,y"]"#]),
        [serde_json::json!(["x,y", "z"])]
    );
    assert_eq!(
        states(&dir, &["--state", "x"]),
        Vec::<serde_json::Value>::new()
    );
    // Read as a state is: `true` is the boolean.
    assert_eq!(
        states(&dir, &["--state", "true"]),
        [serde_json::json!(true)]
    );
    // `-n` counts the matching rows.
    assert_eq!(
        states(&dir, &["--state", "sad,happy", "-n", "2"]),
        either[1..]
    );
    // None named, the whole listing; across fields, only the matching ones.
    assert_eq!(states(&dir, &[]).len(), 6);
    let all: serde_json::Value =
        serde_json::from_str(&datum(&dir, &["tail", "--json", "--state", "sad"])).unwrap();
    assert_eq!(all["rows"].as_array().unwrap().len(), 1, "{all}");
}
