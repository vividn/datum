//! `datum conflicts` through the real binary (da-ttsm, da-05rkv).
//!
//! Every case here spawns the actual `datum` binary and reads its stdout /
//! exit code. A conflict itself takes two copies and a sync, which
//! `sync_ops.rs` drives; here is what the command does around it, and how
//! `resolve` finds its entry, on the conflicts `debug conflicts` makes.
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
            // CI sets RUST_BACKTRACE, which puts a stack trace under an error.
            .env_remove("RUST_BACKTRACE")
            .env_remove("RUST_LIB_BACKTRACE")
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

    /// `debug conflicts 2`, then the short hids `conflicts` lists them by:
    /// the two live entries', and the deleted one's. None is a whole hid.
    fn debug_conflicts(&self) -> (Vec<String>, String) {
        assert!(self.run(&["debug", "conflicts", "2"]).0);
        let (_, list) = self.run(&["conflicts"]);
        let (deleted, live): (Vec<&str>, Vec<&str>) = list
            .lines()
            .filter(|l| l.contains("  debug:"))
            .partition(|l| l.ends_with("(deleted)"));
        let short = |l: &&str| l.split_whitespace().next().unwrap().to_string();
        let (live, deleted): (Vec<_>, Vec<_>) = (
            live.iter().map(short).collect(),
            deleted.iter().map(short).collect(),
        );
        assert_eq!((live.len(), deleted.len()), (2, 1), "{list}");
        let db = self.db();
        for hid in live.iter().chain(&deleted) {
            assert!(db.get_entry_including_deleted(hid).is_err(), "{hid}");
        }
        (live, deleted[0].clone())
    }

    /// The header lines of `conflicts`: one an entry with conflicts.
    fn conflicted(&self) -> Vec<String> {
        let (_, list) = self.run(&["conflicts"]);
        list.lines()
            .filter(|l| l.contains("  debug:"))
            .map(str::to_string)
            .collect()
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
fn resolve_takes_the_short_hid_tail_prints() {
    // da-q4x3c: an exact-hid lookup refused the hid every listing prints.
    let env = Env::new();
    let (live, _) = env.debug_conflicts();
    let (_, tail) = env.run(&["tail"]);
    assert!(tail.contains(&live[0]), "{tail}");

    let (ok, out) = env.run(&["conflicts", "resolve", &live[0]]);
    assert!(ok, "{out}");
    assert!(out.ends_with(&format!("({})\n", live[0])), "{out}");
    let left = env.conflicted();
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(!left.iter().any(|l| l.starts_with(&live[0])), "{left:?}");
}

#[test]
fn resolve_takes_the_short_hid_of_a_deleted_entry() {
    // A delete can be one side of a conflict, and `tail` never shows it.
    let env = Env::new();
    let (_, deleted) = env.debug_conflicts();
    let (ok, out) = env.run(&["conflicts", "resolve", &deleted]);
    assert!(ok, "{out}");
    assert!(out.ends_with(&format!("({deleted})\n")), "{out}");
    let left = env.conflicted();
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(!left.iter().any(|l| l.contains("(deleted)")), "{left:?}");
}

#[test]
fn resolve_refuses_an_ambiguous_prefix_as_update_does() {
    let env = Env::new();
    env.debug_conflicts();
    let (ok, out) = env.run(&["conflicts", "resolve", "debug:"]);
    assert!(!ok, "{out}");
    assert!(out.contains("'debug:' is ambiguous"), "{out}");
    assert_eq!(out, env.run(&["update", "debug:", "note=y"]).1);
    assert_eq!(env.conflicted().len(), 3, "nothing was resolved");
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
