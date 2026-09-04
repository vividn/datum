//! `datum conflicts` through the real binary (da-ttsm).
//!
//! The bead this file closes was not "a command is missing" so much as
//! "nobody would have noticed": the conflict tests in `integrity_cmd.rs`
//! are named after CLI subcommands (`conflicts_list_shows_only_...`) but
//! call `db.list_integrity_warnings` directly, so they stayed green for a
//! month while `datum conflicts` answered `unknown subcommand`. Every case
//! here spawns the actual `datum` binary and reads its stdout / exit code.
//!
//! Scope note: `conflicts` covers the two kinds a sync pull files
//! (`lww_overwrite`, `identity_collision`), not the whole integrity queue.
//! da-z46t removed the queue-wide `list/resolve/auto-resolve` surface
//! deliberately — integrity findings resolve by fixing the data — and
//! `auto_resolve_is_not_a_conflicts_subcommand` keeps it removed.

use std::path::PathBuf;
use std::process::Command;

use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::integrity::{IntegrityKind, ListFilter};
use datum_core::test_util::entry_builder;
use serde_json::json;
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
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .expect("spawn datum");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    /// Seed the shape a sync pull leaves behind when a newer remote edit
    /// overwrites a local one: the winner is the row in `entries`, the
    /// loser survives only inside the warning. Returns (warning hid,
    /// entry slug).
    fn seed_lww(&self, field: &str) -> (String, String) {
        let db = self.db();
        let winner: Entry = entry_builder(field).state(json!("remote")).insert(&db);
        let mut loser = winner.clone();
        loser.state = Some(json!("local"));
        let w = datum_core::integrity::file_lww_overwrite(db.conn(), &loser, &winner)
            .expect("file lww_overwrite");
        (w.hid, winner.slug.to_string())
    }

    fn warning_resolution(&self, hid: &str) -> Option<String> {
        self.db()
            .get_integrity_warning(hid)
            .expect("get warning")
            .expect("warning exists")
            .resolution
    }

    fn state_of(&self, slug: &str) -> Option<serde_json::Value> {
        self.db().get_entry(slug).expect("entry exists").state
    }
}

#[test]
fn bare_conflicts_lists_the_queue_with_both_sides() {
    let env = Env::new();
    let (hid, _) = env.seed_lww("mood");

    // Bare `datum conflicts` is `conflicts list` — the phantom this bead
    // was filed for was exactly this invocation.
    let (ok, out) = env.run(&["conflicts"]);
    assert!(ok, "datum conflicts failed: {out}");
    assert!(out.contains(&hid), "row for {hid} missing from:\n{out}");
    assert!(out.contains("lww_overwrite"), "kind missing from:\n{out}");
    // Both versions print, because picking a side blind is not a choice.
    assert!(out.contains("\"local\""), "local side missing from:\n{out}");
    assert!(
        out.contains("\"remote\""),
        "remote side missing from:\n{out}"
    );
}

#[test]
fn list_leaves_integrity_findings_to_check() {
    let env = Env::new();
    // A double_open filed the ordinary way: two starts on one field.
    assert!(env.run(&["start", "work", "-t=-1"]).0);
    assert!(env.run(&["start", "work"]).0);
    let double_open = env
        .db()
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
            pending: Some(true),
        })
        .expect("list")
        .first()
        .expect("double_open filed")
        .hid
        .clone();

    let (ok, out) = env.run(&["conflicts", "list"]);
    assert!(ok, "conflicts list failed: {out}");
    assert!(
        !out.contains(&double_open) && !out.contains("double_open"),
        "double_open is `datum check`'s to report, not conflicts':\n{out}"
    );
    assert!(
        out.contains("No pending sync conflicts"),
        "expected the empty-queue line, got:\n{out}"
    );
}

#[test]
fn take_local_restores_the_overwritten_version() {
    let env = Env::new();
    let (hid, slug) = env.seed_lww("mood");
    assert_eq!(env.state_of(&slug), Some(json!("remote")));

    let (ok, out) = env.run(&["conflicts", "resolve", &hid, "--take-local"]);
    assert!(ok, "resolve --take-local failed: {out}");
    assert_eq!(env.warning_resolution(&hid).as_deref(), Some("take_local"));
    assert_eq!(
        env.state_of(&slug),
        Some(json!("local")),
        "the local edit the pull overwrote must be back in `entries`"
    );
}

#[test]
fn take_remote_keeps_the_pulled_version() {
    let env = Env::new();
    let (hid, slug) = env.seed_lww("mood");

    let (ok, out) = env.run(&["conflicts", "resolve", &hid, "--take-remote"]);
    assert!(ok, "resolve --take-remote failed: {out}");
    assert_eq!(env.warning_resolution(&hid).as_deref(), Some("take_remote"));
    assert_eq!(env.state_of(&slug), Some(json!("remote")));
}

#[test]
fn keep_both_records_the_decision_without_touching_entries() {
    let env = Env::new();
    let (hid, slug) = env.seed_lww("mood");

    let (ok, out) = env.run(&["conflicts", "resolve", &hid, "--keep-both"]);
    assert!(ok, "resolve --keep-both failed: {out}");
    assert_eq!(env.warning_resolution(&hid).as_deref(), Some("keep_both"));
    assert_eq!(env.state_of(&slug), Some(json!("remote")));

    // Resolved rows leave the default view and return under --all.
    let (_, pending) = env.run(&["conflicts"]);
    assert!(
        !pending.contains(&hid),
        "resolved row still pending:\n{pending}"
    );
    let (ok, all) = env.run(&["conflicts", "list", "--all"]);
    assert!(ok, "conflicts list --all failed: {all}");
    assert!(
        all.contains(&hid),
        "--all must include resolved rows:\n{all}"
    );
    assert!(all.contains("keep_both"), "resolution missing from:\n{all}");
}

#[test]
fn resolve_refuses_an_integrity_finding() {
    let env = Env::new();
    assert!(env.run(&["start", "work", "-t=-1"]).0);
    assert!(env.run(&["start", "work"]).0);
    let w = env
        .db()
        .list_integrity_warnings(&ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
            pending: Some(true),
        })
        .expect("list")
        .first()
        .expect("double_open filed")
        .hid
        .clone();

    let (ok, out) = env.run(&["conflicts", "resolve", &w, "--take-local"]);
    assert!(!ok, "picking a side must not resolve a double_open: {out}");
    assert!(
        out.contains("datum check"),
        "the error should point at where the finding lives:\n{out}"
    );
    assert_eq!(
        env.warning_resolution(&w),
        None,
        "the finding must stay pending"
    );
}

#[test]
fn resolve_requires_an_explicit_side() {
    let env = Env::new();
    let (hid, _) = env.seed_lww("mood");

    let (ok, out) = env.run(&["conflicts", "resolve", &hid]);
    assert!(!ok, "a side-less resolve must not guess: {out}");
    assert!(
        out.contains("--take-local"),
        "error should name the flags:\n{out}"
    );
    assert_eq!(env.warning_resolution(&hid), None);
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
