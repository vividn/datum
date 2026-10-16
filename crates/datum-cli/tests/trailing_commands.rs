//! Trailing commands after an alias (da-90pv8, vividn 2026-10-16): once an
//! alias's slots are filled, a word naming any command runs that command on
//! the alias's fields. Slots fill first, so a state wins over a command;
//! `.` takes a slot's default, and `...` every slot's left.

use std::path::PathBuf;
use std::process::Command;

use crate::common;

use common::run_cli;
use datum_core::test_util::{fresh_db, logged_entries};
use serde_json::json;
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let db = tmp.path().join("trailing.db");
        Self { _tmp: tmp, db }
    }

    /// `datum args…`'s stdout, which must succeed.
    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "datum {args:?}: {stderr}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

/// vividn's examples, each the same as the command it names: `outside tail`
/// is `tail environment:outside`, `envi . tail` is `tail environment`,
/// `practice ... tail` is `tail session`; an alias of two fields passes
/// both, and the words after the command are its own, those typed after
/// an alias whose body ends in it too (`et -n 1`).
#[test]
fn a_command_after_the_slots_runs_on_the_alias_fields() {
    let env = Env::new();
    for (name, body) in [
        ("envi", "switch environment"),
        ("outside", "envi outside"),
        ("practice", "occur session -k skill -k dur"),
        ("meal", "occur body , occur eat"),
        ("et", "envi . tail"),
    ] {
        env.run(&["alias", "set", name, body]);
    }
    env.run(&["outside", "-t", "-60"]);
    env.run(&["envi", "inside", "-t", "-30"]);
    env.run(&["practice", "guitar", "30m"]);
    env.run(&["meal"]);
    for (line, same_as) in [
        (
            &["outside", "tail"][..],
            &["tail", "environment:outside"][..],
        ),
        (&["envi", ".", "tail"], &["tail", "environment"]),
        (&["practice", "...", "tail"], &["tail", "session"]),
        (&["meal", "tail"], &["tail", "body,eat"]),
        (
            &["outside", "tail", "-n", "1"],
            &["tail", "environment:outside", "-n", "1"],
        ),
        (&["outside", "stats"], &["stats", "environment:outside"]),
        (&["et", "-n", "1"], &["tail", "environment", "-n", "1"]),
    ] {
        let got = env.run(line);
        assert!(!got.is_empty(), "{line:?}");
        assert_eq!(got, env.run(same_as), "{line:?}");
    }
}

/// Slots fill first: `envi tail` switches to the state «tail», and
/// `practice tail` is skill «tail».
#[test]
fn a_state_or_key_wins_over_a_command() {
    let db = fresh_db();
    run_cli(&["alias", "set", "envi", "switch environment"], &db).unwrap();
    run_cli(&["alias", "set", "practice", "occur session -k skill"], &db).unwrap();
    run_cli(&["envi", "tail"], &db).unwrap();
    run_cli(&["practice", "tail"], &db).unwrap();
    assert_eq!(
        logged_entries(&db, "environment")[0].state,
        Some(json!("tail"))
    );
    assert_eq!(
        logged_entries(&db, "session")[0].data,
        Some(json!({"skill": "tail"}))
    );
}

/// `.` takes the slot's default, as v2's `dataArgs.ts`: a switch's state is
/// true, a start's dur none, a key its default or nothing, and a required
/// key with none is refused. `...` does the same for every slot left.
#[test]
fn a_dot_takes_each_slot_default() {
    let db = fresh_db();
    for (name, body) in [
        ("envi", "switch environment"),
        ("bed", "start sleep"),
        ("cof", "occur coffee -k size=small -k temp=hot"),
        ("tea", "occur tea -k size="),
        ("juice", "occur juice -k size"),
    ] {
        run_cli(&["alias", "set", name, body], &db).unwrap();
    }
    run_cli(&["envi", "."], &db).unwrap();
    run_cli(&["bed", "."], &db).unwrap();
    run_cli(&["cof", ".", "warm"], &db).unwrap();
    run_cli(&["cof", "..."], &db).unwrap();
    run_cli(&["tea", "."], &db).unwrap();
    assert_eq!(
        logged_entries(&db, "environment")[0].state,
        Some(json!(true))
    );
    let sleep = &logged_entries(&db, "sleep")[0];
    assert_eq!(
        (sleep.state.clone(), sleep.dur.clone()),
        (Some(json!(true)), None)
    );
    let mut coffees: Vec<_> = logged_entries(&db, "coffee")
        .into_iter()
        .map(|e| e.data)
        .collect();
    coffees.sort_by_key(|d| d.as_ref().map(|d| d["temp"].to_string()));
    assert_eq!(
        coffees,
        [
            Some(json!({"size": "small", "temp": "hot"})),
            Some(json!({"size": "small", "temp": "warm"})),
        ]
    );
    assert_eq!(logged_entries(&db, "tea")[0].data, None);
    let err = run_cli(&["juice", "."], &db).unwrap_err().to_string();
    assert!(err.contains("Missing required key: size"), "{err}");
}

/// A word past the slots that names no command is still extra data.
#[test]
fn a_word_past_the_slots_naming_no_command_is_refused() {
    let db = fresh_db();
    run_cli(&["alias", "set", "practice", "occur session -k skill"], &db).unwrap();
    for words in [
        &["practice", "guitar", "zzz"][..],
        &["practice", "...", "zzz"],
    ] {
        let err = run_cli(words, &db).unwrap_err().to_string();
        assert!(err.contains("Extra data"), "{words:?}: {err}");
    }
}

/// `datum help alias` says the rule, with `.` and `...`.
#[test]
fn the_alias_page_says_the_rule() {
    let page = Env::new().run(&["help", "alias"]);
    for words in [
        "TRAILING COMMANDS:",
        "a state wins over a command",
        "datum envi . tail        tail environment",
        "`.` takes a slot's default",
        "`datum practice ... tail` is `tail session`",
    ] {
        assert!(page.contains(words), "{words}");
    }
}
