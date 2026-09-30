//! Easy Sync's core (da-k329q): two copies of a database, each with its own
//! device, converge by piping ops between them — the real binary, ops as
//! JSON lines on stdout and stdin.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

/// One copy of a database: its own file and its own `DATUM_HOME`, so its own
/// client id and endpoint.
struct Copy {
    tmp: TempDir,
}

impl Copy {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
        }
    }

    fn db(&self) -> PathBuf {
        self.tmp.path().join("data.db")
    }

    fn run(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.db())
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("DATUM_COLOR", "0")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn datum");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        self.ok_in(args, "")
    }

    fn ok_in(&self, args: &[&str], stdin: &str) -> String {
        let out = self.run(args, stdin);
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// The ops this copy has that `other` lacks.
    fn ops_for(&self, other: &Copy) -> String {
        let vector = other.ok(&["sync", "pipe", "vector"]);
        self.ok(&["sync", "pipe", "ops", "--since", vector.trim()])
    }

    /// Send `other` what it lacks; the apply summary.
    fn send(&self, other: &Copy) -> String {
        other.ok_in(&["sync", "pipe", "apply"], &self.ops_for(other))
    }

    fn hash(&self) -> String {
        self.ok(&["sync", "pipe", "verify"]).trim().to_string()
    }

    fn ops(&self) -> Vec<Value> {
        self.ok(&["sync", "pipe", "ops"])
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// The hid of the newest entry of `field`.
    fn hid(&self, field: &str) -> String {
        self.query(
            "SELECT hid FROM entries WHERE field = ?1 ORDER BY create_time DESC LIMIT 1",
            field,
        )
    }

    /// Entry `hid`'s `data`.
    fn data(&self, hid: &str) -> Value {
        serde_json::from_str(&self.query("SELECT data FROM entries WHERE hid = ?1", hid)).unwrap()
    }

    fn deleted(&self, hid: &str) -> bool {
        self.query(
            "SELECT deleted_at IS NOT NULL FROM entries WHERE hid = ?1",
            hid,
        ) == "1"
    }

    fn query(&self, sql: &str, arg: &str) -> String {
        rusqlite::Connection::open(self.db())
            .unwrap()
            .query_row(sql, [arg], |r| {
                let v: rusqlite::types::Value = r.get(0)?;
                Ok(match v {
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    other => format!("{other:?}"),
                })
            })
            .unwrap()
    }

    fn sql(&self, sql: &str) {
        rusqlite::Connection::open(self.db())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }
}

fn converged(a: &Copy, b: &Copy) {
    a.send(b);
    b.send(a);
    assert_eq!(a.hash(), b.hash(), "the two copies hold different data");
}

/// The single op a command adds, asserting that it adds exactly one.
fn one_op(copy: &Copy, args: &[&str]) -> Value {
    let before = copy.ops().len();
    copy.ok(args);
    let ops = copy.ops();
    assert_eq!(
        ops.len(),
        before + 1,
        "`datum {}` must emit exactly one op",
        args.join(" ")
    );
    let op = ops
        .into_iter()
        .max_by_key(|o| o["hlc"].as_str().unwrap().to_string())
        .unwrap();
    assert_eq!(
        op["raw"],
        args.join(" "),
        "the op carries its own command line"
    );
    op
}

#[test]
fn two_copies_converge_over_a_pipe() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.ok(&["switch", "mood", "happy"]);
    assert!(a.send(&b).starts_with("applied 4 "));
    assert_eq!(a.hash(), b.hash());

    a.ok(&["occur", "tea"]);
    b.ok(&["switch", "mood", "calm"]);
    converged(&a, &b);
    assert_eq!(
        b.ok(&["sync", "pipe", "vector"]),
        a.ok(&["sync", "pipe", "vector"])
    );
}

