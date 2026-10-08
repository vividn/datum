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
        String::from_utf8(out.stdout).expect("utf-8 stdout")
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
