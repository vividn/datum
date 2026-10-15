//! Every verb that takes an id takes a comma list, except the ones whose
//! grammar says one id (da-9p3o). The web's multi-select hands several hids
//! to a verb as ONE comma-joined word, and reads which verbs take a list from
//! `help --json`'s usage lines — so the behaviour and the grammar are pinned
//! together here, through the real binary (exit codes and stderr included).
//!
//! Also here: a reader that closes stdout early ends the command quietly.

use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

/// An id nothing resolves: `-` is outside the base36 a hid is drawn from, and
/// no slug starts with it. A bare `zz` resolved whenever an entry's random
/// hid began with `zz`, about one run in 650 (da-h0p2y).
const BAD_ID: &str = "no-such-id";

struct Env {
    _tmp: TempDir,
    db: PathBuf,
    home: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        Self {
            db: tmp.path().join("t.db"),
            home,
            _tmp: tmp,
        }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("HOME", &self.home)
            .env("NO_COLOR", "1")
            .args(args);
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    /// Log `field` switched to `state`, update it once (so undo has a step
    /// back), and return its short hid.
    fn entry(&self, field: &str, state: &str) -> String {
        let out = self.run(&["switch", field, state, "--show", "json"]);
        assert!(out.status.success(), "{}", stderr(&out));
        let docs: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let hid = docs["docs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["state"] == state)
            .unwrap()["hid"]
            .as_str()
            .unwrap()
            .to_string();
        let out = self.run(&["update", &hid, "note=x"]);
        assert!(out.status.success(), "{}", stderr(&out));
        hid
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// `a` occurs in `s`, and before `b`.
fn in_order(s: &str, a: &str, b: &str) -> bool {
    matches!((s.find(a), s.find(b)), (Some(i), Some(j)) if i < j)
}

#[test]
fn get_prints_each_entry_of_a_list_in_order() {
    let env = Env::new();
    let (a, b) = (env.entry("mood", "happy"), env.entry("proj", "code"));
    for extra in [&[][..], &["--pretty"][..]] {
        let list = format!("{b},{a}");
        let out = env.run(&[&["get", &list][..], extra].concat());
        assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
        assert!(in_order(&stdout(&out), "code", "happy"), "{}", stdout(&out));
    }
}

#[test]
fn get_with_a_partly_bad_list_prints_the_good_and_fails_naming_the_bad() {
    let env = Env::new();
    let (a, b) = (env.entry("mood", "happy"), env.entry("proj", "code"));
    let out = env.run(&["get", &format!("{a},{BAD_ID},{b}")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(in_order(&stdout(&out), "happy", "code"), "{}", stdout(&out));
    assert!(stderr(&out).contains(BAD_ID), "{}", stderr(&out));
}

#[test]
fn history_and_why_take_a_list_each_under_its_own_hid_line() {
    let env = Env::new();
    let (a, b) = (env.entry("mood", "happy"), env.entry("proj", "code"));
    let list = format!("{a},{b}");
    for verb in ["history", "why"] {
        let out = env.run(&[verb, &list]);
        assert_eq!(out.status.code(), Some(0), "{verb}: {}", stderr(&out));
        let text = stdout(&out);
        assert!(
            in_order(&text, &format!("hid: {a}"), &format!("hid: {b}")),
            "{verb}: {text}"
        );
    }
    let out = env.run(&["why", &format!("{a},{BAD_ID}")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains(&format!("hid: {a}")));
    assert!(stderr(&out).contains(BAD_ID), "{}", stderr(&out));
}

#[test]
fn history_json_with_a_list_is_one_document() {
    let env = Env::new();
    let (a, b) = (env.entry("mood", "happy"), env.entry("proj", "code"));
    let out = env.run(&["history", &format!("{a},{b}"), "--show", "json"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let trails: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    let hids: Vec<&str> = trails.iter().map(|t| t["hid"].as_str().unwrap()).collect();
    assert!(
        hids[0].starts_with(&a) && hids[1].starts_with(&b),
        "{hids:?}"
    );
    assert!(trails
        .iter()
        .all(|t| t["versions"].as_array().unwrap().len() == 2));
}

#[test]
fn history_refuses_a_version_or_diff_with_a_list() {
    let env = Env::new();
    let (a, b) = (env.entry("mood", "happy"), env.entry("proj", "code"));
    let list = format!("{a},{b}");
    for flags in [&["--version", "1"][..], &["--diff", "1", "2"][..]] {
        let out = env.run(&[&["history", &list][..], flags].concat());
        assert_eq!(out.status.code(), Some(1));
        assert!(
            stderr(&out).contains("ONE entry's history"),
            "{}",
            stderr(&out)
        );
        assert!(stdout(&out).is_empty());
    }
    // One id still takes them.
    let out = env.run(&["history", &a, "--version", "1"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

#[test]
fn undo_and_redo_step_every_entry_of_a_list() {
    let env = Env::new();
    let (a, b) = (env.entry("mood", "happy"), env.entry("proj", "code"));
    let list = format!("{a},{b}");
    let notes = |env: &Env| stdout(&env.run(&["get", &list])).matches("note").count();

    assert_eq!(notes(&env), 2);
    let out = env.run(&["undo", &list]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(notes(&env), 0, "both entries stepped back");

    let out = env.run(&["redo", &format!("{list},{BAD_ID}")]);
    assert_eq!(out.status.code(), Some(1), "a bad id fails the command");
    assert!(stderr(&out).contains(BAD_ID), "{}", stderr(&out));
    assert_eq!(notes(&env), 2, "the ids that resolved still moved");

    let out = env.run(&["undo", &list, "--to-version", "1"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("ONE entry's history"));
    assert_eq!(notes(&env), 2, "a refused undo writes nothing");
}

/// The LAST stderr line of a partly-bad list carries each failed id's reason
/// (da-b45wn): datum.day's bar shows only that line, and «failed for X»
/// alone said which id failed and not why.
#[test]
fn a_partly_bad_list_ends_on_each_failed_id_with_its_reason() {
    let env = Env::new();
    let a = env.entry("mood", "happy");
    // CI sets RUST_BACKTRACE, which puts a stack trace under the error line.
    let out = env
        .cmd(&["undo", &format!("{a},{BAD_ID}")])
        .env_remove("RUST_BACKTRACE")
        .env_remove("RUST_LIB_BACKTRACE")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let err = stderr(&out);
    let last = err.lines().last().unwrap_or_default();
    assert!(last.contains(BAD_ID) && last.contains("not found"), "{err}");
}

#[test]
fn help_json_declares_which_verbs_take_a_list() {
    let env = Env::new();
    let out = env.run(&["help", "--json"]);
    assert!(out.status.success());
    let grammar: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    let usage = |name: &str| -> String {
        let cmd = grammar.iter().find(|c| c["name"] == name).unwrap();
        cmd["usage"][0].as_str().unwrap().to_string()
    };
    for verb in [
        "get", "history", "why", "update", "merge", "rekey", "delete", "undo", "redo", "reparse",
        "retime",
    ] {
        assert!(
            usage(verb).contains(",ID_OR_SLUG]...")
                || usage(verb).contains(",HID]...")
                || usage(verb).contains(",SLUG]..."),
            "{verb} declares a list: {}",
            usage(verb)
        );
    }
    // `edit` opens ONE entry in $EDITOR; a conflict is resolved one at a time.
    assert_eq!(usage("edit"), "datum edit [ID_OR_SLUG]");
    let conflicts = grammar.iter().find(|c| c["name"] == "conflicts").unwrap();
    assert!(
        conflicts["usage"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u == "datum conflicts resolve <HID> [--use-a | --use-b | --prefer-a | --prefer-b | --merge] [KEY=a|b|VALUE…]")
    );
}

#[test]
fn update_help_says_several_ids_are_joined_with_commas() {
    let out = Env::new().run(&["help", "update"]);
    assert!(stdout(&out).contains("Several ids are joined with commas"));
}

/// `datum tail | head -1`: the reader is gone before datum writes. Exit as a
/// unix tool killed by SIGPIPE reports (141), with no panic on stderr — not
/// Rust's panic exit 101. The pipe's read end is closed before the spawn, so
/// the write always fails.
#[test]
fn a_closed_stdout_ends_the_command_quietly() {
    let env = Env::new();
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let out = env
        .cmd(&["help", "--json"])
        .stdout(writer)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(141), "{}", stderr(&out));
    assert!(stderr(&out).is_empty(), "{}", stderr(&out));
}
