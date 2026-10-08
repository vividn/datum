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
            .env("NO_COLOR", "1")
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

/// da-z0zh3: a bare `undo` over an entry another copy has changed since
/// refuses, naming it, and moves nothing — the undo stack is this copy's,
/// and reverting would throw the other copy's change away.
#[test]
fn undo_refuses_a_command_whose_entry_a_sync_changed_since() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    b.ok(&["update", &hid, "cups=3"]);
    b.send(&a);
    assert_eq!(a.data(&hid)["cups"], 3);

    let out = a.run(&["undo"], "");
    assert!(!out.status.success(), "the undo must refuse");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(&hid) && err.contains("coffee") && err.contains("sync"),
        "the refusal names the entry and why: {err}"
    );
    assert_eq!(a.data(&hid)["cups"], 3, "nothing moved");
    assert!(!a.deleted(&hid));
}

/// …and so does one over an IMPORTED row, whose whole trail comes after
/// the import: a sync can be its first history row.
#[test]
fn undo_of_an_import_refuses_a_row_a_sync_changed_since() {
    let (a, b) = (Copy::new(), Copy::new());
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/datum_backup_3days.json"
    );
    a.ok(&["db", "v2import", "--from", fixture]);
    a.send(&b);
    b.ok(&["update", "syn0001", "n=7"]);
    b.send(&a);
    assert_eq!(a.data("syn0001")["n"], 7);

    let out = a.run(&["undo"], "");
    assert!(!out.status.success(), "the undo must refuse");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("syn0001") && err.contains("sync"), "{err}");
    assert!(!a.deleted("syn0001"), "nothing moved");
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

/// The live entries of `field` on `copy`: what `tail` shows.
fn live(copy: &Copy, field: &str) -> usize {
    copy.query(
        "SELECT COUNT(*) FROM entries WHERE field = ?1
           AND superseded_by IS NULL AND deleted_at IS NULL",
        field,
    )
    .parse()
    .unwrap()
}

/// Two coffees with one id, logged apart on X then Y (da-04glw). Returns
/// (X's hid, Y's hid).
fn one_id_twice(x: &Copy, y: &Copy) -> (String, String) {
    let at = "2026-10-04 08:00";
    x.ok(&["occur", "coffee", "-t", at, "note=first", "a=1"]);
    y.ok(&["occur", "coffee", "-t", at, "note=second", "b=2"]);
    (x.hid("coffee"), y.hid("coffee"))
}

#[test]
fn two_entries_with_one_id_merge_into_one_and_the_later_created_wins_a_clash() {
    let (x, y) = (Copy::new(), Copy::new());
    let (first, _) = one_id_twice(&x, &y);
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(live(copy, "coffee"), 1);
        let data = copy.data(&first);
        assert_eq!(data["a"], 1, "{data}");
        assert_eq!(data["b"], 2, "{data}");
        assert_eq!(data["note"], "second", "{data}");
        let conflicts = copy.ok(&["conflicts", "list"]);
        assert_eq!(
            conflicts.matches("identity_collision").count(),
            1,
            "both copies declared `coffee`'s spec: one event twice, no warning\n{conflicts}"
        );
        assert!(
            conflicts.contains(r#"data.note: "second" over "first""#),
            "{conflicts}"
        );
    }
    // The merge is settled: syncing again rewrites nothing.
    let history = |c: &Copy| c.query("SELECT COUNT(*) FROM entry_history WHERE ?1 != ''", "x");
    let before = (history(&x), history(&y));
    converged(&x, &y);
    assert_eq!((history(&x), history(&y)), before);
}

#[test]
fn a_survivor_older_than_its_ops_names_the_clash_too() {
    let (x, y) = (Copy::new(), Copy::new());
    x.ok(&[
        "occur",
        "coffee",
        "-t",
        "2026-10-04 08:00",
        "note=first",
        "a=1",
    ]);
    // As a database written before ops: its coffee has none.
    x.sql("DELETE FROM ops; DELETE FROM sync_local;");
    y.ok(&[
        "occur",
        "coffee",
        "-t",
        "2026-10-04 08:00",
        "note=second",
        "b=2",
    ]);
    let first = x.hid("coffee");
    y.send(&x);
    converged(&x, &y);
    for copy in [&x, &y] {
        let data = copy.data(&first);
        assert_eq!((&data["a"], &data["b"]), (&1.into(), &2.into()), "{data}");
        assert_eq!(data["note"], "second", "{data}");
        let conflicts = copy.ok(&["conflicts", "list"]);
        assert!(
            conflicts.contains(r#"data.note: "second" over "first""#),
            "{conflicts}"
        );
    }
}

#[test]
fn a_retimed_merged_entry_keeps_what_was_merged_into_it() {
    let (x, y) = (Copy::new(), Copy::new());
    let (first, _) = one_id_twice(&x, &y);
    converged(&x, &y);
    x.ok(&["retime", &first, "-t", "2026-10-04 09:00"]);
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(live(copy, "coffee"), 1);
        let data = copy.data(&first);
        assert_eq!((&data["a"], &data["b"]), (&1.into(), &2.into()), "{data}");
    }
}