#[test]
fn every_change_is_one_op_with_its_own_line() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "cups=2"]);
    let hid = a.hid("coffee");
    let first = a
        .ops()
        .into_iter()
        .find(|o| o["hid"] == hid.as_str())
        .unwrap();
    assert_eq!(first["kind"], "create");
    assert_eq!(first["raw"], "occur coffee cups=2");
    assert_eq!(first["effect"]["create"]["data"]["cups"], 2);
    // A second entry, so the delete below does not empty the field and take
    // its spec too, in a second op (da-elv6d).
    a.ok(&["occur", "coffee", "-t", "1000"]);

    let op = one_op(&a, &["update", &hid, "cups=3"]);
    assert_eq!(op["kind"], "update");
    assert_eq!(op["effect"]["set"]["data.cups"], 3);
    assert!(
        op["deps"].as_object().unwrap().len() == 1,
        "it saw the create"
    );

    let op = one_op(&a, &["merge", &hid, "size=big"]);
    assert_eq!(op["effect"]["set"]["data.size"], "big");
    assert_eq!(
        op["effect"]["set"].get("data.cups"),
        None,
        "only what changed"
    );

    let op = one_op(&a, &["retime", &hid, "-q"]);
    assert_eq!(op["kind"], "retime");
    assert!(op["effect"]["set"]["occur_time"].is_object());

    let op = one_op(&a, &["delete", &hid]);
    assert_eq!(
        (op["kind"].as_str(), op["effect"]["deleted"].as_bool()),
        (Some("delete"), Some(true))
    );

    let op = one_op(&a, &["undo", &hid]);
    assert_eq!(
        (op["kind"].as_str(), op["effect"]["deleted"].as_bool()),
        (Some("undo"), Some(false))
    );

    let op = one_op(&a, &["redo", &hid]);
    assert_eq!(op["kind"], "redo");

    let op = one_op(&a, &["undo", &hid]);
    assert_eq!(op["kind"], "undo");

    one_op(&a, &["rekey", &hid, "size=small"]);
}

#[test]
fn concurrent_edits_of_different_keys_both_land() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=3"]);
    b.ok(&["merge", &hid, "size=big"]);
    converged(&a, &b);
    let data = &a.data(&hid);
    assert_eq!(
        (data["cups"].as_i64(), data["size"].as_str()),
        (Some(3), Some("big"))
    );
    assert!(!a.ok(&["check"]).contains("sync_conflict"));
}

#[test]
fn concurrent_edits_of_one_key_go_to_the_later_and_file_a_conflict() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=4"]);
    b.ok(&["update", &hid, "cups=5"]);
    converged(&a, &b);
    // The later HLC wins; read which that is rather than assume it.
    let last = |c: &Copy| {
        c.ops()
            .into_iter()
            .filter(|o| o["kind"] == "update")
            .max_by_key(|o| {
                (
                    o["hlc"].as_str().unwrap().to_string(),
                    o["endpoint"].as_str().unwrap().to_string(),
                )
            })
    };
    let winner = last(&a).unwrap();
    assert_eq!(winner["endpoint"], last(&b).unwrap()["endpoint"]);
    let won = if winner["raw"].as_str().unwrap().ends_with("cups=5") {
        5
    } else {
        4
    };
    assert_eq!(a.data(&hid)["cups"], won);
    for copy in [&a, &b] {
        let check = copy.run(&["check"], "");
        let out = String::from_utf8_lossy(&check.stdout);
        assert!(out.contains("integrity_sync_conflict"), "{out}");
        assert!(
            out.contains(&format!(
                "`update {hid} cups=4` lost to `update {hid} cups=5`"
            )),
            "{out}"
        );
        let conflicts = copy.ok(&["conflicts"]);
        assert!(conflicts.contains("sync_conflict"), "{conflicts}");
    }
}

#[test]
fn a_delete_stands_against_a_concurrent_edit() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    b.ok(&["update", &hid, "cups=9"]);
    a.ok(&["delete", &hid]);
    converged(&a, &b);
    for copy in [&a, &b] {
        assert!(copy.deleted(&hid), "the entry stays deleted");
        let conflicts = copy.ok(&["conflicts"]);
        assert!(conflicts.contains("sync_conflict"), "{conflicts}");
        assert!(
            conflicts.contains(&format!("by `update {hid} cups=9`")),
            "{conflicts}"
        );
    }
}

