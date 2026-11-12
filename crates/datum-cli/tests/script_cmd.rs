//! `datum script FILE` and `datum < FILE` (da-ygfeh): a script of command lines,
//! stopping at the first that fails with its line number and exit 1, going on
//! under `--allow-partial`, all or nothing under `--atomic`, written nowhere
//! under `--dry-run`, and every «now» one instant under `--now`.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use rusqlite::Connection;
use tempfile::TempDir;

struct Env {
    tmp: TempDir,
    home: PathBuf,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let db = tmp.path().join("s.db");
        Self { tmp, home, db }
    }

    /// `datum <args>` with `stdin`.
    fn datum(&self, args: &[&str], stdin: &str) -> Output {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(args)
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("NO_COLOR", "1")
            // A backtrace's frames differ between two runs, as CI prints them.
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

    /// `datum script <lines> <flags>`, the lines written to a file.
    fn script(&self, lines: &[&str], flags: &[&str]) -> Output {
        let file = self.tmp.path().join("script.txt");
        std::fs::write(&file, lines.join("\n")).unwrap();
        let file = file.to_string_lossy().into_owned();
        self.datum(&[&["script", file.as_str()][..], flags].concat(), "")
    }

    /// Each live entry's field, in the order they were written.
    fn fields(&self) -> Vec<String> {
        if !self.db.exists() {
            return Vec::new();
        }
        let c = Connection::open(&self.db).unwrap();
        let mut s = c
            .prepare(
                "SELECT field FROM entries WHERE deleted_at IS NULL
                 AND field NOT IN ('spec', 'alias') ORDER BY create_time, rowid",
            )
            .unwrap();
        let rows = s.query_map([], |r| r.get(0)).unwrap();
        rows.collect::<rusqlite::Result<_>>().unwrap()
    }

    fn query(&self, sql: &str) -> Vec<String> {
        let c = Connection::open(&self.db).unwrap();
        let mut s = c.prepare(sql).unwrap();
        let rows = s.query_map([], |r| r.get(0)).unwrap();
        rows.collect::<rusqlite::Result<_>>().unwrap()
    }
}

const BAD: &[&str] = &[
    "moment coffee -y -t 9:00",
    "# a comment, and a blank line, are no lines that run",
    "",
    "moment tea -y -t 'not a time'",
    "moment cake -y -t 10:00",
];

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_bad_line_stops_the_script_there_with_its_number_and_exit_1() {
    let env = Env::new();
    let out = env.script(BAD, &[]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("error: line 4: "), "{err}");
    assert!(err.contains("stopped at line 4"), "{err}");
    assert_eq!(env.fields(), ["coffee"]);
}

#[test]
fn allow_partial_runs_the_lines_after_a_bad_one_and_still_exits_1() {
    let env = Env::new();
    let out = env.script(BAD, &["--allow-partial"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("1 of 3 lines failed: 4"),
        "{}",
        stderr(&out)
    );
    assert_eq!(env.fields(), ["coffee", "cake"]);
}

#[test]
fn atomic_leaves_nothing_after_a_failure() {
    let env = Env::new();
    env.script(&["moment seed -yy -t 8:00"], &[]);
    let out = env.script(BAD, &["--atomic"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("nothing was written"),
        "{}",
        stderr(&out)
    );
    assert_eq!(env.fields(), ["seed"]);
}

#[test]
fn an_atomic_script_is_one_undo() {
    let env = Env::new();
    env.script(&["moment seed -yy -t 8:00"], &[]);
    let lines = [
        "moment coffee -y -t 9:00",
        "start walk -y -t 10:00",
        "moment cake -y -t 11:00",
    ];
    let out = env.script(&lines, &["--atomic"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(env.fields(), ["seed", "coffee", "walk", "cake"]);
    assert!(
        !env.tmp.path().join("s.db.spool").exists(),
        "the script did not land"
    );
    let out = env.datum(&["undo"], "");
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(env.fields(), ["seed"]);
}

#[test]
fn atomic_refuses_a_line_one_write_cannot_carry() {
    let env = Env::new();
    let out = env.script(
        &["moment coffee -y -t 9:00", "db config auto_setup false"],
        &["--atomic"],
    );
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("error: line 2: an --atomic script"),
        "{}",
        stderr(&out)
    );
    assert_eq!(env.fields(), Vec::<String>::new());
}

#[test]
fn a_dry_run_writes_nothing() {
    let env = Env::new();
    env.script(&["moment seed -yy -t 8:00"], &[]);
    let out = env.script(&["moment coffee -y -t 9:00"], &["--dry-run"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("DRYRUN"));
    // In front of `script`, the flag is still the script's.
    let out = env.datum(&["--dry-run", "script", "-"], "moment tea -y -t 9:00\n");
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("DRYRUN"));
    assert_eq!(env.fields(), ["seed"]);
}

#[test]
fn now_makes_every_bare_time_one_instant() {
    let env = Env::new();
    let lines = ["moment a", "start b", "moment c -t -5"];
    let out = env.script(&lines, &["--now", "2026-01-05T09:00:00Z"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let at = |f: &str| env.query(&format!("SELECT at FROM entries WHERE field = '{f}'"));
    assert_eq!(at("a"), ["2026-01-05T09:00:00+00:00"]);
    assert_eq!(at("b"), at("a"));
    assert_eq!(at("c"), ["2026-01-05T08:55:00+00:00"]);
}

#[test]
fn tz_is_the_zone_every_line_reads_its_times_in() {
    let env = Env::new();
    let out = env.script(&["moment a -y -t 9:00"], &["--tz", "Asia/Tokyo"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let zone = env.query("SELECT at_tz FROM entries WHERE field = 'a'");
    assert_eq!(zone, ["Asia/Tokyo"]);
}

/// `datum < FILE` is `datum script FILE`: the same entries, the same words.
#[test]
fn stdin_and_a_file_agree() {
    let (piped, filed) = (Env::new(), Env::new());
    let a = piped.datum(&[], &BAD.join("\n"));
    let b = filed.script(BAD, &[]);
    assert_eq!(a.status.code(), b.status.code());
    assert_eq!(stderr(&a), stderr(&b));
    let words = |o: &Output| -> Vec<String> {
        let out = String::from_utf8_lossy(&o.stdout);
        out.lines()
            .map(|l| l.split(' ').next().unwrap().to_string())
            .collect()
    };
    assert_eq!(words(&a), words(&b));
    assert_eq!(piped.fields(), filed.fields());
}
