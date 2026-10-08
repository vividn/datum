//! da-hxmtg: `switch`/`start`/`end --if-changed` writes nothing when the
//! field already holds that state at the entry's time, and names the
//! entry that set it as ISLAST (exit 0). Subprocess, so the exit code and
//! the printed action are the real ones.

use std::process::Output;

use rusqlite::Connection;
use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.tmp.path().join("t.db"))
            .env("DATUM_HOME", self.tmp.path())
            .env("DATUM_CONFIG_HOME", self.tmp.path())
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum")
    }

    /// Run and return stdout, asserting exit 0.
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(
            out.status.code(),
            Some(0),
            "`datum {}`: stdout={stdout} stderr={}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        stdout
    }

    fn count(&self) -> i64 {
        Connection::open(self.tmp.path().join("t.db"))
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }
}

#[test]
fn same_state_twice_prints_islast_and_writes_nothing() {
    let env = Env::new();
    assert!(env
        .ok(&["switch", "mood", "happy", "--if-changed"])
        .contains("CREATE"));
    let n = env.count();
    assert!(env
        .ok(&["switch", "mood", "happy", "--if-changed"])
        .starts_with("ISLAST"));
    assert_eq!(env.count(), n, "ISLAST must not write");
}

#[test]
fn a_different_state_writes() {
    let env = Env::new();
    env.ok(&["switch", "mood", "happy"]);
    let n = env.count();
    assert!(env
        .ok(&["switch", "mood", "sad", "--if-changed"])
        .contains("CREATE"));
    assert_eq!(env.count(), n + 1);
}

#[test]
fn compares_with_the_state_governing_at_t() {
    let env = Env::new();
    env.ok(&["switch", "mood", "happy", "-t", "900"]);
    env.ok(&["switch", "mood", "sad", "-t", "1100"]);
    let n = env.count();
    // At 10:00 the field is happy, whatever the latest entry says.
    assert!(env
        .ok(&["switch", "mood", "happy", "--if-changed", "-t", "1000"])
        .starts_with("ISLAST"));
    assert!(env
        .ok(&["switch", "mood", "sad", "--if-changed", "-t", "1000"])
        .contains("CREATE"));
    assert_eq!(env.count(), n + 1);
}

#[test]
fn without_the_flag_a_duplicate_is_written() {
    let env = Env::new();
    env.ok(&["switch", "mood", "happy", "-t", "900"]);
    let n = env.count();
    assert!(env
        .ok(&["switch", "mood", "happy", "-t", "1000"])
        .contains("CREATE"));
    assert_eq!(env.count(), n + 1);
}

#[test]
fn start_while_open_and_end_while_closed_are_islast() {
    let env = Env::new();
    env.ok(&["start", "work", "-t", "900"]);
    let n = env.count();
    assert!(env
        .ok(&["start", "work", "--if-changed", "-t", "910"])
        .starts_with("ISLAST"));
    assert_eq!(env.count(), n);
    env.ok(&["end", "work", "-t", "920"]);
    let n = env.count();
    assert!(env
        .ok(&["end", "work", "--if-changed", "-t", "930"])
        .starts_with("ISLAST"));
    assert_eq!(env.count(), n);
}

#[test]
fn other_verbs_refuse_the_flag() {
    let env = Env::new();
    let out = env.run(&["occur", "coffee", "--if-changed"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--if-changed"));
}

#[test]
fn a_dry_run_previews_islast_and_leaves_the_cursor() {
    let env = Env::new();
    env.ok(&["switch", "mood", "happy", "-t", "900"]);
    let before = env.ok(&["occur", "coffee", "-t", "910"]);
    // An explicit -t, like the entries above: at now, before 09:00, the
    // happy entry would still be in the future and nothing would govern.
    let out = env.ok(&[
        "switch",
        "mood",
        "happy",
        "--if-changed",
        "--dry-run",
        "-t",
        "920",
    ]);
    assert!(out.starts_with("ISLAST"), "{out}");
    let cursor = env.ok(&["get", "_LAST"]);
    assert!(
        cursor.contains("coffee") && !cursor.contains("mood"),
        "`_LAST` must still be the coffee entry: {cursor} (before: {before})"
    );
}

#[test]
fn a_timeless_entry_refuses_the_flag() {
    let env = Env::new();
    env.ok(&["switch", "mood", "happy"]);
    let n = env.count();
    let out = env.run(&["switch", "mood", "happy", "-T", "--if-changed"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--if-changed"));
    assert_eq!(env.count(), n);
}
