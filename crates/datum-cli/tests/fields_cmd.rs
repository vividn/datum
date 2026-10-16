//! `datum fields` (da-1lnf): the plain listing is a table with a header,
//! and it leaves ignored fields out unless asked for.
//!
//! Driven through the real binary because what is being asserted is
//! *stdout* — which line comes first, and which field names are on it.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    config_home: PathBuf,
    db: PathBuf,
}

impl Env {
    /// A fresh db carrying the bead's four fields: `x` (timeless), `work`
    /// (started and ended, so `false true`), and `old` — logged, then
    /// ignored.
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        let db = tmp.path().join("fields.db");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        let env = Self {
            _tmp: tmp,
            home,
            config_home,
            db,
        };
        for argv in [
            &["add", "x", "-T"][..],
            &["start", "work"],
            &["end", "work"],
            &["occur", "old"],
            &["ignore", "old"],
        ] {
            env.run(argv);
        }
        env
    }

    fn run(&self, args: &[&str]) -> String {
        self.run_both(args).0
    }

    /// Stdout and stderr of a command that succeeds.
    fn run_both(&self, args: &[&str]) -> (String, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum");
        assert!(
            out.status.success(),
            "datum {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        (
            String::from_utf8(out.stdout).expect("utf-8 stdout"),
            String::from_utf8(out.stderr).expect("utf-8 stderr"),
        )
    }
}

#[test]
fn plain_listing_has_a_header_and_hides_ignored_fields() {
    let env = Env::new();
    let out = env.run(&["fields"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("field  states"),
        "the header is the first line of stdout:\n{out}"
    );
    assert_eq!(lines[1], "work   false true", "{out}");
    assert_eq!(lines[2], "x", "{out}");
    assert_eq!(
        lines.len(),
        3,
        "`old` is ignored, so it is not listed:\n{out}"
    );
}

/// The three spellings are one flag: `--include-ignored`, and the
/// engineering view's `-A` / `--show all`.
#[test]
fn ignored_fields_come_back_in_every_spelling() {
    let env = Env::new();
    let asked = env.run(&["fields", "--include-ignored"]);
    for argv in [
        &["fields", "-A"][..],
        &["fields", "--show", "all"],
        &["fields", "--show-all"],
    ] {
        assert_eq!(env.run(argv), asked, "{argv:?} listed something else");
    }
    let lines: Vec<&str> = asked.lines().collect();
    assert_eq!(lines[0], "field  states      ignored", "{asked}");
    assert_eq!(lines[1], "old                true", "{asked}");
    assert_eq!(lines[2], "work   false true", "{asked}");
    assert_eq!(lines[3], "x", "{asked}");
}

/// `--json` is UNCHANGED: every field, with its `ignored` flag. The web
/// widget bar and `web/tests/fields.test.mjs` read it.
#[test]
fn json_still_carries_every_field_and_prints_no_header() {
    let env = Env::new();
    let out = env.run(&["fields", "--json"]);
    assert!(
        !out.starts_with("field"),
        "the header must not reach --json:\n{out}"
    );
    let listing: Value = serde_json::from_str(&out).expect("--json is a JSON document");
    let rows = listing.as_array().expect("an array of fields");
    assert_eq!(
        rows.iter()
            .map(|r| (
                r["field"].as_str().unwrap(),
                r["ignored"].as_bool().unwrap()
            ))
            .collect::<Vec<_>>(),
        [("old", true), ("work", false), ("x", false)],
        "--json shape changed:\n{out}"
    );
    // The flags that reshape the plain listing leave `--json` alone.
    for argv in [
        &["fields", "--json", "--include-ignored"][..],
        &["fields", "--json", "-A"],
    ] {
        assert_eq!(env.run(argv), out, "{argv:?} changed the JSON");
    }
}

/// da-t3quu: `ignore` on a name datum has never seen — a typo, most often —
/// still writes the row, and says on stderr that there was no such field.
#[test]
fn ignoring_a_never_seen_field_notes_it_and_writes_the_row() {
    let env = Env::new();
    let (out, err) = env.run_both(&["ignore", "moood"]);
    assert_eq!(out, "✓ Ignoring 'moood'\n");
    assert_eq!(
        err,
        "note: no field 'moood' yet (no entries, no spec); ignoring it anyway\n"
    );
    let doc: Value = serde_json::from_str(&env.run(&["fields", "--json"])).unwrap();
    let row = doc
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["field"] == "moood")
        .expect("the IGNORED row makes it a field");
    assert_eq!(row["ignored"], true);
}

