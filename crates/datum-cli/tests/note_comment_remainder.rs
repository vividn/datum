//! `note` routes bare words to the comment by default (da-py5w).
//!
//! `note` is the one logging verb whose un-keyed positionals are prose:
//! it behaves as if `-C`/`--comment-remainder` were given, so
//! `note coffee too strong today` needs no quotes and no flag. The other
//! verbs still refuse a bare word — for them it would be a state or a typo.
//!
//! Real binary, because the claim is about what a person's shell does:
//! exit codes, the printed `--show json` echo, and the stored row.

use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    config_home: PathBuf,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        let db = tmp.path().join("note.db");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        Self {
            _tmp: tmp,
            home,
            config_home,
            db,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("NO_COLOR", "1")
            .env("TZ", "UTC")
            .args(args)
            .output()
            .expect("spawn datum")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "`datum {}` failed ({}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).expect("utf-8 stdout")
    }
}

/// What cannot match across two databases: the hid, and every digit of the
/// create_time the timeless slug and the `info` clock are built from.
/// Flattened so the rest of the echo can be compared as the bytes it is.
fn mask(json: &str, hids: &[String]) -> String {
    let mut out = json.to_string();
    for hid in hids {
        out = out.replace(hid.as_str(), "HID");
    }
    out.chars()
        .map(|c| if c.is_ascii_digit() { '0' } else { c })
        .collect()
}

/// Every `hid` in a `--show json` echo. A field's first log echoes two
/// documents — the TRACK one that declares it, then the entry (da-c8mh).
fn hids_of(json: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(json.trim()).expect("json echo");
    v["docs"]
        .as_array()
        .expect("docs")
        .iter()
        .map(|d| d["hid"].as_str().expect("hid").to_string())
        .collect()
}

/// The stored `data` object of the newest entry for `field`.
fn data_of(env: &Env, field: &str) -> serde_json::Value {
    let json = env.ok(&["tail", "-n", "1", field, "--show", "json"]);
    let v: serde_json::Value = serde_json::from_str(json.trim()).expect("tail json");
    v["rows"].as_array().expect("rows").last().expect("a row")["data"].clone()
}

#[test]
fn bare_words_become_the_comment() {
    let env = Env::new();
    env.ok(&["note", "coffee", "too", "strong", "today"]);

    assert_eq!(
        data_of(&env, "coffee"),
        serde_json::json!({ "comment": ["too strong today"] }),
        "the words join into `comment`, in order, single-spaced — and nothing else lands",
    );
    // …and the human view says so too.
    let shown = env.ok(&["get", "coffee"]);
    assert!(
        shown.contains(r#"comment: "too strong today""#),
        "`get` should show the comment, got:\n{shown}",
    );
}

#[test]
fn key_value_tokens_stay_data_around_the_words() {
    let env = Env::new();
    env.ok(&["note", "coffee", "tasted=burnt", "too", "strong", "today"]);

    assert_eq!(
        data_of(&env, "coffee"),
        serde_json::json!({ "comment": ["too strong today"], "tasted": "burnt" }),
        "a `KEY=VALUE` anywhere in the tail is data; the rest is the comment",
    );
}

#[test]
fn explicit_c_is_redundant_not_an_error() {
    let bare = Env::new();
    let flagged = Env::new();

    let a = bare.ok(&["note", "coffee", "same", "words", "--show", "json"]);
    let b = flagged.ok(&["note", "coffee", "-C", "same", "words", "--show", "json"]);

    assert_eq!(
        mask(&a, &hids_of(&a)),
        mask(&b, &hids_of(&b)),
        "an explicit `-C` must be accepted and change nothing",
    );
}

#[test]
fn a_leading_dash_is_still_a_flag() {
    let env = Env::new();
    // `-t` places the entry; only `late` is comment text.
    env.ok(&["note", "coffee", "-t", "9:00", "late"]);

    assert_eq!(
        data_of(&env, "coffee"),
        serde_json::json!({ "comment": ["late"] }),
    );
    let json = env.ok(&["tail", "-n", "1", "coffee", "--show", "json"]);
    let v: serde_json::Value = serde_json::from_str(json.trim()).unwrap();
    assert!(
        v["rows"][0]["time"].as_str().unwrap().contains("T09:00:00"),
        "`-t 9:00` must still place the entry, got {}",
        v["rows"][0]["time"],
    );
}

#[test]
fn an_explicit_remainder_bucket_still_wins() {
    let env = Env::new();
    env.ok(&["note", "tea", "-R", "sips", "one", "two"]);

    assert_eq!(
        data_of(&env, "tea"),
        serde_json::json!({ "sips": ["one", "two"] }),
        "`-R <bucket>` names where the remainder goes — and keeps its array shape",
    );
}

#[test]
fn the_other_verbs_still_refuse_a_bare_word() {
    let env = Env::new();
    for verb in ["occur", "start", "switch", "add"] {
        let out = env.run(&[verb, "coffee", "too", "strong"]);
        assert!(
            !out.status.success(),
            "`{verb} coffee too strong` must still be an error — for these verbs a bare \
             word is a state or a typo, not prose",
        );
    }
}

#[test]
fn help_documents_the_rule_and_drops_the_flag() {
    let env = Env::new();
    let help = env.ok(&["help", "note"]);

    assert!(
        !help.contains("--comment-remainder"),
        "`-C` is implicit on `note`, so its OPTIONS must not offer it:\n{help}",
    );
    assert!(
        help.contains("comment"),
        "`datum help note` must say where the words go:\n{help}",
    );
    // The sibling verbs keep the flag. da-ce7n3 folded the shared logging
    // wall onto `datum help logging`, so that is where a sibling's `-C` is
    // now written down — and the topic page has to name `note` as the one
    // verb it is not true for, or folding the wall would have hidden the
    // difference this bead exists to state.
    let topic = env.ok(&["help", "logging"]);
    assert!(
        topic.contains("--comment-remainder"),
        "`datum help logging` documents the shared flag:\n{topic}",
    );
    assert!(
        topic.contains("`note` is the exception"),
        "`datum help logging` must say `note` has no -C:\n{topic}",
    );
    // And the parser still accepts it on a sibling, folded page or not.
    assert!(env.ok(&["help", "occur"]).contains("datum help logging"));
}
