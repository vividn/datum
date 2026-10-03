//! `--show json` (da-yc9w): the action echo as data.
//!
//! Driven through the real binary because the mode is defined by what lands
//! on *stdout*, and because the web passes `DATUM_COLOR=1` (there is no tty)
//! — so every case here runs with colour ON, which is the condition under
//! which an un-stripped escape would leak into the JSON.

use std::path::PathBuf;
use std::process::{Command, Output};

use datum_cli::output::strip_ansi;
use serde_json::Value;
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
        let db = tmp.path().join("show_json.db");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        Self {
            _tmp: tmp,
            home,
            config_home,
            db,
        }
    }

    /// `datum <args...>`, with colour on (what the web passes).
    fn output(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("DATUM_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum")
    }

    /// stdout of `datum <args...>`.
    fn run(&self, args: &[&str]) -> String {
        String::from_utf8(self.output(args).stdout).expect("utf-8 stdout")
    }

    /// The `docs` array of the one JSON document `args` printed.
    fn docs(&self, args: &[&str]) -> Vec<Value> {
        parse_docs(&self.run(args))
    }

    /// Run `args`, which must FAIL on the token `bad`, and return the docs
    /// it printed regardless.
    fn failing_docs(&self, args: &[&str], bad: &str) -> Vec<Value> {
        let out = self.output(args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?} must still fail");
        assert!(stderr.contains(bad), "the error names {bad}: {stderr}");
        parse_docs(&String::from_utf8(out.stdout).expect("utf-8 stdout"))
    }
}

/// The `docs` array of the one JSON document in `stdout`.
fn parse_docs(stdout: &str) -> Vec<Value> {
    assert!(
        !stdout.contains('\u{1b}'),
        "`--show json` must carry text, not escapes: {stdout:?}"
    );
    assert_eq!(
        stdout.lines().count(),
        1,
        "one JSON document on stdout and nothing else, got: {stdout:?}"
    );
    let doc: Value = serde_json::from_str(stdout.trim_end())
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout:?}"));
    doc["docs"].as_array().cloned().expect("docs array")
}

/// The bare shape: one doc per entry the echo printed, the three
/// addressable pieces, no data. A field's FIRST log prints ONE — the
/// declaration rides in the entry's own `following_state` (da-gvcen).
#[test]
fn occur_emits_one_doc_with_the_addressable_pieces() {
    let env = Env::new();
    let docs = env.docs(&["occur", "coffee", "--show", "json"]);
    assert_eq!(docs.len(), 1, "one entry, one doc");
    let d = &docs[0];

    assert_eq!(d["action"], "CREATE");
    let hid = d["hid"].as_str().unwrap();
    let id = d["id"].as_str().unwrap();
    assert_eq!(hid.len(), 5, "the displayed hid, not the stored 12: {hid}");
    assert!(
        id.starts_with("coffee:"),
        "the full slug, not the hid: {id}"
    );
    assert_eq!(
        d["header"].as_str().unwrap(),
        format!("CREATE {hid} {id}"),
        "the header is exactly its three pieces"
    );
    assert!(d["info"].as_str().unwrap().contains("coffee"));
    assert_eq!(d["data"].as_array().unwrap().len(), 0, "no data was given");
    assert_eq!(d["data_inline"], "");
}

