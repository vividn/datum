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

/// da-8un1i: `sync` is a group, and `sync resume` draws no JSON. The env
/// is ignored there as anywhere; a typed `--json` is refused, not dropped.
#[test]
fn a_sync_subcommand_with_no_json_ignores_the_env_and_refuses_the_flag() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    assert_eq!(
        ok(&db, "json", &["sync", "resume"]),
        "sync was not paused\n"
    );
    assert!(ok(&db, "json", &["sync", "status"]).starts_with('{'));

    let out = datum(&db, "", &["sync", "resume", "--json"]);
    assert!(!out.status.success(), "should have refused");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("sync resume: --show json is not a rendering sync resume has"),
        "{stderr:?}"
    );
}

/// da-a6p7v: so do the subcommands of `alias`, `db` and `spec` that draw no
/// JSON, having done nothing; and `redo` draws the level it used to drop.
#[test]
fn a_subcommand_with_no_json_ignores_the_env_and_refuses_the_flag() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    assert_eq!(
        ok(&db, "json", &["alias", "set", "aa", "occur tea"]),
        "✓ Alias 'aa' saved.\n"
    );
    assert!(ok(&db, "json", &["alias", "list"]).starts_with('['));

    for (name, argv) in [
        (
            "alias set",
            &["alias", "set", "zz", "occur coffee", "--json"][..],
        ),
        ("alias remove", &["alias", "remove", "aa", "--json"]),
        ("db setup", &["db", "setup", "--json"]),
        ("spec list", &["spec", "list", "--json"]),
    ] {
        let out = datum(&db, "", argv);
        assert!(!out.status.success(), "{name} should have refused");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!(
                "{name}: --show json is not a rendering {name} has"
            )),
            "{stderr:?}"
        );
    }
    let aliases = ok(&db, "", &["alias", "list"]);
    assert!(
        aliases.contains("aa") && !aliases.contains("zz"),
        "{aliases}"
    );

    ok(&db, "", &["occur", "coffee"]);
    ok(&db, "", &["undo"]);
    let redone = ok(&db, "", &["redo", "--json"]);
    assert!(redone.starts_with("{\"docs\":["), "{redone}");
}