#[test]
fn two_creates_of_one_slug_keep_both_and_supersede_one() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&[
        "occur",
        "coffee",
        "-d",
        "2026-03-16",
        "-t",
        "1200",
        "cups=1",
    ]);
    b.ok(&[
        "occur",
        "coffee",
        "-d",
        "2026-03-16",
        "-t",
        "1200",
        "cups=2",
    ]);
    converged(&a, &b);
    let conflicts = a.ok(&["conflicts"]);
    assert!(conflicts.contains("identity_collision"), "{conflicts}");
    // Both copies declared `coffee`'s spec: one event twice, no warning.
    assert_eq!(
        conflicts.matches("identity_collision").count(),
        1,
        "{conflicts}"
    );
}

#[test]
fn the_same_ops_twice_or_in_any_order_give_the_same_state() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "cups=2"]);
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=3"]);
    a.ok(&["switch", "mood", "happy"]);
    a.ok(&["delete", &hid]);
    let ops = a.ok(&["sync", "pipe", "ops"]);

    let twice = Copy::new();
    twice.ok_in(&["sync", "pipe", "apply"], &ops);
    let again = twice.ok_in(&["sync", "pipe", "apply"], &ops);
    // Seven: the delete of `coffee`'s only entry takes its spec (da-elv6d).
    assert!(again.starts_with("applied 0 · already had 7 "), "{again}");
    assert_eq!(twice.hash(), a.hash());

    let reversed: String = ops.lines().rev().map(|l| format!("{l}\n")).collect();
    let shuffled = Copy::new();
    let summary = shuffled.ok_in(&["sync", "pipe", "apply"], &reversed);
    assert!(
        summary.starts_with("applied 7 · already had 0 · held 0 "),
        "{summary}"
    );
    assert_eq!(shuffled.hash(), a.hash());
}

#[test]
fn a_tampered_op_is_refused_and_a_gap_is_held_until_filled() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "cups=2"]);
    let ops = a.ok(&["sync", "pipe", "ops"]);
    let lines: Vec<&str> = ops.lines().collect();
    assert_eq!(lines.len(), 2);

    let b = Copy::new();
    let tampered = lines[0].replace("\"cups\":2", "\"cups\":9");
    assert_ne!(tampered, lines[0]);
    let out = b.run(&["sync", "pipe", "apply"], &tampered);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("refused") && err.contains("bad signature"),
        "{err}"
    );
    assert!(
        b.ok(&["sync", "pipe", "ops"]).is_empty(),
        "nothing of it was kept"
    );

    let held = b.ok_in(&["sync", "pipe", "apply"], lines[1]);
    assert!(
        held.starts_with("applied 0 · already had 0 · held 1 "),
        "{held}"
    );
    let filled = b.ok_in(&["sync", "pipe", "apply"], lines[0]);
    assert!(
        filled.starts_with("applied 2 · already had 0 · held 0 "),
        "{filled}"
    );
    assert_eq!(b.hash(), a.hash());
}

#[test]
fn entries_from_before_ops_sync_into_an_empty_copy() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "cups=2"]);
    a.ok(&["switch", "mood", "happy"]);
    // As a database written before this bead: entries, and no ops at all.
    a.sql("DELETE FROM ops; DELETE FROM sync_local;");
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=3"]);
    let kinds: Vec<String> = a
        .ops()
        .iter()
        .map(|o| o["kind"].as_str().unwrap().into())
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| *k == "baseline").count(),
        4,
        "{kinds:?}"
    );
    assert!(kinds.contains(&"update".to_string()));

    let empty = Copy::new();
    assert!(a.send(&empty).starts_with("applied 5 "));
    assert_eq!(empty.hash(), a.hash());
}