/// One string per key, and an inline join the caller never has to compose.
/// Asserting on the joined string is what a per-key-only assertion misses:
/// a join bug would still pass the array check.
#[test]
fn data_is_one_string_per_key_and_the_inline_join_holds_them_all() {
    let env = Env::new();
    let docs = env.docs(&[
        "occur",
        "coffee",
        "beans=ethiopian",
        "cups=2",
        "grind=fine",
        "--show",
        "json",
    ]);
    let d = &docs[0];
    let items: Vec<&str> = d["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(items.len(), 3, "one string per key, got {items:?}");
    assert!(items.contains(&"beans: \"ethiopian\""), "{items:?}");
    assert!(items.contains(&"cups: 2"), "{items:?}");
    assert!(items.contains(&"grind: \"fine\""), "{items:?}");

    let inline = d["data_inline"].as_str().unwrap();
    for key in ["beans", "cups", "grind"] {
        assert!(
            inline.contains(key),
            "inline join dropped {key}: {inline:?}"
        );
    }
    assert_eq!(inline, items.join(", "), "inline is the CLI's own join");
}

/// A command that echoes several entries emits one doc per entry, in the
/// order it printed them — this is what lets a caller draw a section each.
#[test]
fn several_entries_emit_several_docs_in_printed_order() {
    let env = Env::new();
    env.run(&["occur", "coffee", "cups=1", "--show", "none"]);
    env.run(&["occur", "coffee", "cups=2", "--show", "none"]);

    let printed = strip_ansi(&env.run(&["get", "coffee"]));
    // Two entries and no declaration row: the first log declares its
    // field in its own columns (da-gvcen).
    let printed_hids: Vec<&str> = printed
        .lines()
        .filter(|l| l.starts_with("EXISTS "))
        .map(|l| l.split_whitespace().nth(1).unwrap())
        .collect();
    assert_eq!(printed_hids.len(), 2, "two entries: {printed:?}");

    let docs = env.docs(&["get", "coffee", "--show", "json"]);
    let json_hids: Vec<&str> = docs.iter().map(|d| d["hid"].as_str().unwrap()).collect();
    assert_eq!(json_hids, printed_hids, "same docs, same order");
}

/// The anti-drift gate. The JSON is not a second formatter: its `header` and
/// `info` are byte-for-byte what `--show default` prints for the same entry,
/// ANSI stripped. If anyone adds a second path that formats an action, this
/// is the test that fails.
#[test]
fn header_and_info_are_byte_identical_to_the_default_show() {
    let env = Env::new();
    for args in [
        vec!["occur", "coffee", "beans=ethiopian", "cups=2", "grind=fine"],
        vec!["switch", "mood", "happy"],
        vec!["start", "work", "project=datum"],
    ] {
        env.run(&[args.clone(), vec!["--show", "none"]].concat());
        let field = args[1];
        let printed = strip_ansi(&env.run(&["get", field]));
        let mut printed_lines = printed.lines();
        let header = printed_lines.next().expect("header line");
        let info = printed_lines.next().unwrap_or_default();

        let docs = env.docs(&["get", field, "--show", "json"]);
        // One row per log, declaration included (da-gvcen).
        assert_eq!(docs.len(), 1, "the entry on {field}");
        assert_eq!(docs[0]["header"], header, "header drifted on {field}");
        assert_eq!(docs[0]["info"], info, "info drifted on {field}");
    }
}

/// The LAST `--show` wins, in both directions (da-qt6z). `none` used to
/// latch over anything after it; vividn asked for last-wins, so silence is
/// now as overridable as every other value.
#[test]
fn the_last_show_wins_over_none_in_both_directions() {
    let env = Env::new();
    let stdout = env.run(&["occur", "coffee", "--show", "none", "--show", "json"]);
    let doc: Value = serde_json::from_str(stdout.trim_end()).unwrap_or_else(|e| {
        panic!("--show none --show json prints the document ({e}): {stdout:?}")
    });
    assert_eq!(
        doc["docs"].as_array().map(Vec::len),
        Some(1),
        "the document, not silence: {stdout:?}"
    );
    assert_eq!(
        env.run(&["occur", "tea", "--show", "json", "--show", "none"]),
        "",
        "--show json --show none is silent"
    );
    // `--json` is exactly `--show json`, so the pair is still json.
    assert_eq!(
        env.docs(&["occur", "milk", "--show", "json", "--json"])
            .len(),
        1
    );
}

/// An unknown value is still not a `Show`. (The CLI's `resolve()` has always
/// fallen back to the default show for one — `--show json` changes nothing
/// about that; what must not happen is an unknown value quietly parsing.)
#[test]
fn an_unknown_show_value_still_errors() {
    use std::str::FromStr;
    assert!(datum_cli::output::Show::from_str("bogus").is_err());
    assert!(datum_parse::input::output_args::Show::from_str("bogus").is_err());
    assert_eq!(
        datum_cli::output::Show::from_str("json").unwrap(),
        datum_cli::output::Show::Json
    );
}

/// A batch that writes and then fails on a later token still echoes the
/// writes it made (da-0uvyr): the default output prints them before the
/// error, and datum.day, which runs everything under `--show json`, must
/// not be told only «failed» about a command that deleted an entry.
#[test]
fn a_partial_del_echoes_the_delete_it_made() {
    let env = Env::new();
    env.run(&["occur", "coffee"]);
    let tea = env.docs(&["occur", "tea", "--show", "json"]);
    let hid = tea[0]["hid"].as_str().unwrap();

    let docs = env.failing_docs(&["del", &format!("{hid},nope9"), "--show", "json"], "nope9");
    let deleted: Vec<_> = docs.iter().filter(|d| d["action"] == "DELETE").collect();
    assert!(
        deleted.iter().any(|d| d["hid"] == hid),
        "tea's DELETE is echoed: {docs:?}"
    );
    assert!(
        !env.run(&["tail"]).contains("tea"),
        "and tea really is gone"
    );
}

/// The same flush covers `update`: the good token's UPDATE is echoed.
#[test]
fn a_partial_update_echoes_the_update_it_made() {
    let env = Env::new();
    let tea = env.docs(&["occur", "tea", "--show", "json"]);
    let hid = tea[0]["hid"].as_str().unwrap();

    let docs = env.failing_docs(
        &["update", &format!("{hid},nope9"), "n=1", "--show", "json"],
        "nope9",
    );
    assert_eq!(docs.len(), 1, "{docs:?}");
    assert_eq!(docs[0]["action"], "UPDATE");
    assert_eq!(docs[0]["hid"], hid);
    assert_eq!(docs[0]["data"], serde_json::json!(["n: 1"]));
}
