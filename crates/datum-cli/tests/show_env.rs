//! `DATUM_SHOW`: a `--show` asked for from outside argv (da-vx31e).
//!
//! datum.day wants every write's echo as JSON, and appending `--show json`
//! made every entry it logged record the flag in `raw_input`, so `why`
//! quoted a line nobody typed. The env asks for the same rendering and
//! leaves argv — and the record — what was typed. A typed `--show` wins,
//! and a command with no rendering for the value ignores it.

use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn datum(db: &Path, show: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("NO_COLOR", "1")
        .env("DATUM_SHOW", show)
        .args(args)
        .output()
        .expect("spawn datum")
}

fn ok(db: &Path, show: &str, args: &[&str]) -> String {
    let out = datum(db, show, args);
    assert!(
        out.status.success(),
        "DATUM_SHOW={show} datum {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

#[test]
fn the_echo_is_json_and_the_record_is_what_was_typed() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");

    let echo = ok(&db, "json", &["occur", "coffee"]);
    assert!(echo.starts_with("{\"docs\":["), "{echo}");

    // `why` has no JSON rendering, so the same env leaves it alone.
    let why = ok(&db, "json", &["why", "_LAST"]);
    let raw = why.lines().find(|l| l.contains("raw_input:")).expect(&why);
    assert_eq!(
        raw.split_once("raw_input:").unwrap().1.trim(),
        "occur coffee"
    );
}

#[test]
fn help_is_still_the_help() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    let help = ok(&db, "json", &["help"]);
    assert!(help.starts_with("datum"), "{help}");
}

#[test]
fn a_typed_show_wins() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    assert_eq!(ok(&db, "json", &["occur", "x", "--show", "none"]), "");
    // Empty is unset: the default echo.
    assert!(ok(&db, "", &["occur", "y"]).starts_with("CREATE "));
}

#[test]
fn a_value_show_would_refuse_is_an_error() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    let out = datum(&db, "banana", &["occur", "x"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("DATUM_SHOW"));
}