#[test]
fn a_survivor_superseded_in_turn_hands_its_rows_to_the_new_one() {
    let (x, y, z) = (Copy::new(), Copy::new(), Copy::new());
    let at = "2026-10-04 08:00";
    y.ok(&["occur", "coffee", "-t", at, "b=1"]);
    x.ok(&["occur", "coffee", "-t", at, "a=1"]);
    z.ok(&["occur", "coffee", "-t", at, "c=1"]);
    let earliest = y.hid("coffee");
    // x merges z's into its own, then meets y's, older than both; y meets
    // them the other way round.
    z.send(&x);
    y.send(&x);
    z.send(&y);
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(live(copy, "coffee"), 1);
        let pointers = copy.query(
            "SELECT COUNT(*) FROM entries WHERE field = 'coffee' AND superseded_by = ?1",
            &earliest,
        );
        assert_eq!(pointers, "2");
        let data = copy.data(&earliest);
        assert_eq!(
            (&data["a"], &data["b"], &data["c"]),
            (&1.into(), &1.into(), &1.into())
        );
    }
}

#[test]
fn an_edit_of_the_merged_in_row_shows_in_the_merged_entry() {
    let (x, y) = (Copy::new(), Copy::new());
    let (first, second) = one_id_twice(&x, &y);
    y.send(&x);
    // Y has not seen X's coffee: its own is the only one it knows.
    y.ok(&["update", &second, "b=3"]);
    converged(&x, &y);
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(live(copy, "coffee"), 1);
        let data = copy.data(&first);
        assert_eq!((&data["a"], &data["b"]), (&1.into(), &3.into()), "{data}");
    }
}

#[test]
fn an_edit_of_the_merged_entry_takes_a_key_from_the_merged_in_row() {
    let (x, y) = (Copy::new(), Copy::new());
    let (first, second) = one_id_twice(&x, &y);
    // A key only the merged-in row gives, which the edit below leaves alone.
    y.ok(&["update", &second, "c=5"]);
    converged(&x, &y);
    x.ok(&["update", &first, "note=third", "b=4"]);
    converged(&x, &y);
    for copy in [&x, &y] {
        let data = copy.data(&first);
        assert_eq!(
            (&data["note"], &data["a"], &data["b"], &data["c"]),
            (&"third".into(), &1.into(), &4.into(), &5.into()),
            "{data}"
        );
    }
}

#[test]
fn deleting_the_merged_entry_leaves_nothing_to_come_back() {
    let (x, y) = (Copy::new(), Copy::new());
    let (first, _) = one_id_twice(&x, &y);
    converged(&x, &y);
    x.ok(&["delete", &first]);
    converged(&x, &y);
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(live(copy, "coffee"), 0);
        assert!(!copy.ok(&["tail"]).contains("coffee"));
    }
}

#[test]
fn one_event_logged_on_two_copies_is_one_entry_and_no_conflict() {
    let (x, y) = (Copy::new(), Copy::new());
    for copy in [&x, &y] {
        copy.ok(&["occur", "coffee", "-t", "2026-10-04 08:00", "a=1"]);
    }
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(live(copy, "coffee"), 1);
        let conflicts = copy.ok(&["conflicts", "list"]);
        assert!(!conflicts.contains("identity_collision"), "{conflicts}");
    }
}

