//! `datum debug` through the real binary (da-4dvk7): a db full of sync
//! conflicts made by a debug device, the ops that decide them, and the
//! refusals that keep that device's ops from ever travelling.

use std::path::PathBuf;
use std::process::Command;

use datum_core::db::Db;
use serde_json::json;
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    path: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let path = tmp.path().join("debug.db");
        drop(Db::open(path.to_str().unwrap()).expect("open db"));
        Self { _tmp: tmp, path }
    }

    fn db(&self) -> Db {
        Db::open(self.path.to_str().unwrap()).expect("reopen db")
    }

    /// `datum <args...>` against this env's db: (ok, stdout+stderr).
    fn run(&self, args: &[&str]) -> (bool, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.path)
            .env("DATUM_COLOR", "0")
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .args(args)
            .output()
            .expect("spawn datum");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    fn conflicts(&self) -> String {
        let (ok, out) = self.run(&["conflicts"]);
        assert!(ok, "{out}");
        out
    }
}

#[test]
fn debug_conflicts_files_n_conflicts_and_the_delete_case() {
    let env = Env::new();
    let (ok, out) = env.run(&["debug", "conflicts", "3"]);
    assert!(ok, "{out}");
    assert!(out.starts_with("4 sync conflicts filed"), "{out}");

    let list = env.conflicts();
    assert_eq!(list.matches("sync_conflict").count(), 4, "{list}");
    assert_eq!(list.matches(r#"state="edited here""#).count(), 3, "{list}");
    // The delete stands: three entries are left to see.
    let (_, tail) = env.run(&["tail"]);
    assert_eq!(tail.matches("edited there").count(), 3, "{tail}");
}

#[test]
fn debug_ops_shows_two_edits_that_saw_only_the_create() {
    let env = Env::new();
    let (_, out) = env.run(&["debug", "conflicts", "1"]);
    let hid = out
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    let (ok, ops) = env.run(&["debug", "ops", hid]);
    assert!(ok, "{ops}");
    let lines: Vec<&str> = ops.lines().skip(1).collect();
    assert_eq!(lines.len(), 3, "{ops}");
    let mine = lines[0].split_whitespace().nth(1).unwrap();
    assert!(lines[0].contains("(this copy)  create"), "{ops}");
    let dep = format!("deps {{{mine}}}");
    assert!(
        lines[1].contains("(this copy)") && lines[1].contains(&dep),
        "{ops}"
    );
    assert!(
        !lines[2].contains("(this copy)") && lines[2].contains(&dep),
        "{ops}"
    );
    assert!(lines[2].contains(r#"state="edited there""#), "{ops}");
}

#[test]
fn a_copy_holding_debug_ops_does_not_sync() {
    let env = Env::new();
    env.run(&["debug", "conflicts", "1"]);
    for argv in [
        &["sync", "pipe", "ops"][..],
        &["sync"],
        &["sync", "run"],
        &["sync", "chain", "invite"],
        &["sync", "chain", "connect", "nonsense"],
    ] {
        let (ok, out) = env.run(argv);
        assert!(!ok && out.contains("debug device"), "{argv:?}: {out}");
    }
    let (ok, out) = env.run(&["sync", "status"]);
    assert!(ok && out.contains("debug device"), "{out}");
    let (_, out) = env.run(&["sync", "status", "--show", "json"]);
    assert!(
        out.contains(r#""chain":null"#),
        "the invite started no chain: {out}"
    );
    let (ok, out) = env.run(&["debug", "conflicts", "1"]);
    assert!(ok, "a second run still applies its own device's ops: {out}");
}

#[test]
fn a_copy_in_a_chain_is_refused_and_nothing_is_written() {
    let env = Env::new();
    let genesis = json!({"type": "genesis", "chain": "c1", "admins": [], "label": "a"});
    env.db().sync_chain_write(genesis, true).unwrap();
    let (ok, out) = env.run(&["debug", "conflicts", "2"]);
    assert!(!ok && out.contains("sync chain c1"), "{out}");
    let (_, tail) = env.run(&["tail"]);
    assert!(!tail.contains("debug"), "{tail}");
    assert!(env.conflicts().starts_with("No pending"));
}

#[test]
fn a_database_holding_real_entries_is_refused_and_nothing_is_written() {
    let env = Env::new();
    env.run(&["occur", "coffee"]);
    let (ok, out) = env.run(&["debug", "conflicts", "2"]);
    assert!(
        !ok && out.contains("1 entry `datum debug` did not make (coffee"),
        "{out}"
    );
    let (_, tail) = env.run(&["tail"]);
    assert!(!tail.contains("debug"), "{tail}");
    let (ok, out) = env.run(&["sync", "status"]);
    assert!(ok && !out.contains("debug device"), "{out}");
}

/// What is not anyone's data does not count: a spec, and DEBUG's sample
/// seeder's entries, which carry `sample=true` (web/src/lib/sample.ts).
#[test]
fn specs_and_sample_entries_are_not_real_entries() {
    let env = Env::new();
    env.run(&["spec", "set", "mood", "--verb", "switch"]);
    env.run(&["occur", "coffee", "sample=true"]);
    let (ok, out) = env.run(&["debug", "conflicts", "1"]);
    assert!(ok, "{out}");
}