/// A field datum knows, by an entry or by a spec alone (a kit's), is
/// ignored with today's output and nothing on stderr.
#[test]
fn ignoring_a_known_field_prints_no_note() {
    let env = Env::new();
    env.run(&["setup", "declared", "--verb", "occur"]);
    for field in ["work", "declared"] {
        let (out, err) = env.run_both(&["ignore", field]);
        assert_eq!(out, format!("✓ Ignoring '{field}'\n"));
        assert_eq!(err, "", "{field} is known");
    }
}

/// da-mekab: a command that begins nothing says so of a name datum has
/// never seen, as `ignore` does, and writes its row; of a field known by an
/// entry or by a spec alone, and of the same name again, it says nothing.
/// `argv`'s `FIELD` is the field.
fn notes_only_a_never_seen_field(argv: &[&str], doing: &str) {
    let env = Env::new();
    env.run(&["setup", "declared", "--verb", "start"]);
    let stderr = |field: &str| {
        let line: Vec<&str> = argv
            .iter()
            .map(|w| if *w == "FIELD" { field } else { w })
            .collect();
        env.run_both(&line).1
    };
    assert_eq!(stderr("work"), "", "work has entries");
    assert_eq!(stderr("declared"), "", "declared has a spec");
    assert_eq!(
        stderr("moood"),
        format!("note: no field 'moood' yet (no entries, no spec); {doing} it anyway\n")
    );
    assert_eq!(stderr("moood"), "", "the first row made it a field");
    let all: Value = serde_json::from_str(&env.run(&["fields", "--json"])).unwrap();
    let rows = all.as_array().unwrap();
    assert!(rows.iter().any(|r| r["field"] == "moood"), "{all}");
}

#[test]
fn ending_a_never_seen_field_notes_it() {
    notes_only_a_never_seen_field(&["end", "FIELD"], "ending");
    notes_only_a_never_seen_field(&["stop", "FIELD"], "ending");
}

/// A dry run ends nothing, so it says no «ending it anyway»; the end after
/// it, the first to write, does.
#[test]
fn a_dry_run_end_of_a_never_seen_field_has_no_note() {
    let env = Env::new();
    let dry = env.run_both(&["end", "moood", "--dry-run"]).1;
    assert!(!dry.contains("note:"), "{dry}");
    let wet = env.run_both(&["end", "moood"]).1;
    assert!(wet.starts_with("note: no field 'moood' yet"), "{wet}");
}

#[test]
fn flagging_a_never_seen_field_notes_it() {
    notes_only_a_never_seen_field(&["flag", "FIELD", "check this"], "flagging");
}

#[test]
fn a_meta_note_on_a_never_seen_field_notes_it() {
    notes_only_a_never_seen_field(&["meta", "FIELD", "protocol"], "noting");
}

/// da-mekab: `occur`, `start` and `switch` are how a field is born, so a
/// note there would be noise.
#[test]
fn what_begins_a_field_has_no_note() {
    let env = Env::new();
    for argv in [&["occur", "a"][..], &["start", "b"], &["switch", "c", "on"]] {
        assert_eq!(env.run_both(argv).1, "", "{argv:?}");
    }
}

/// da-wt4bb: a flag's comment and a meta note's text are stored in `state`,
/// and are no state the field was switched to. A state that is also some
/// flag's comment is still one.
#[test]
fn a_flags_comment_and_a_meta_notes_text_are_not_the_fields_states() {
    let env = Env::new();
    for argv in [
        &["switch", "mood", "good", "-t", "-2h"][..],
        &["switch", "mood", "tired", "-t", "-1h"],
        &["flag", "mood", "come back to this"],
        &["meta", "mood", "rate it at bedtime"],
        &["flag", "mood", "tired"],
    ] {
        env.run(argv);
    }
    let out = env.run(&["fields"]);
    assert_eq!(out.lines().nth(1), Some("mood   good tired"), "{out}");
    let json: Value = serde_json::from_str(&env.run(&["fields", "--json"])).unwrap();
    let mood = json
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["field"] == "mood");
    assert_eq!(
        mood.unwrap()["states"],
        serde_json::json!(["good", "tired"])
    );
}
