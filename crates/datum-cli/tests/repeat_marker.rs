//! A repeat is marked `!` in its echo, an `off` as an `on` or a state
//! (da-jvnrm, vividn, 2026-11-07: «a repeated 'off' no longer gets a yellow !
//! and it should»), and `tail --json` says `state_repeat` for the same rows.

use std::path::Path;
use std::process::Command;

/// The echo's info line (its second) of `datum <args>`, color off.
fn echo(db: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn datum");
    assert!(out.status.success(), "{args:?}: {out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    text.lines().nth(1).unwrap_or_default().to_string()
}

/// Each line's echo, and whether it reads as a repeat (the `!` against its state).
fn marks(db: &Path, lines: &[&[&str]], state: &[&str]) -> Vec<bool> {
    lines
        .iter()
        .zip(state)
        .map(|(args, s)| {
            let line = echo(db, args);
            assert!(line.contains(s), "{args:?}: {line:?}");
            line.contains(&format!("!{s}"))
        })
        .collect()
}

#[test]
fn a_repeated_on_off_and_state_carry_the_mark() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    let nap: &[&[&str]] = &[
        &["start", "nap"],
        &["start", "nap"],
        &["end", "nap"],
        &["end", "nap"],
    ];
    assert_eq!(
        marks(&db, nap, &["on", "on", "off", "off"]),
        [false, true, false, true]
    );
    let mood: &[&[&str]] = &[
        &["switch", "mood", "calm"],
        &["switch", "mood", "calm"],
        &["switch", "mood", "end"],
        &["switch", "mood", "end"],
    ];
    assert_eq!(
        marks(&db, mood, &["calm", "calm", "off", "off"]),
        [false, true, false, true]
    );

    for field in ["nap", "mood"] {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &db)
            .env("DATUM_HOME", dir.path())
            .env("DATUM_CONFIG_HOME", dir.path())
            .args(["tail", field, "--json"])
            .output()
            .unwrap();
        let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let rows = rows["rows"].as_array().unwrap().clone();
        let repeats: Vec<bool> = rows
            .iter()
            .map(|r| r["state_repeat"].as_bool().unwrap())
            .collect();
        assert_eq!(repeats, [false, true, false, true], "{field}");
    }

    // datum.day's echo chip draws `--show json`'s segments: the mark is one.
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", &db)
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path())
        .args(["end", "nap", "--show", "json"])
        .output()
        .unwrap();
    let echo: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let segments = echo["docs"][0]["info_segments"].as_array().unwrap();
    assert!(
        segments
            .iter()
            .any(|s| s["text"] == " !" && s["color"] == "#ffd700"),
        "{segments:?}"
    );
}
