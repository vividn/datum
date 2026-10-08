//! `datum db restore --hard` (da-rle8g): the database becomes exactly the
//! backup — its entries verbatim, no tombstones, no history. The real
//! binary; the database is read back with SQLite itself.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

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

    fn db(&self) -> PathBuf {
        self.tmp.path().join("data.db")
    }

    fn run(&self, args: &[&str], stdin: &[u8]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.db())
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn datum");
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        child.wait_with_output().unwrap()
    }

    fn ok(&self, args: &[&str], stdin: &[u8]) -> String {
        let out = self.run(args, stdin);
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn count(&self, sql: &str) -> i64 {
        rusqlite::Connection::open(self.db())
            .unwrap()
            .query_row(sql, [], |r| r.get(0))
            .unwrap()
    }

    /// Every row of `entries`, tombstones included, as (hid, data).
    fn rows(&self) -> Vec<Row> {
        let conn = rusqlite::Connection::open(self.db()).unwrap();
        let mut stmt = conn
            .prepare("SELECT hid, data FROM entries ORDER BY hid")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                let data: Option<String> = r.get(1)?;
                Ok((r.get(0)?, data.map(|d| serde_json::from_str(&d).unwrap())))
            })
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        rows
    }

    /// A backup, then a live entry and a tombstone it lacks.
    fn backed_up_then_changed(&self) -> String {
        self.ok(&["occur", "coffee", "n=1"], b"");
        self.ok(&["alias", "set", "cof", "occur coffee"], b"");
        let backup = self.ok(&["db", "backup", "-"], b"");
        self.ok(&["occur", "water"], b"");
        self.ok(&["occur", "juice"], b"");
        self.ok(&["del", "juice"], b"");
        // `juice` and the spec its only entry took with it (da-elv6d).
        assert_eq!(self.count(TOMBSTONES), 2);
        backup
    }
}

type Row = (String, Option<serde_json::Value>);

const TOMBSTONES: &str = "SELECT COUNT(*) FROM entries WHERE deleted_at IS NOT NULL";
const HISTORY: &str = "SELECT COUNT(*) FROM entry_history";

/// The backup's entries as (hid, data), in hid order.
fn backup_rows(backup: &str) -> Vec<Row> {
    let mut rows: Vec<_> = backup
        .lines()
        .skip(1)
        .map(|l| {
            let e: serde_json::Value = serde_json::from_str(l).unwrap();
            let data = (!e["data"].is_null()).then(|| e["data"].clone());
            (e["hid"].as_str().unwrap().to_string(), data)
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

#[test]
fn hard_restore_makes_the_database_exactly_the_backup() {
    let env = Env::new();
    let backup = env.backed_up_then_changed();
    assert!(env.count(HISTORY) > 0);

    env.ok(
        &["db", "restore", "--hard", "--yes", "-"],
        backup.as_bytes(),
    );

    assert_eq!(env.rows(), backup_rows(&backup));
    assert_eq!(env.count(TOMBSTONES), 0, "the tombstone went with the rest");
    assert_eq!(env.count(HISTORY), 0, "no history before the restore point");
    assert_eq!(
        env.ok(&["alias", "show", "cof"], b"").trim(),
        "cof → datum occur coffee",
        "the backup's alias is live"
    );
}

/// A sync conflict filed against the pre-restore rows goes with them: it
/// lives in `integrity_warnings` (the old `sync_conflicts` table, renamed
/// by `m25_to_26`), which the hard batch empties.
#[test]
fn hard_restore_leaves_no_conflict_about_a_dropped_entry() {
    let env = Env::new();
    let backup = env.backed_up_then_changed();
    let water = env.ok(&["db", "backup", "-"], b"");
    let water = water
        .lines()
        .find(|l| l.contains(r#""field":"water""#))
        .unwrap();
    rusqlite::Connection::open(env.db())
        .unwrap()
        .execute(
            "INSERT INTO integrity_warnings (id, hid, entry_slug, local_entry, \
             detected_at, detected_by, kind) \
             VALUES ('c0nflict00001', 'c0nfl', 'water:x', ?1, '2026-09-26T12:00:00Z', \
             'sync_pull', 'lww_overwrite')",
            [water],
        )
        .unwrap();
    assert!(env.ok(&["conflicts"], b"").contains("lww_overwrite"));

    env.ok(
        &["db", "restore", "--hard", "--yes", "-"],
        backup.as_bytes(),
    );

    assert!(env
        .ok(&["conflicts", "--all"], b"")
        .starts_with("No sync conflicts"));
    assert_eq!(env.count("SELECT COUNT(*) FROM integrity_warnings"), 0);
}

#[test]
fn hard_dry_run_counts_and_writes_nothing() {
    let env = Env::new();
    let backup = env.backed_up_then_changed();
    let before = (env.rows(), env.count(HISTORY));

    let out = env.ok(
        &["db", "restore", "--hard", "--dry-run", "-"],
        backup.as_bytes(),
    );

    assert!(
        out.contains("Would drop:") && out.contains("2 tombstoned or superseded"),
        "{out}"
    );
    assert_eq!((env.rows(), env.count(HISTORY)), before);
}

#[test]
fn hard_off_a_tty_without_yes_refuses_and_writes_nothing() {
    let env = Env::new();
    let backup = env.backed_up_then_changed();
    let before = env.rows();

    let out = env.run(&["db", "restore", "--hard", "-"], backup.as_bytes());

    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--yes"));
    assert_eq!(env.rows(), before);
}

#[test]
fn hard_refuses_merge_and_force_by_name() {
    let env = Env::new();
    for flag in ["--merge", "--force"] {
        let out = env.run(&["db", "restore", "--hard", flag, "x.jsonl"], b"");
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("--hard") && err.contains(flag), "{err}");
    }
}

/// A yes to a hard dry run's counts drops what they counted (da-h3yb3): a
/// write since — another tab's — refuses the run whole (exit 12, nothing
/// changed), and the fingerprint taken again goes through.
#[test]
fn hard_if_unchanged_refuses_a_write_since_the_count() {
    let env = Env::new();
    let backup = env.backed_up_then_changed();
    let fingerprint = |env: &Env| {
        let out = env.ok(
            &["db", "restore", "--hard", "--dry-run", "--json", "-"],
            backup.as_bytes(),
        );
        let out: serde_json::Value = serde_json::from_str(&out).unwrap();
        out["fingerprint"].as_str().unwrap().to_string()
    };
    let counted = fingerprint(&env);
    env.ok(&["occur", "tea"], b"");
    let before = (env.rows(), env.count(HISTORY));

    let hard = |fp: &str| {
        env.run(
            &[
                "db",
                "restore",
                "--hard",
                "--yes",
                "--if-unchanged",
                fp,
                "-",
            ],
            backup.as_bytes(),
        )
    };
    let out = hard(&counted);
    assert_eq!(out.status.code(), Some(12), "{out:?}");
    assert_eq!(
        (env.rows(), env.count(HISTORY)),
        before,
        "a refused run wrote"
    );

    assert!(hard(&fingerprint(&env)).status.success());
    assert_eq!(env.rows(), backup_rows(&backup));
}
