//! `--show json` (da-yc9w): the action echo as data.
//!
//! Driven through the real binary because the mode is defined by what lands
//! on *stdout*, and because the web passes `DATUM_COLOR=1` (there is no tty)
//! — so every case here runs with colour ON, which is the condition under
//! which an un-stripped escape would leak into the JSON.

use std::path::PathBuf;
use std::process::Command;

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

    /// stdout of `datum <args...>`, with colour on (what the web passes).
    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("DATUM_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum");
        String::from_utf8(out.stdout).expect("utf-8 stdout")
    }

    /// The `docs` array of the one JSON document `args` printed.
    fn docs(&self, args: &[&str]) -> Vec<Value> {
        let stdout = self.run(args);
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
}

/// The bare shape: one doc per entry the echo printed, the three
/// addressable pieces, no data. A field's FIRST log prints two — the
/// TRACK document that declares it, then the entry (da-c8mh).
#[test]
fn occur_emits_one_doc_with_the_addressable_pieces() {
    let env = Env::new();
    let docs = env.docs(&["occur", "coffee", "--show", "json"]);
    assert_eq!(docs.len(), 2, "the declaration and the entry");
    assert_eq!(
        docs[0]["action"], "TRACK\u{25cf}",
        "the declaration names itself"
    );
    let d = &docs[1];

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
    let d = &docs[1];
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
    // The field's TRACK document names itself, so its header is not
    // `EXISTS` — both are printed and both are echoed (da-c8mh).
    let printed_hids: Vec<&str> = printed
        .lines()
        .filter(|l| l.starts_with("EXISTS ") || l.starts_with("TRACK"))
        .map(|l| l.split_whitespace().nth(1).unwrap())
        .collect();
    assert_eq!(
        printed_hids.len(),
        3,
        "the declaration and two entries: {printed:?}"
    );

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
        // The TRACK document the first log declared the field with, then
        // the entry — both printed, both echoed (da-c8mh).
        assert_eq!(docs.len(), 2, "the declaration and the entry on {field}");
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
        Some(2),
        "the documents, not silence: {stdout:?}"
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
        2
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