#[test]
fn a_copied_file_on_another_device_mints_its_own_endpoint() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    let c = Copy::new();
    std::fs::copy(a.db(), c.db()).unwrap();
    c.ok(&["occur", "tea"]);
    let endpoints: std::collections::BTreeSet<String> = c
        .ops()
        .iter()
        .map(|o| o["endpoint"].as_str().unwrap().into())
        .collect();
    assert_eq!(
        endpoints.len(),
        2,
        "the copy signs as itself: {endpoints:?}"
    );
}

#[test]
fn ops_since_a_vector_sends_only_what_it_lacks() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee"]);
    a.send(&b);
    // Only a's own op at b's seq for a, which b holds: the one line that
    // would show a copy sharing a's key.
    let overlap = a.ops_for(&b);
    assert_eq!(overlap.lines().count(), 1, "{overlap}");
    let had = b.ok_in(&["sync", "pipe", "apply"], &overlap);
    assert!(had.starts_with("applied 0 · already had 1 "), "{had}");
    a.ok(&["occur", "coffee", "cups=2"]);
    let lacked = a.ops_for(&b);
    assert_eq!(lacked.lines().count(), 2, "{lacked}");
    assert!(lacked.starts_with(&overlap));
    assert!(lacked.contains("\"raw\":\"occur coffee cups=2\""));
}

#[test]
fn a_baseline_signed_later_does_not_undo_an_edit() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "cups=2"]);
    // One database from before ops, copied onto a second device.
    a.sql("DELETE FROM ops; DELETE FROM sync_local;");
    let b = Copy::new();
    std::fs::copy(a.db(), b.db()).unwrap();
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=3"]);
    // b signs its baselines only now, after a's edit.
    b.ok(&["sync", "pipe", "ops"]);
    converged(&a, &b);
    assert_eq!(
        b.data(&hid)["cups"],
        3,
        "the edit stands on the copy it never touched"
    );
    assert!(!a.ok(&["conflicts"]).contains("sync_conflict"));
}

#[test]
fn an_edit_waits_for_the_create_it_depends_on_from_another_copy() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    b.ok(&["update", &hid, "cups=3"]);
    let b_only: String = b
        .ok(&["sync", "pipe", "ops"])
        .lines()
        .filter(|l| l.contains("\"kind\":\"update\""))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(b_only.lines().count(), 1);

    // c has nothing of a's: b's edit is b's seq 1, so no seq gap holds it —
    // its deps on a's create do.
    let c = Copy::new();
    let held = c.ok_in(&["sync", "pipe", "apply"], &b_only);
    assert!(
        held.starts_with("applied 0 · already had 0 · held 1 "),
        "{held}"
    );
    let filled = c.ok_in(&["sync", "pipe", "apply"], &a.ok(&["sync", "pipe", "ops"]));
    assert!(filled.contains(" held 0 "), "{filled}");
    assert_eq!(c.data(&hid)["cups"], 3, "the edit landed on the create");
    assert_eq!(c.hash(), b.hash());
}

#[test]
fn the_op_line_is_the_command_without_its_db() {
    let a = Copy::new();
    let db = a.db();
    a.ok(&["--db", db.to_str().unwrap(), "occur", "tea"]);
    let op = a
        .ops()
        .into_iter()
        .find(|o| o["slug"].as_str().unwrap().starts_with("tea:"))
        .unwrap();
    assert_eq!(op["raw"], "occur tea");
}

/// A copy of `a`'s file under `a`'s own client: the same client id, another path.
fn twin(a: &Copy) -> Copy {
    let copy = Copy::new();
    std::fs::create_dir_all(copy.tmp.path().join("home")).unwrap();
    let client = |c: &Copy| c.tmp.path().join("home/client_id");
    std::fs::copy(client(a), client(&copy)).unwrap();
    std::fs::copy(a.db(), copy.db()).unwrap();
    copy
}

/// The endpoint of the newest op `c` signed.
fn endpoint(c: &Copy) -> String {
    c.query(
        "SELECT endpoint FROM ops WHERE hid != ?1 ORDER BY hlc DESC LIMIT 1",
        "",
    )
}

const SHARED: &str = "two copies of one database sharing an endpoint key";

