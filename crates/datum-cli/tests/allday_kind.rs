//! da-rvvce: the all-day field kind. Its entries are always full-day, as
//! `-D` makes them, on today or the day `-d` names, and `-t` is refused.

use std::process::{Command, Output};

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

fn echo(dir: &TempDir, args: &[&str]) -> serde_json::Value {
    let mut argv = args.to_vec();
    argv.extend(["--show", "json"]);
    let out: serde_json::Value = serde_json::from_str(&ok(dir, &argv)).unwrap();
    out["docs"][0].clone()
}

#[test]
fn an_allday_field_logs_whole_days_and_refuses_a_clock() {
    let dir = tempfile::tempdir().unwrap();
    ok(&dir, &["setup", "steps", "--kind", "allday"]);
    assert!(ok(&dir, &["setup", "steps"]).contains("allday"));

    let today = chrono::Utc::now().date_naive().to_string();
    let doc = echo(&dir, &["steps", "8000"]);
    assert_eq!(doc["shape"], "ALLDAY", "{doc}");
    assert_eq!(doc["id"], format!("steps:{today}"), "{doc}");
    assert_eq!(doc["state"], 8000, "{doc}");

    let doc = echo(&dir, &["allday", "steps", "9000", "-d", "2026-11-01"]);
    assert_eq!(doc["shape"], "ALLDAY", "{doc}");
    assert_eq!(doc["id"], "steps:2026-11-01", "{doc}");

    for clock in [&["-t", "10:00"][..], &["-q"]] {
        let mut argv = vec!["steps", "7000"];
        argv.extend(clock);
        let out = run(&dir, &argv);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{argv:?} must be refused");
        assert!(
            stderr.contains("allday") && stderr.contains("no time"),
            "{stderr}"
        );
    }
    // Nothing was written by the refusals.
    let tail = ok(&dir, &["tail", "steps"]);
    assert!(!tail.contains("7000"), "{tail}");
}

/// The day's value is the day's alone: a point, not a state the field
/// holds on into the next day.
#[test]
fn an_allday_entry_is_a_point_with_the_days_value() {
    let dir = tempfile::tempdir().unwrap();
    let doc = echo(&dir, &["allday", "rating", "7", "-y"]);
    assert_eq!(doc["shape"], "ALLDAY", "{doc}");
    // A first log declares its field with the kind it was logged by.
    assert!(ok(&dir, &["setup", "rating"]).contains("allday"));
    // Its first log carries the field's TRACKED declaration, as a
    // moment's does; the next is a bare point.
    let doc = echo(&dir, &["rating", "6", "-d", "2026-11-01"]);
    let hid = doc["hid"].as_str().unwrap().to_string();
    let all = ok(&dir, &["get", &hid, "-A"]);
    let verb = all
        .lines()
        .find(|l| l.trim_start().starts_with("verb "))
        .unwrap();
    assert_eq!(verb.split_whitespace().nth(1), Some("point"), "{all}");
}

#[test]
fn setup_help_names_the_allday_kind() {
    let dir = tempfile::tempdir().unwrap();
    let help = ok(&dir, &["setup", "--help"]);
    assert!(
        help.contains("  allday   a value for a whole day   datum allday steps 8000"),
        "{help}"
    );
    let err = run(&dir, &["setup", "x", "--kind", "nope"]);
    assert!(String::from_utf8_lossy(&err.stderr).contains("allday"));
}
