//! `datum import` and `datum export --jsonl` (da-jouhm): an export imports
//! back to the same entries; a timed row with no `at` is refused by its line;
//! a given `create_time` is kept; one bad row writes nothing, unless
//! `--allow-partial`; a second run writes nothing new; one `undo` takes the
//! import back; `--dry-run` writes nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
    home: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        Self { tmp, home }
    }

    fn db(&self, name: &str) -> PathBuf {
        self.tmp.path().join(format!("{name}.db"))
    }

    /// `datum <args>` on database `db`, with `stdin`.
    fn datum(&self, db: &str, args: &[&str], stdin: &str) -> Output {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(args)
            .env("DATUM_DB", self.db(db))
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("TZ", "Europe/Berlin")
            .env("NO_COLOR", "1")
            .env("RUST_BACKTRACE", "0")
            .env("RUST_LIB_BACKTRACE", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    /// `datum import -` of `rows` into `db`.
    fn import(&self, db: &str, rows: &[&str], flags: &[&str]) -> Output {
        self.datum(
            db,
            &[&["import", "-"][..], flags].concat(),
            &rows.join("\n"),
        )
    }

    fn ok(&self, db: &str, args: &[&str]) -> String {
        let out = self.datum(db, args, "");
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
        text(&out.stdout)
    }

    fn export(&self, db: &str) -> String {
        if !Path::new(&self.db(db)).exists() {
            return String::new();
        }
        self.ok(db, &["export", "--jsonl"])
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

const COFFEE: &str = r#"{"field": "coffee", "at": "2026-11-10T08:30:00+01:00", "dur": "MOMENT"}"#;
const MOOD: &str = r#"{"field": "mood", "at": "2026-11-10 09:00", "state": "happy"}"#;
const UNTIMED: &str = r#"{"field": "mood", "state": "sad"}"#;

#[test]
fn an_export_imports_back_to_the_same_entries() {
    let env = Env::new();
    for line in [
        &["moment", "coffee", "-y", "-t", "9:00", "cups=2"][..],
        &["start", "work", "-y", "-t", "10:00"],
        &["switch", "mood", "happy", "-y", "-t", "11:00"],
        &["stop", "work", "-y", "-t", "12:00"],
        &["on", "walk", "30m", "-y", "-t", "13:00"],
        &["note", "jot", "hello", "there"],
    ] {
        env.ok("a", line);
    }
    let a = env.export("a");
    assert_eq!(a.lines().count(), 6, "{a}");
    let rows: Vec<&str> = a.lines().collect();
    let out = env.import("b", &rows, &[]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(env.export("b"), a);
    // The untimed field is a record in both, so a second run reads it so too.
    assert_eq!(
        env.ok("b", &["setup", "jot"]),
        env.ok("a", &["setup", "jot"])
    );
}

#[test]
fn a_timed_row_without_at_is_refused_by_its_line_and_nothing_is_written() {
    let env = Env::new();
    let out = env.import("a", &[COFFEE, UNTIMED], &[]);
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(
        err.contains("line 2:") && err.contains("`at` is required"),
        "{err}"
    );
    assert!(!err.contains("line 1:"), "{err}");
    assert_eq!(env.export("a"), "");
}

#[test]
fn allow_partial_writes_the_rows_that_read_and_still_exits_1() {
    let env = Env::new();
    let out = env.import("a", &[COFFEE, "not json", MOOD], &["--allow-partial"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("line 2: not JSON"),
        "{}",
        text(&out.stderr)
    );
    let exported = env.export("a");
    assert_eq!(exported.lines().count(), 2, "{exported}");
}

#[test]
fn the_summary_names_the_zone_only_when_a_time_named_none() {
    let env = Env::new();
    let zoned = env.import("a", &[COFFEE], &[]);
    assert!(zoned.status.success(), "{}", text(&zoned.stderr));
    assert!(
        !text(&zoned.stdout).contains("naming no zone"),
        "{}",
        text(&zoned.stdout)
    );
    let bare = env.import("a", &[MOOD], &["--tz", "America/Chicago"]);
    assert!(
        text(&bare.stdout).contains("naming no zone was read in America/Chicago"),
        "{}",
        text(&bare.stdout)
    );
}

#[test]
fn a_given_create_time_is_the_entrys_own() {
    let env = Env::new();
    let row = r#"{"field": "coffee", "at": "2026-11-10T08:30:00Z", "create_time": "2026-11-10T08:31:00Z"}"#;
    assert!(env.import("a", &[row], &[]).status.success());
    let exported = env.export("a");
    assert!(
        exported.contains(r#""create_time":"2026-11-10T08:31:00Z""#),
        "{exported}"
    );
}

#[test]
fn a_second_run_writes_nothing_new_and_one_undo_takes_the_import_back() {
    let env = Env::new();
    env.ok("a", &["moment", "tea", "-y", "-t", "7:00"]);
    let before = env.export("a");
    assert!(env.import("a", &[COFFEE, MOOD], &[]).status.success());
    let imported = env.export("a");
    assert_eq!(imported.lines().count(), 3, "{imported}");

    let again = env.import("a", &[COFFEE, MOOD], &[]);
    assert!(again.status.success());
    assert!(
        text(&again.stdout).contains("Imported 0 entries"),
        "{}",
        text(&again.stdout)
    );
    assert_eq!(env.export("a"), imported);

    env.ok("a", &["undo"]);
    assert_eq!(env.export("a"), before);
}

#[test]
fn a_row_there_with_other_content_is_kept_and_named() {
    let env = Env::new();
    assert!(env.import("a", &[MOOD], &[]).status.success());
    let other = r#"{"field": "mood", "at": "2026-11-10 09:00", "state": "sad"}"#;
    let out = env.import("a", &[other], &[]);
    assert!(out.status.success());
    assert!(
        text(&out.stdout).contains("other content"),
        "{}",
        text(&out.stdout)
    );
    assert!(env.export("a").contains(r#""state":"happy""#));
}

#[test]
fn dry_run_writes_nothing() {
    let env = Env::new();
    env.ok("a", &["moment", "tea", "-y", "-t", "7:00"]);
    let before = env.export("a");
    for flags in [
        &["import", "-", "--dry-run"][..],
        &["--dry-run", "import", "-"],
    ] {
        let out = env.datum("a", flags, &[COFFEE, MOOD].join("\n"));
        assert!(out.status.success(), "{}", text(&out.stderr));
        assert!(
            text(&out.stdout).contains("Would import 2 entries"),
            "{}",
            text(&out.stdout)
        );
        assert_eq!(env.export("a"), before);
    }
    // Nor makes the database it names.
    let out = env.datum("new", &["import", "-", "--dry-run"], COFFEE);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!env.db("new").exists());
}

#[test]
fn schema_is_json_and_opens_no_database() {
    let env = Env::new();
    let out = env.ok("a", &["import", "--schema"]);
    let schema: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(schema["required"], serde_json::json!(["field"]));
    assert!(!env.db("a").exists());
}