#[test]
fn copies_that_diverged_before_ops_keep_the_delete() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "cups=2"]);
    a.sql("DELETE FROM ops; DELETE FROM sync_local;");
    let b = Copy::new();
    std::fs::copy(a.db(), b.db()).unwrap();
    let hid = a.hid("coffee");
    // b deletes it, a edits it after, and both forget the ops, as if all of
    // it were from before them.
    b.ok(&["delete", &hid]);
    a.ok(&["update", &hid, "cups=3"]);
    for c in [&a, &b] {
        c.sql("DELETE FROM ops; DELETE FROM sync_local;");
    }
    // The live baseline is signed last, and its row written last: neither
    // may decide it.
    b.ok(&["sync", "pipe", "ops"]);
    a.ok(&["sync", "pipe", "ops"]);
    converged(&a, &b);
    for c in [&a, &b] {
        assert!(c.deleted(&hid), "the tombstone stands");
        assert!(c.ok(&["conflicts"]).contains("sync_conflict"));
    }
}

#[test]
fn a_copy_at_another_path_on_one_device_mints_its_own_endpoint() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    let copy = twin(&a);
    a.ok(&["occur", "tea"]);
    copy.ok(&["occur", "juice"]);
    assert_ne!(endpoint(&a), endpoint(&copy));
    converged(&a, &copy);
}

#[test]
fn a_moved_file_keeps_its_endpoint() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    let moved = twin(&a);
    std::fs::remove_file(a.db()).unwrap();
    moved.ok(&["occur", "tea"]);
    let endpoints: std::collections::BTreeSet<String> = moved
        .ops()
        .iter()
        .map(|o| o["endpoint"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(endpoints.len(), 1, "{endpoints:?}");
}

/// datum.day's case (da-6kpj5): the client is `DATUM_CLIENT_ID`, and the
/// `client_id` file dies with each process. The copy keeps one endpoint.
#[test]
fn a_client_named_by_the_environment_keeps_its_endpoint() {
    let a = Copy::new();
    let key = || {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", a.db())
            .env("DATUM_HOME", a.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", a.tmp.path().join("config"))
            .env("DATUM_CLIENT_ID", "web-browser")
            .args(["sync", "endpoint-key"])
            .output()
            .unwrap();
        let _ = std::fs::remove_file(a.tmp.path().join("home/client_id"));
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(key(), key());
}

#[test]
fn copies_sharing_a_key_are_refused_loudly_and_rekey_parts_them() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    // Copied before v44 learned the path: both copies take the key.
    a.sql("UPDATE sync_local SET path = ''");
    let copy = twin(&a);
    a.ok(&["occur", "tea"]);
    assert_eq!(endpoint(&a), endpoint(&copy));

    // copy has not signed a's tea, so a's vector holds more of "its" ops.
    let vector = a.ok(&["sync", "pipe", "vector"]);
    let out = copy.run(&["sync", "pipe", "ops", "--since", vector.trim()], "");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains(SHARED));
    let out = copy.run(&["sync", "pipe", "apply"], &a.ok(&["sync", "pipe", "ops"]));
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(SHARED) && err.contains("datum sync chain rekey"),
        "{err}"
    );

    let rekeyed = copy.ok(&["sync", "chain", "rekey"]);
    assert!(rekeyed.starts_with("new endpoint key "), "{rekeyed}");
    converged(&a, &copy);
    copy.ok(&["occur", "juice"]);
    assert_ne!(endpoint(&a), endpoint(&copy));
    converged(&a, &copy);
}

#[test]
fn a_fork_of_one_endpoint_is_refused_not_merged() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    a.sql("UPDATE sync_local SET path = ''");
    let copy = twin(&a);
    a.ok(&["occur", "tea"]);
    copy.ok(&["occur", "juice"]);
    // Both signed the same seq, so the vectors agree: the op at that seq,
    // which `ops --since` sends along, is what shows it.
    for (from, to) in [(&a, &copy), (&copy, &a)] {
        let out = to.run(&["sync", "pipe", "apply"], &from.ops_for(to));
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains(SHARED));
    }
}
