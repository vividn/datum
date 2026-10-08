//! `datum conflicts` through the real binary (da-ttsm, da-05rkv).
//!
//! Every case here spawns the actual `datum` binary and reads its stdout /
//! exit code. A conflict itself takes two copies and a sync, which
//! `sync_ops.rs` drives; here is what the command does around it.
//!
//! Scope note: `conflicts` lists what the sync fold leaves contested in
//! the entries' `conflicts` column, not the integrity queue. da-z46t
//! removed the queue-wide `list/resolve/auto-resolve` surface deliberately
//! — integrity findings resolve by fixing the data — and
//! `auto_resolve_is_not_a_conflicts_subcommand` keeps it removed.

use std::path::PathBuf;
use std::process::Command;

use datum_core::db::Db;
use datum_core::integrity::{IntegrityKind, ListFilter};
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    path: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let path = tmp.path().join("conflicts.db");
        // Materialize the schema before the binary runs so seeding and
        // the CLI look at the same file.
        drop(Db::open(path.to_str().unwrap()).expect("open db"));
        Self { _tmp: tmp, path }
    }

    fn db(&self) -> Db {
        Db::open(self.path.to_str().unwrap()).expect("reopen db")
    }

    /// Run `datum <args...>` against this env's db. Returns (ok, stdout+stderr).
    fn run(&self, args: &[&str]) -> (bool, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.path)
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    fn double_open(&self) -> String {
        assert!(self.run(&["start", "work", "-t=-1"]).0);
        assert!(self.run(&["start", "work"]).0);
        self.db()
            .list_integrity_warnings(&ListFilter {
                kind: Some(IntegrityKind::DoubleOpen),
                pending: Some(true),
            })
            .expect("list")
            .first()
            .expect("double_open filed")
            .hid
            .clone()
    }
}

#[test]
fn bare_conflicts_says_there_are_none() {
    let env = Env::new();
    // Bare `datum conflicts` is `conflicts list`.
    let (ok, out) = env.run(&["conflicts"]);
    assert!(ok, "datum conflicts failed: {out}");
    assert!(out.contains("No sync conflicts"), "{out}");
}

#[test]
fn list_leaves_integrity_findings_to_check() {
    let env = Env::new();
    let double_open = env.double_open();
    let (ok, out) = env.run(&["conflicts", "list"]);
    assert!(ok, "conflicts list failed: {out}");
    assert!(
        !out.contains(&double_open) && !out.contains("double_open"),
        "double_open is `datum check`'s to report, not conflicts':\n{out}"
    );
    assert!(out.contains("No sync conflicts"), "{out}");
}

#[test]
fn resolve_refuses_an_entry_with_no_conflicts() {
    let env = Env::new();
    env.double_open();
    let hid: String = env
        .db()
        .conn()
        .query_row(
            "SELECT hid FROM entries WHERE field = 'work' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let (ok, out) = env.run(&["conflicts", "resolve", &hid]);
    assert!(!ok, "nothing to resolve, and nothing may be written: {out}");
    assert!(out.contains("has no conflicts"), "{out}");
}

#[test]
fn resolve_takes_one_entry_then_picks() {
    let env = Env::new();
    let (ok, out) = env.run(&["conflicts", "resolve", "a", "b"]);
    assert!(!ok, "{out}");
    assert!(out.contains("one entry, then KEY=VALUE picks"), "{out}");
}

#[test]
fn auto_resolve_is_not_a_conflicts_subcommand() {
    // da-z46t removed the auto-resolver's CLI lever along with the rest of
    // the queue-wide surface; the double_open rule stays internal. If this
    // starts passing, that decision has been quietly reversed.
    let env = Env::new();
    let (ok, out) = env.run(&["conflicts", "auto-resolve"]);
    assert!(!ok, "conflicts auto-resolve should not exist: {out}");
    assert!(
        out.contains("unknown conflicts subcommand"),
        "expected the subcommand guard, got:\n{out}"
    );
}