#[test]
fn two_states_at_one_instant_under_two_ids_keep_the_earlier_and_merge_nothing() {
    let (x, y) = (Copy::new(), Copy::new());
    let at = "2026-10-04 08:00";
    for (copy, state) in [(&x, "happy"), (&y, "sad")] {
        copy.ok(&[
            "switch",
            "mood",
            state,
            "-t",
            at,
            "--id",
            "%field%:%state%",
            "n=1",
        ]);
    }
    let earlier = x.hid("mood");
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(live(copy, "mood"), 1);
        let state = copy.query("SELECT state FROM entries WHERE hid = ?1", &earlier);
        assert_eq!(state, r#""happy""#);
        let conflicts = copy.ok(&["conflicts", "list"]);
        assert!(conflicts.contains("identity_collision"), "{conflicts}");
        assert!(!conflicts.contains("    merged "), "{conflicts}");
    }
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

/// Before merging into a row with no ops, each copy signs a baseline for it:
/// two copies of one pre-ops file sign two, under two endpoints, for one hid.
#[test]
fn two_copies_each_baselining_one_op_less_survivor_converge() {
    let a = Copy::new();
    let at = "2026-10-04 08:00";
    a.ok(&["occur", "coffee", "-t", at, "a=1"]);
    a.sql("DELETE FROM ops; DELETE FROM sync_local;");
    let b = Copy::new();
    std::fs::copy(a.db(), b.db()).unwrap();
    let survivor = a.hid("coffee");
    let c = Copy::new();
    c.ok(&["occur", "coffee", "-t", at, "b=2"]);
    c.send(&a);
    c.send(&b);
    let baselines = "SELECT COUNT(DISTINCT endpoint) FROM ops WHERE kind = 'baseline' AND hid = ?1";
    converged(&a, &b);
    converged(&a, &c);
    converged(&b, &c);
    for copy in [&a, &b, &c] {
        assert_eq!(copy.hash(), a.hash());
        assert_eq!(live(copy, "coffee"), 1);
        assert_eq!(copy.query(baselines, &survivor), "2");
        let data = copy.data(&survivor);
        assert_eq!((&data["a"], &data["b"]), (&1.into(), &2.into()), "{data}");
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

/// A keep-both pair on B: A's `habit` kept, B's own superseded under the
/// same slug. Returns (kept, superseded) hids.
fn keep_both_pair(a: &Copy, b: &Copy) -> (String, String) {
    a.ok(&["occur", "habit", "-t", "2026-10-04 09:00", "note=from-a"]);
    b.ok(&["occur", "habit", "-t", "2026-10-04 09:00", "note=from-b"]);
    a.send(b);
    let by = |sql: &str| b.query(sql, "habit");
    let kept = by("SELECT hid FROM entries WHERE field = ?1 AND superseded_by IS NULL");
    let gone = by("SELECT hid FROM entries WHERE field = ?1 AND superseded_by IS NOT NULL");
    (kept, gone)
}

#[test]
fn an_edit_of_a_keep_both_row_rewrites_that_row_alone_on_both_copies() {
    let (a, b) = (Copy::new(), Copy::new());
    let (kept, gone) = keep_both_pair(&a, &b);
    let before = b.data(&gone);
    b.ok(&["update", &kept, "note=edited"]);
    converged(&a, &b);
    converged(&a, &b);
    for copy in [&a, &b] {
        assert_eq!(copy.data(&kept)["note"], "edited");
        assert_eq!(copy.data(&gone), before, "the superseded row was rewritten");
    }
}

#[test]
fn resolving_a_keep_both_pair_leaves_the_copies_converged() {
    for side in ["--take-local", "--take-remote"] {
        let (a, b) = (Copy::new(), Copy::new());
        let (_, gone) = keep_both_pair(&a, &b);
        converged(&a, &b);
        let before = b.data(&gone);
        let warning = b.query(
            "SELECT id FROM integrity_warnings WHERE kind = ?1",
            "identity_collision",
        );
        b.ok(&["conflicts", "resolve", &warning, side]);
        converged(&a, &b);
        converged(&a, &b);
        for copy in [&a, &b] {
            assert_eq!(
                copy.data(&gone),
                before,
                "{side} rewrote the superseded row"
            );
        }
    }
}

/// da-b2f8o: a value logging would refuse is merged as it is when another
/// device wrote it — a refusal there would lose data — and `datum check`
/// lists it instead.
#[test]
fn a_synced_value_its_spec_would_refuse_is_applied_and_check_lists_it() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "run", "km=5", "-q"]);
    a.send(&b);
    // `b` types the key; `a` logs before it hears of that.
    let spec = r#"{"field":"run","verb":"occur","subfields":{"km":{"value_type":"number"}}}"#;
    b.ok(&["spec", "set", "run", "--from-json", spec]);
    assert!(!b.run(&["occur", "run", "km=abc"], "").status.success());
    a.ok(&["occur", "run", "km=abc"]);
    a.send(&b);
    assert_eq!(b.data(&b.hid("run"))["km"], "abc");
    let check: Value = serde_json::from_slice(&b.run(&["check", "--json"], "").stdout).unwrap();
    let issue = check["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["category"] == "mistyped_data_value")
        .unwrap_or_else(|| panic!("check lists no mistyped value: {check}"));
    assert_eq!(issue["entries"][0]["state"], r#"km: "abc" is not a number"#);
}
