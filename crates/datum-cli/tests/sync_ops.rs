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
        self.run_env(args, stdin, &[])
    }

    fn run_env(&self, args: &[&str], stdin: &str, env: &[(&str, &str)]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env_remove("DATUM_SCRIPT")
            .envs(env.iter().copied())
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

    /// Entry `hid`'s `conflicts` column (da-05rkv), `Null` when it has none.
    fn conflicts(&self, hid: &str) -> Value {
        let c: Option<String> = rusqlite::Connection::open(self.db())
            .unwrap()
            .query_row("SELECT conflicts FROM entries WHERE hid = ?1", [hid], |r| {
                r.get(0)
            })
            .unwrap();
        c.map_or(Value::Null, |c| serde_json::from_str(&c).unwrap())
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
    // And each holds what its own ops fold to (da-n0ruf): every scenario
    // that converges is a control for `check sync` saying nothing.
    for copy in [a, b] {
        let out = copy.run(&["check", "sync"], "");
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "`check sync` on a converged copy: {said}"
        );
    }
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

/// Every op names who and what signed it (da-zjc9w), and a second copy
/// reads that back from the ops it was sent: the CLI's host, installation,
/// program and build, and on a scripted run the script `DATUM_SCRIPT` names.
#[test]
fn a_second_copy_reads_each_op_s_origin() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee"]);
    let scripted = a.run_env(&["occur", "tea"], "", &[("DATUM_SCRIPT", "nightly")]);
    assert!(scripted.status.success());
    a.send(&b);
    let origin = |field: &str| {
        let hid = b.hid(field);
        let versions: Value =
            serde_json::from_str(&b.ok(&["history", &hid, "--show", "json"])).unwrap();
        versions[0]["origin"].clone()
    };
    // One resolver gives an entry its `client_id` and an op its installation.
    let installation = a.query(
        "SELECT client_id FROM entries WHERE hid = ?1",
        &a.hid("coffee"),
    );
    for (field, script) in [("coffee", None), ("tea", Some("nightly"))] {
        let o = origin(field);
        assert_eq!(o["program"], "datum", "{o}");
        assert_eq!(o["installation"], installation, "{o}");
        assert!(
            o["version"].as_str().is_some_and(|v| v.starts_with("3.")),
            "{o}"
        );
        assert!(o["host"].as_str().is_some_and(|h| !h.is_empty()), "{o}");
        assert_eq!(
            o["user"].as_str(),
            std::env::var("USER")
                .ok()
                .filter(|u| !u.is_empty())
                .as_deref()
        );
        assert_eq!(o["script"].as_str(), script, "{o}");
    }
    let history = b.ok(&["history", &b.hid("tea")]);
    assert!(history.contains(" · script nightly"), "{history}");
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

/// da-e3be3: after a wipe, the refusal names EVERY entry another copy
/// changed, with what to do about each, and still moves nothing — undo is
/// the whole command or none of it.
#[test]
fn undo_of_a_wipe_names_every_entry_a_sync_changed_since() {
    let (a, b) = (Copy::new(), Copy::new());
    for field in ["coffee", "tea", "water"] {
        a.ok(&["occur", field, "n=1"]);
    }
    a.send(&b);
    let (coffee, tea, water) = (a.hid("coffee"), a.hid("tea"), a.hid("water"));
    a.ok(&["delete", "--all"]);
    b.ok(&["update", &coffee, "n=2"]);
    b.ok(&["update", &tea, "n=3"]);
    b.send(&a);

    let out = a.run(&["undo"], "");
    assert!(!out.status.success(), "the undo must refuse");
    let err = String::from_utf8_lossy(&out.stderr);
    for hid in [&coffee, &tea] {
        assert!(
            err.contains(&format!("`datum conflicts resolve {hid}`")),
            "the refusal names {hid} and what to do: {err}"
        );
    }
    assert!(!err.contains(&water), "water is not in the way: {err}");
    assert!(err.contains("nothing was changed"), "{err}");
    for hid in [&coffee, &tea, &water] {
        assert!(a.deleted(hid), "nothing moved: {hid} came back");
    }
}

/// da-e3be3: `undo --skip-changed` is the way past that refusal — it
/// takes back the rest of the wipe, leaves the changed entries as they are,
/// naming each, and moves the cursor on.
#[test]
fn undo_skip_changed_takes_back_the_rest_of_a_wipe() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "n=1"]);
    a.ok(&["occur", "water", "n=1"]);
    a.send(&b);
    let (coffee, water) = (a.hid("coffee"), a.hid("water"));
    a.ok(&["delete", "--all"]);
    b.ok(&["update", &coffee, "n=2"]);
    b.send(&a);

    let refused = String::from_utf8(a.run(&["undo"], "").stderr).unwrap();
    assert!(refused.contains("`datum undo --skip-changed`"), "{refused}");

    let out = a.run(&["undo", "--skip-changed"], "");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains(&coffee) && !err.contains(&water), "{err}");
    assert!(!a.deleted(&water), "the rest came back");
    assert!(a.deleted(&coffee), "the changed entry is left as it is");
    assert_eq!(a.data(&coffee)["n"], 2);
    // The cursor moved on: the next undo is the command before the wipe.
    a.ok(&["undo"]);
    assert!(a.deleted(&water));
}

/// da-2ahs4: the redo of that undo re-applies exactly what it took back —
/// the wipe again, but not over the entry the undo left as the other copy
/// left it — so the redo stack walks on past it.
#[test]
fn redo_after_undo_skip_changed_leaves_what_the_undo_left() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "n=1"]);
    a.ok(&["occur", "water", "n=1"]);
    a.send(&b);
    let (coffee, water) = (a.hid("coffee"), a.hid("water"));
    a.ok(&["delete", "--all"]);
    b.ok(&["update", &coffee, "n=3"]);
    b.send(&a);
    a.ok(&["occur", "tea"]);
    let tea = a.hid("tea");
    a.ok(&["undo"]);
    a.ok(&["undo", "--skip-changed"]);
    assert!(!a.deleted(&water));
    let coffee_before = (a.deleted(&coffee), a.data(&coffee));

    let out = a.run(&["redo"], "");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(a.deleted(&water), "the wipe is re-applied");
    assert_eq!(
        (a.deleted(&coffee), a.data(&coffee)),
        coffee_before,
        "coffee stays as the other copy left it"
    );
    assert_eq!(a.data(&coffee)["n"], 3);
    // The command undone after the wipe is redone past it.
    a.ok(&["redo"]);
    assert!(!a.deleted(&tea));
}

/// …and when another copy changed EVERY entry of the command, it takes
/// back nothing, names them all, and still moves the cursor past it: the
/// step the flag asked for, which the refusal says in those words.
#[test]
fn undo_skip_changed_with_every_entry_changed_moves_only_the_cursor() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "n=1"]);
    a.send(&b);
    let coffee = a.hid("coffee");
    a.ok(&["update", &coffee, "n=2"]);
    a.send(&b);
    b.ok(&["update", &coffee, "n=3"]);
    b.send(&a);

    let refused = String::from_utf8(a.run(&["undo"], "").stderr).unwrap();
    assert!(
        refused.contains("`datum undo --skip-changed` steps past it and takes nothing back"),
        "{refused}"
    );
    let out = a.run(&["undo", "--skip-changed"], "");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("left 1 of the 1 entries") && err.contains(&coffee),
        "{err}"
    );
    assert!(
        !err.contains("nothing moved"),
        "the skip already said it: {err}"
    );
    assert_eq!(a.data(&coffee)["n"], 3, "the other copy's change stays");
    // The cursor moved on: the next undo is the occur before the update,
    // which wrote coffee too, so the same change stands in its way.
    let next = String::from_utf8(a.run(&["undo"], "").stderr).unwrap();
    assert!(next.contains("1 of the 2 entries"), "{next}");
    // Its redo has nothing to re-apply, and says why (da-2ahs4).
    let out = a.run(&["redo"], "");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("the undo left every entry of that command as another copy left it")
            && !err.contains("nothing moved"),
        "{err}"
    );
    assert_eq!(a.data(&coffee)["n"], 3, "the other copy's change stays");
}

/// `--skip-changed` is the bare undo's: beside a named entry, which steps
/// alone already, it is refused and moves nothing.
#[test]
fn undo_skip_changed_with_a_named_entry_is_refused() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "n=1"]);
    let coffee = a.hid("coffee");
    let out = a.run(&["undo", &coffee, "--skip-changed"], "");
    assert!(!out.status.success(), "a named undo must refuse the flag");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--skip-changed is for a bare `undo`"), "{err}");
    assert!(!a.deleted(&coffee), "nothing moved");
}

/// A refused redo says what redo does, not undo's words.
#[test]
fn redo_refuses_in_its_own_words_an_entry_a_sync_changed_since() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "n=1"]);
    a.send(&b);
    let coffee = a.hid("coffee");
    a.ok(&["update", &coffee, "n=2"]);
    a.ok(&["undo"]);
    a.send(&b);
    b.ok(&["update", &coffee, "n=3"]);
    b.send(&a);

    // The dry run, which lights the web's REDO, exits as the redo would
    // (da-38x8z).
    let dry = a.run(&["redo", "--dry-run"], "");
    assert!(!dry.status.success(), "the dry run must refuse too");
    assert_eq!(a.data(&coffee)["n"], 3, "a dry run moves nothing");

    let out = a.run(&["redo"], "");
    assert!(!out.status.success(), "the redo must refuse");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("redo re-applies the whole command or nothing"),
        "{err}"
    );
    assert!(
        !err.contains("--skip-changed"),
        "redo has no way past: {err}"
    );
    assert_eq!(a.data(&coffee)["n"], 3, "nothing moved");
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
    assert_eq!(
        (a.conflicts(&hid), b.conflicts(&hid)),
        (Value::Null, Value::Null)
    );
}

/// The values of each side of `key` in `conflicts`, as they are listed.
fn sides(conflicts: &Value, key: &str) -> Vec<Value> {
    conflicts[key]
        .as_array()
        .unwrap_or_else(|| panic!("no conflict on {key}: {conflicts}"))
        .iter()
        .map(|s| s["value"].clone())
        .collect()
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
    // Both copies hold the one conflict, byte for byte (da-05rkv).
    assert_eq!(a.conflicts(&hid), b.conflicts(&hid));
    let mut values = sides(&a.conflicts(&hid), "data.cups");
    values.sort_by_key(|v| v.as_i64());
    assert_eq!(values, [4, 5]);
    for side in a.conflicts(&hid)["data.cups"].as_array().unwrap() {
        assert!(side["op"].as_str().unwrap().contains('/'), "{side}");
        assert!(
            side["hlc"].is_string() && side["device"].is_string(),
            "{side}"
        );
    }
    for copy in [&a, &b] {
        let check = copy.run(&["check"], "");
        let out = String::from_utf8_lossy(&check.stdout);
        assert!(out.contains("sync_conflict"), "{out}");
        let conflicts = copy.ok(&["conflicts"]);
        assert!(conflicts.contains("data.cups"), "{conflicts}");
    }
}

#[test]
fn a_resolution_settles_the_conflict_on_every_copy() {
    for pick in [None, Some("cups=7")] {
        let (a, b) = (Copy::new(), Copy::new());
        a.ok(&["occur", "coffee", "cups=2"]);
        a.send(&b);
        let hid = a.hid("coffee");
        a.ok(&["update", &hid, "cups=4"]);
        b.ok(&["update", &hid, "cups=5"]);
        converged(&a, &b);
        let shown = a.data(&hid)["cups"].clone();
        // Keeping the value shown is an edit too: one op, setting it.
        let mut resolve = vec!["conflicts", "resolve", &hid];
        resolve.extend(pick);
        let op = one_op(&b, &resolve);
        assert!(op["effect"]["set"]["data.cups"].is_number(), "{op}");
        converged(&a, &b);
        for copy in [&a, &b] {
            assert_eq!(copy.conflicts(&hid), Value::Null, "{pick:?}");
            let want = if pick.is_some() {
                7.into()
            } else {
                shown.clone()
            };
            assert_eq!(copy.data(&hid)["cups"], want, "{pick:?}");
            assert!(copy.ok(&["conflicts"]).contains("No sync conflicts"));
        }
    }
}

#[test]
fn an_edit_concurrent_with_the_resolution_is_a_new_conflict_on_both() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=4"]);
    b.ok(&["update", &hid, "cups=5"]);
    converged(&a, &b);
    a.ok(&["conflicts", "resolve", &hid, "cups=6"]);
    b.ok(&["update", &hid, "cups=8"]);
    converged(&a, &b);
    assert_eq!(a.conflicts(&hid), b.conflicts(&hid));
    let mut values = sides(&a.conflicts(&hid), "data.cups");
    values.sort_by_key(|v| v.as_i64());
    assert_eq!(values, [6, 8], "the settled sides stay settled");
}

/// A coffee two copies edited unseen: each set `cups` and `note`, and the
/// later dropped the `size` the earlier changed (da-07594). The copies and
/// the hid.
fn clash() -> (Copy, Copy, String) {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2", "size=small", "note=plain"]);
    a.send(&b);
    let hid = a.hid("coffee");
    b.ok(&["update", &hid, "cups=5", "size=big", "note=strong"]);
    a.ok(&[
        "update",
        &hid,
        "--strategy",
        "useNew",
        "cups=4",
        "note=weak",
    ]);
    converged(&a, &b);
    (a, b, hid)
}

/// `[A, B]` of `key`, read from the column: which copy's clock is the
/// earlier is not assumed.
fn a_and_b(copy: &Copy, hid: &str, key: &str) -> (Value, Value) {
    match &sides(&copy.conflicts(hid), key)[..] {
        [a, b] => (a.clone(), b.clone()),
        other => panic!("{key} has {} sides", other.len()),
    }
}

#[test]
fn each_strategy_combines_a_and_b_and_settles_both_copies() {
    use serde_json::json;
    for how in ["--use-a", "--use-b", "--prefer-a", "--prefer-b", "--merge"] {
        let (a, b, hid) = clash();
        let (cups, note, size) = (
            a_and_b(&a, &hid, "data.cups"),
            a_and_b(&a, &hid, "data.note"),
            a_and_b(&a, &hid, "data.size"),
        );
        // One side unset `size`: only the other has it.
        assert!(size.0.is_null() != size.1.is_null(), "{size:?}");
        let either = if size.0.is_null() { &size.1 } else { &size.0 };
        let want = match how {
            "--use-a" => json!({"cups": cups.0, "note": note.0, "size": size.0}),
            "--use-b" => json!({"cups": cups.1, "note": note.1, "size": size.1}),
            "--prefer-a" => json!({"cups": cups.0, "note": note.0, "size": either}),
            "--prefer-b" => json!({"cups": cups.1, "note": note.1, "size": either}),
            _ => json!({
                "cups": [cups.0, cups.1], "note": [note.0, note.1], "size": either,
            }),
        };
        one_op(&b, &["conflicts", "resolve", &hid, how]);
        converged(&a, &b);
        for copy in [&a, &b] {
            let data = copy.data(&hid);
            for key in ["cups", "note", "size"] {
                assert_eq!(data[key], want[key], "{how} {key}");
            }
            assert_eq!(copy.conflicts(&hid), Value::Null, "{how}");
        }
    }
}

#[test]
fn a_bare_resolve_leaves_unset_the_key_the_later_side_unset() {
    // The one key where `--prefer-b` and what the entry shows part: B unset
    // `size`, so the entry shows none, and `--prefer-b` would keep A's.
    let (a, b, hid) = clash();
    let (earlier, later) = a_and_b(&a, &hid, "data.size");
    assert_eq!((earlier, later), ("big".into(), Value::Null));
    let shown = a.data(&hid);
    assert_eq!(shown["size"], Value::Null);
    let op = one_op(&b, &["conflicts", "resolve", &hid]);
    assert!(
        op["effect"]["unset"].to_string().contains("data.size"),
        "{op}"
    );
    converged(&a, &b);
    for copy in [&a, &b] {
        assert_eq!(copy.data(&hid), shown, "no data moved");
        assert_eq!(copy.conflicts(&hid), Value::Null);
    }
}

#[test]
fn picks_take_their_keys_and_the_strategy_the_rest_under_the_same_letters() {
    let (a, b, hid) = clash();
    // The letters are the column's order, so both copies print one listing.
    let listing = a.ok(&["conflicts"]);
    assert_eq!(listing, b.ok(&["conflicts"]));
    assert_eq!(
        a.ok(&["conflicts", "--json"]),
        b.ok(&["conflicts", "--json"])
    );
    for key in ["data.cups", "data.note", "data.size"] {
        let at = listing.find(&format!("    {key}\n")).expect(key);
        let mut sides = listing[at..].lines().skip(1);
        assert!(sides.next().unwrap().starts_with("      A  "), "{listing}");
        assert!(sides.next().unwrap().starts_with("      B  "), "{listing}");
    }
    for copy in [&a, &b] {
        let check = copy.run(&["check"], "");
        let out = String::from_utf8_lossy(&check.stdout);
        assert!(out.contains("sync_conflict (x1)"), "{out}");
    }
    let (cups, _) = a_and_b(&a, &hid, "data.cups");
    let (_, size) = a_and_b(&a, &hid, "data.size");
    // A side, a new value, and B's for the key not named.
    one_op(
        &a,
        &[
            "conflicts",
            "resolve",
            &hid,
            "cups=a",
            "note=sweet",
            "--use-b",
        ],
    );
    converged(&a, &b);
    for copy in [&a, &b] {
        let data = copy.data(&hid);
        assert_eq!(
            (&data["cups"], &data["note"], &data["size"]),
            (&cups, &Value::from("sweet"), &size)
        );
        assert!(copy.ok(&["conflicts"]).contains("No sync conflicts"));
        assert_eq!(copy.ok(&["conflicts", "--json"]), "[]\n");
        assert!(!copy.ok(&["check"]).contains("sync_conflict"));
    }
}

#[test]
fn a_key_with_three_sides_takes_a_pick_by_letter() {
    let (a, b, c) = (Copy::new(), Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    a.send(&c);
    let hid = a.hid("coffee");
    for (copy, cups) in [(&a, "cups=4"), (&b, "cups=5"), (&c, "cups=6")] {
        copy.ok(&["update", &hid, cups]);
    }
    converged(&a, &b);
    converged(&b, &c);
    converged(&a, &b);
    let third = sides(&a.conflicts(&hid), "data.cups");
    assert_eq!(third.len(), 3);
    let listing = c.ok(&["conflicts"]);
    assert_eq!(listing, a.ok(&["conflicts"]));
    assert!(listing.contains("\n      C  "), "{listing}");
    // A strategy is over two sides.
    let refused = c.run(&["conflicts", "resolve", &hid, "--use-a"], "");
    let err = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "{err}");
    assert!(err.contains("data.cups has more than two sides"), "{err}");
    assert_eq!(c.conflicts(&hid), a.conflicts(&hid), "nothing was written");
    c.ok(&["conflicts", "resolve", &hid, "cups=c"]);
    converged(&b, &c);
    converged(&a, &b);
    for copy in [&a, &b, &c] {
        assert_eq!(copy.data(&hid)["cups"], third[2]);
        assert_eq!(copy.conflicts(&hid), Value::Null);
    }
}

/// A database from before da-05rkv: its conflicts were rows of its own
/// `integrity_warnings`. Upgraded, they are its entries' `conflicts`, the
/// same as a copy that never had the rows derives, and the rows are gone.
#[test]
fn the_upgrade_moves_filed_conflicts_into_the_entries() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=4"]);
    b.ok(&["update", &hid, "cups=5"]);
    converged(&a, &b);
    let want = b.conflicts(&hid);
    assert!(want.is_object(), "{want}");
    a.sql(
        "DROP INDEX entries_conflicts;
         ALTER TABLE entries DROP COLUMN conflicts;
         INSERT INTO integrity_warnings
             (id, hid, entry_slug, local_entry, detected_at, detected_by, kind)
         VALUES ('w1', 'w1', 'coffee', '{}', '2026-10-01T00:00:00Z', 'sync_pull', 'sync_conflict'),
                ('w2', 'w2', 'coffee', '{}', '2026-10-01T00:00:00Z', 'sync_pull', 'lww_overwrite'),
                ('w3', 'w3', 'coffee', '{}', '2026-10-01T00:00:00Z', 'sync_pull', 'identity_collision');
         UPDATE _meta SET value = '57' WHERE key = 'schema_version';",
    );
    a.ok(&["conflicts"]);
    assert_eq!(a.conflicts(&hid), want);
    let filed = a.query(
        "SELECT COUNT(*) FROM integrity_warnings WHERE ?1 != ''",
        "x",
    );
    assert_eq!(filed, "0");
    assert_eq!(a.hash(), b.hash());
}

/// Three copies: one has the two clashing ops in one batch, one gets them
/// one at a time, and the two that made them each hear of the other's.
#[test]
fn a_conflict_reads_the_same_however_its_ops_arrived() {
    let (a, b, c, d) = (Copy::new(), Copy::new(), Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    for x in [&b, &c, &d] {
        a.send(x);
    }
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=4"]);
    b.ok(&["update", &hid, "cups=5"]);
    a.send(&d);
    b.send(&d);
    a.send(&b);
    b.send(&c);
    b.send(&a);
    let want = a.conflicts(&hid);
    assert!(want.is_object(), "{want}");
    for x in [&b, &c, &d] {
        assert_eq!(x.conflicts(&hid), want);
        assert_eq!(x.hash(), a.hash());
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
    assert_eq!(a.conflicts(&hid), b.conflicts(&hid));
    assert_eq!(sides(&a.conflicts(&hid), "deleted").len(), 2);
    for copy in [&a, &b] {
        assert!(copy.deleted(&hid), "the entry stays deleted");
        let conflicts = copy.ok(&["conflicts"]);
        assert!(conflicts.contains("deleted"), "{conflicts}");
    }
    // Resolving brings it back, on both.
    b.ok(&["conflicts", "resolve", &hid, "deleted=false"]);
    converged(&a, &b);
    for copy in [&a, &b] {
        assert!(!copy.deleted(&hid));
        assert_eq!(copy.data(&hid)["cups"], 9);
        assert_eq!(copy.conflicts(&hid), Value::Null);
    }
}

/// A delete against an undelete that did not see it: the later clock's
/// stands — here the undelete, so the entry is live — and both copies name
/// the two sides until one that saw both decides.
#[test]
fn a_delete_against_an_undelete_shows_on_both_whichever_stands() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    a.ok(&["delete", &hid]);
    b.ok(&["delete", &hid]);
    b.ok(&["undo", &hid]);
    converged(&a, &b);
    assert_eq!(a.conflicts(&hid), b.conflicts(&hid));
    assert_eq!(sides(&a.conflicts(&hid), "deleted"), [true, false]);
    for copy in [&a, &b] {
        assert!(!copy.deleted(&hid), "the later clock's, the undelete");
        assert!(!copy.ok(&["conflicts"]).contains("(deleted)"));
    }
    a.ok(&["conflicts", "resolve", &hid, "deleted=true"]);
    converged(&a, &b);
    for copy in [&a, &b] {
        assert!(copy.deleted(&hid));
        assert_eq!(copy.conflicts(&hid), Value::Null);
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
        // Both copies declared `coffee`'s spec: one event twice, no conflict.
        assert_eq!(
            copy.query(
                "SELECT COUNT(*) FROM entries WHERE conflicts IS NOT NULL AND ?1 = ?1",
                ""
            ),
            "1"
        );
        assert_eq!(
            sides(&copy.conflicts(&first), "data.note"),
            ["first", "second"]
        );
    }
    assert_eq!(x.conflicts(&first), y.conflicts(&first));
    // The merge is settled: syncing again rewrites nothing.
    let history = |c: &Copy| c.query("SELECT COUNT(*) FROM entry_history WHERE ?1 != ''", "x");
    let before = (history(&x), history(&y));
    converged(&x, &y);
    assert_eq!((history(&x), history(&y)), before);
}

/// A kit's spec for `weighin` on K, and on P afterwards the automatic one a
/// first log writes — and, with `set`, a key its owner then set (da-m82n0).
/// Returns K, P and the hid of the kit's spec, the earlier-created.
fn a_kit_then_a_first_log(set: &[&str]) -> (Copy, Copy, String) {
    let (k, p) = (Copy::new(), Copy::new());
    let kit = k.tmp.path().join("w.toml");
    std::fs::write(
        &kit,
        "[kit]\nname = \"w\"\n\n[[field]]\nname = \"weighin\"\nverb = \"occur\"\n\
         remainder = \"note\"\n\n[field.subfields.kg]\nvalue_type = \"number\"\n",
    )
    .unwrap();
    k.ok(&["kit", "install", kit.to_str().unwrap(), "--yes"]);
    let spec = k.query("SELECT hid FROM entries WHERE slug = ?1", "spec:weighin");
    p.ok(&["occur", "weighin"]);
    if !set.is_empty() {
        p.ok(set);
    }
    converged(&k, &p);
    (k, p, spec)
}

#[test]
fn a_kit_spec_applies_over_an_automatic_one_created_later() {
    let (k, p, spec) = a_kit_then_a_first_log(&[]);
    for copy in [&k, &p] {
        let data = copy.data(&spec);
        assert_eq!(data["subfields"]["kg"]["value_type"], "number", "{data}");
        assert_eq!(
            (&data["kit_id"], &data["remainder"]),
            (&"w".into(), &"note".into())
        );
        assert_eq!(copy.conflicts(&spec), Value::Null, "nothing is contested");
        assert_eq!(live(copy, "spec"), 1);
    }
}

#[test]
fn a_key_the_automatic_specs_owner_set_is_an_intent_like_any_other() {
    let (k, p, spec) = a_kit_then_a_first_log(&["setup", "weighin", "--remainder", "memo"]);
    for copy in [&k, &p] {
        let data = copy.data(&spec);
        assert_eq!(data["remainder"], "memo", "the edit goes over the create");
        assert_eq!(data["subfields"]["kg"]["value_type"], "number", "{data}");
        let conflicts = copy.conflicts(&spec);
        assert_eq!(sides(&conflicts, "data.remainder"), ["note", "memo"]);
        assert_eq!(conflicts.as_object().unwrap().len(), 1, "{conflicts}");
    }
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
        assert_eq!(
            sides(&copy.conflicts(&first), "data.note"),
            ["first", "second"]
        );
    }
}

/// `update` naming the row merged in, after the merge, edits the entry it
/// shows: it settles the clash, from either copy.
#[test]
fn an_update_naming_the_merged_in_row_settles_the_clash() {
    for by_its_maker in [true, false] {
        let (x, y) = (Copy::new(), Copy::new());
        let (first, second) = one_id_twice(&x, &y);
        converged(&x, &y);
        let editor = if by_its_maker { &y } else { &x };
        editor.ok(&["update", &second, "note=third"]);
        converged(&x, &y);
        for copy in [&x, &y] {
            assert_eq!(copy.conflicts(&first), Value::Null, "{by_its_maker}");
            assert_eq!(copy.data(&first)["note"], "third");
        }
    }
}

#[test]
fn an_edit_of_the_merged_entry_settles_the_clash_of_its_rows() {
    let (x, y) = (Copy::new(), Copy::new());
    let (first, _) = one_id_twice(&x, &y);
    converged(&x, &y);
    x.ok(&["update", &first, "note=both"]);
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(copy.conflicts(&first), Value::Null);
        assert_eq!(copy.data(&first)["note"], "both");
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

/// Every order of `items`.
fn orders<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let first = rest.remove(i);
        for mut tail in orders(&rest) {
            tail.insert(0, first.clone());
            out.push(tail);
        }
    }
    out
}

/// What a copy holds of `field`: its verify hash, then each row's slug,
/// occur time, data, survivor and conflicts, by hid.
fn holds(copy: &Copy, field: &str) -> String {
    let rows = copy.query(
        "SELECT group_concat(r, char(10)) FROM (
           SELECT hid || ' ' || slug || ' ' || occur_time || ' ' || data || ' '
                  || ifnull(superseded_by, '-') || ' ' || ifnull(conflicts, '-') AS r
           FROM entries WHERE field = ?1 ORDER BY hid)",
        field,
    );
    format!("{}\n{rows}", copy.hash())
}

/// A fresh copy given `first` and then each of `ops` in turn, one apply
/// each, in every order: what it holds of `field` after each order.
fn in_every_order(first: &[Value], ops: &[Value], field: &str) -> Vec<String> {
    let line = |v: &[Value]| v.iter().map(|o| format!("{o}\n")).collect::<String>();
    orders(ops)
        .into_iter()
        .map(|order| {
            let z = Copy::new();
            z.ok_in(&["sync", "pipe", "apply"], &line(first));
            for op in &order {
                z.ok_in(&["sync", "pipe", "apply"], &line(std::slice::from_ref(op)));
            }
            holds(&z, field)
        })
        .collect()
}

/// vividn's A4 (da-vkeg0): x and y each log a coffee at 08:00 and one of them
/// retimes its own to 09:00 before seeing the other's. The two creates are one entry, the retime a change to it,
/// whatever order a copy receives the three ops in: one coffee at 09:00
/// showing both rows, contested nowhere, the same on every copy.
#[test]
fn one_instant_logged_twice_and_retimed_is_one_entry_in_every_arrival_order() {
    let (at, to) = ("2026-10-04 08:00", "2026-10-04 09:00");
    // x logs first, so its row is the one shown: x retimes it before y logs
    // or after, or y retimes the row merged into it.
    for (retimer, before) in [(0, true), (0, false), (1, false)] {
        let (x, y) = (Copy::new(), Copy::new());
        let r = [&x, &y][retimer];
        let retime = || {
            let hid = r.hid("coffee");
            r.ok(&["retime", &hid, "-t", to]);
            r.query("SELECT slug FROM entries WHERE hid = ?1", &hid)
        };
        x.ok(&["occur", "coffee", "-t", at, "a=1"]);
        let mut moved = String::new();
        if before {
            moved = retime();
        }
        y.ok(&["occur", "coffee", "-t", at, "b=2"]);
        if !before {
            moved = retime();
        }
        let when = ["after", "before"][before as usize];
        let case = format!("{} retimed {when} y logged", ["x", "y"][retimer]);
        let (mut first, mut coffee) = (Vec::new(), Vec::new());
        for op in x.ops().into_iter().chain(y.ops()) {
            match op["field"] == "coffee" && op["slug"] != "spec:coffee" {
                true => coffee.push(op),
                false => first.push(op),
            }
        }
        assert_eq!(coffee.len(), 3, "{case}: two creates and a retime");
        let every = in_every_order(&first, &coffee, "coffee");
        converged(&x, &y);
        for held in &every {
            assert_eq!(held, &holds(&x, "coffee"), "{case}");
        }
        for copy in [&x, &y] {
            assert_eq!(live(copy, "coffee"), 1, "{case}");
            let shown = copy.query(
                "SELECT hid FROM entries WHERE field = ?1 AND superseded_by IS NULL",
                "coffee",
            );
            let data = copy.data(&shown);
            assert_eq!((&data["a"], &data["b"]), (&1.into(), &2.into()), "{case}");
            assert_eq!(
                copy.query("SELECT slug FROM entries WHERE hid = ?1", &shown),
                moved,
                "{case}: the retime moved the one entry"
            );
            assert_eq!(copy.conflicts(&shown), Value::Null, "{case}");
        }
    }
}

/// Both retime the one coffee, neither having seen the other: the later
/// clock's instant shows, and both are named, the same on both copies.
#[test]
fn two_retimes_of_one_instant_logged_twice_are_an_occur_time_conflict() {
    let (x, y) = (Copy::new(), Copy::new());
    let (first, second) = one_id_twice(&x, &y);
    x.ok(&["retime", &first, "-t", "2026-10-04 09:00"]);
    y.ok(&["retime", &second, "-t", "2026-10-04 10:00"]);
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(live(copy, "coffee"), 1);
        assert_eq!(sides(&copy.conflicts(&first), "occur_time").len(), 2);
    }
    assert_eq!(x.conflicts(&first), y.conflicts(&first));
}

/// A coffee logged at an instant another copy's coffee was moved FROM, by a
/// copy that had seen it move, is a coffee of its own on every copy.
#[test]
fn a_log_where_a_seen_entry_was_moved_from_is_an_entry_of_its_own() {
    let (x, y) = (Copy::new(), Copy::new());
    x.ok(&["occur", "coffee", "-t", "2026-10-04 08:00", "a=1"]);
    x.ok(&["retime", &x.hid("coffee"), "-t", "2026-10-04 09:00"]);
    x.send(&y);
    y.ok(&["occur", "coffee", "-t", "2026-10-04 08:00", "b=2"]);
    converged(&x, &y);
    for copy in [&x, &y] {
        assert_eq!(live(copy, "coffee"), 2);
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
        assert!(conflicts.contains("No sync conflicts"), "{conflicts}");
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
        // The row set aside shows on the one kept: its state, not merged.
        assert_eq!(sides(&copy.conflicts(&earlier), "state"), ["happy", "sad"]);
        assert_eq!(copy.data(&earlier)["n"], 1);
    }
    assert_eq!(x.conflicts(&earlier), y.conflicts(&earlier));
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

/// Creates of one `spec:<field>` slug are one entry however the apply that
/// brings them is cut (da-yrtk6): a first sync stopped part-way and
/// finished later holds what one clean session does. Three copies make the
/// spec one after another: the first two each the bare one a first log
/// writes or a declared one, the second editing its own after, the third
/// declaring a colour of its own — so two rows go under the survivor, and
/// the later-created of them wins the colour whichever arrived first. A copy applying
/// every copy's ops whole agrees with one applying them in two pieces, cut
/// at every point, as sent and reversed: any create comes first, and the
/// edit before its own create.
#[test]
fn specs_merged_under_one_slug_fold_the_same_however_their_apply_is_cut() {
    let make = |copy: &Copy, declared: bool, color: &str| match declared {
        true => copy.ok(&["setup", "tea", "--verb", "occur", "--color", color]),
        false => copy.ok(&["occur", "tea", "-t", "2026-10-04 08:00"]),
    };
    for (first, second) in [(false, true), (true, false), (true, true)] {
        let (x, y, z) = (Copy::new(), Copy::new(), Copy::new());
        make(&x, first, "#ff0000");
        make(&y, second, "#00ffff");
        y.ok(&["setup", "tea", "--glyph", "y"]);
        z.ok(&["setup", "tea", "--verb", "occur", "--color", "#0000ff"]);
        let lines: Vec<String> = [&x, &y, &z]
            .iter()
            .flat_map(|c| {
                c.ok(&["sync", "pipe", "ops"])
                    .lines()
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .collect();
        let apply = |pieces: &[&[String]]| {
            let copy = Copy::new();
            for piece in pieces {
                copy.ok_in(&["sync", "pipe", "apply"], &piece.join("\n"));
            }
            copy
        };
        let whole = apply(&[&lines]);
        let spec = whole.query(
            "SELECT hid FROM entries WHERE slug = ?1 AND superseded_by IS NULL",
            "spec:tea",
        );
        assert_eq!(
            whole.query(
                "SELECT COUNT(*) FROM entries WHERE superseded_by = ?1",
                &spec
            ),
            "2",
            "the three are one entry"
        );
        // The last-created's colour, and the edit, whichever row it is of.
        let data = whole.data(&spec);
        assert_eq!(data["color"], "#0000ff", "{data}");
        assert_eq!(data["glyph"], "y", "{data}");
        let want = whole.hash();
        let reversed: Vec<String> = lines.iter().rev().cloned().collect();
        for order in [&lines, &reversed] {
            for cut in 1..order.len() {
                let (head, tail) = order.split_at(cut);
                assert_eq!(
                    apply(&[head, tail]).hash(),
                    want,
                    "first declared: {first}, second declared: {second}, cut at {cut} of {}",
                    order.len()
                );
            }
        }
    }
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
    assert!(a.ok(&["conflicts"]).contains("No sync conflicts"));
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
        assert_eq!(sides(&c.conflicts(&hid), "deleted").len(), 2);
    }
    assert_eq!(a.conflicts(&hid), b.conflicts(&hid));
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
    let (a, b) = (Copy::new(), Copy::new());
    let (kept, gone) = keep_both_pair(&a, &b);
    converged(&a, &b);
    let before = b.data(&gone);
    assert_eq!(sides(&b.conflicts(&kept), "data.note").len(), 2);
    b.ok(&["conflicts", "resolve", &kept, "note=from-b"]);
    converged(&a, &b);
    converged(&a, &b);
    for copy in [&a, &b] {
        assert_eq!(copy.data(&gone), before, "the superseded row was rewritten");
        assert_eq!(copy.data(&kept)["note"], "from-b");
        assert_eq!(copy.conflicts(&kept), Value::Null);
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
    b.ok(&["setup", "run", "--from-json", spec]);
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

// ─── History (da-s4erq) ──────────────────────────────────────────────────────

/// What `datum history` prints for `hid`, and its `--json`, newest first.
fn history(copy: &Copy, hid: &str) -> (String, Value) {
    let json = copy.ok(&["history", hid, "--json"]);
    (
        copy.ok(&["history", hid]),
        serde_json::from_str(&json).unwrap(),
    )
}

/// `side[key]` of each side of a fork, sorted as text.
fn fork_sides(fork: &Value, key: &str) -> Vec<String> {
    let mut all: Vec<String> = fork
        .as_array()
        .unwrap_or_else(|| panic!("no fork: {fork}"))
        .iter()
        .map(|s| s[key].to_string())
        .collect();
    all.sort();
    all
}

/// History is read from the ops, so every copy that holds them prints the
/// same one — the copy that made a change, the one that pulled it, and one
/// that took every op in a single batch, each of which journalled it its
/// own way — with the clash as a fork naming both sides and the edit that
/// saw them as its join.
#[test]
fn history_is_the_same_on_every_copy_and_shows_the_fork_and_its_join() {
    for pick in [None, Some("cups=7")] {
        let (a, b, c) = (Copy::new(), Copy::new(), Copy::new());
        a.ok(&["occur", "coffee", "cups=2"]);
        a.send(&b);
        let hid = a.hid("coffee");
        a.ok(&["update", &hid, "cups=4"]);
        b.ok(&["update", &hid, "cups=5"]);
        converged(&a, &b);
        let (text, json) = history(&a, &hid);
        assert_eq!((text.clone(), json.clone()), history(&b, &hid));
        // The later of the two updates is where the entry forked.
        let fork = &json[0]["fork"]["data.cups"];
        assert_eq!(fork_sides(fork, "value"), ["4", "5"], "{json}");
        assert_eq!(fork_sides(fork, "version"), ["2", "3"], "{json}");
        // Its sides are the `conflicts` column's, each with its version.
        let mut column = fork.clone();
        for side in column.as_array_mut().unwrap() {
            side.as_object_mut().unwrap().remove("version");
        }
        assert_eq!(column, a.conflicts(&hid)["data.cups"]);
        assert!(json[1].get("fork").is_none(), "one side is no fork: {json}");
        assert!(text.contains("\n        fork data.cups: "), "{text}");
        for side in fork.as_array().unwrap() {
            let device = side["device"].as_str().unwrap();
            assert!(
                text.contains(&format!("{} ({device}, ", side["value"])),
                "{text}"
            );
        }

        let mut resolve = vec!["conflicts", "resolve", &hid];
        resolve.extend(pick);
        b.ok(&resolve);
        converged(&a, &b);
        a.send(&c);
        let (text, json) = history(&a, &hid);
        assert_eq!((text.clone(), json.clone()), history(&b, &hid));
        assert_eq!((text.clone(), json.clone()), history(&c, &hid));
        assert_eq!(
            json.as_array().unwrap().len(),
            4,
            "a version per op: {json}"
        );
        assert!(json[0].get("fork").is_none(), "{json}");
        let join = &json[0]["join"]["data.cups"];
        assert_eq!(join["value"], a.data(&hid)["cups"], "{json}");
        let took = match pick {
            // A value neither side gave.
            Some(_) => "a value of its own".to_string(),
            // The value shown: the side whose it was.
            None => {
                let kept = fork
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["value"] == join["value"]);
                format!("{}'s", kept.unwrap()["device"].as_str().unwrap())
            }
        };
        let line = format!("\n        join data.cups: {} ({took})\n", join["value"]);
        assert!(text.contains(&line), "{line:?} not in {text}");
        assert_eq!(
            json[0]["raw"],
            resolve.join(" "),
            "the line that settled it"
        );

        // The undo journal is each copy's own, and is not what was read.
        let journal = |c: &Copy| {
            c.query(
                "SELECT group_concat(change_kind) FROM entry_history WHERE entry_hid = ?1",
                &hid,
            )
        };
        assert_ne!(journal(&a), journal(&c));
    }
}

#[test]
fn history_shows_a_delete_against_an_edit_as_a_fork_and_bringing_it_back_as_its_join() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    b.ok(&["update", &hid, "cups=9"]);
    a.ok(&["delete", &hid]);
    converged(&a, &b);
    let (_, json) = history(&a, &hid);
    assert_eq!(json, history(&b, &hid).1);
    let fork = &json[0]["fork"]["deleted"];
    assert_eq!(fork_sides(fork, "value"), ["false", "true"], "{json}");
    b.ok(&["conflicts", "resolve", &hid, "deleted=false"]);
    converged(&a, &b);
    let (text, json) = history(&a, &hid);
    assert_eq!((text.clone(), json.clone()), history(&b, &hid));
    assert_eq!(json[0]["kind"], "undelete", "{json}");
    assert_eq!(json[0]["join"]["deleted"]["value"], false, "{json}");
    assert!(text.contains("\n        join deleted: false ("), "{text}");
}

/// Two rows merged under one id are one entry with one history: the ops of
/// both, the clash between them a fork, and each version the merged entry
/// as the ops up to it leave it.
#[test]
fn history_of_a_merged_entry_holds_the_ops_of_both_rows() {
    let (x, y) = (Copy::new(), Copy::new());
    let (first, second) = one_id_twice(&x, &y);
    converged(&x, &y);
    let (text, json) = history(&x, &first);
    assert_eq!((text, json.clone()), history(&y, &first));
    let versions = json.as_array().unwrap();
    let hids: Vec<&str> = versions
        .iter()
        .map(|v| v["hid"].as_str().unwrap())
        .collect();
    assert!(
        hids.contains(&first.as_str()) && hids.contains(&second.as_str()),
        "{hids:?}"
    );
    let forked = versions.iter().find(|v| v.get("fork").is_some()).unwrap();
    assert_eq!(
        fork_sides(&forked["fork"]["data.note"], "value"),
        [r#""first""#, r#""second""#]
    );
    let latest = versions.len().to_string();
    let shown: Value =
        serde_json::from_str(&x.ok(&["history", &first, "--version", &latest])).unwrap();
    assert_eq!(
        shown["data"],
        x.data(&first),
        "the newest version is the entry"
    );
    assert_eq!(shown["hid"], first);
}

/// A database that never synced has ops too: a version per command, with
/// the line that wrote it and what it set, and no device to name. The
/// numbers are the ones `--version` and `undo --to-version` take.
#[test]
fn a_database_that_never_synced_shows_a_version_per_command() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "cups=2"]);
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=3"]);
    a.ok(&["update", &hid, "cups=4", "with=oat"]);
    let (text, json) = history(&a, &hid);
    let kinds: Vec<&str> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["update", "update", "create"]);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 9, "{text}");
    assert_eq!(lines[0], format!("hid: {hid}"));
    assert!(
        lines[1].starts_with("  v3  ")
            && lines[1].ends_with(&format!("update    'update {hid} cups=4 with=oat'")),
        "{text}"
    );
    // Each version says who and what signed it (da-zjc9w).
    for at in [2, 5, 8] {
        assert!(lines[at].starts_with("        origin: "), "{text}");
    }
    assert_eq!(lines[3], r#"        set data.cups=4 data.with="oat""#);
    assert_eq!(lines[6], "        set data.cups=3");
    assert!(
        lines[7].starts_with("  v1  ") && lines[7].ends_with("create    'occur coffee cups=2'"),
        "{text}"
    );
    assert!(
        !text.contains(json[0]["device"].as_str().unwrap()),
        "{text}"
    );

    let version = |n: &str| -> Value {
        serde_json::from_str(&a.ok(&["history", &hid, "--version", n])).unwrap()
    };
    assert_eq!(version("2")["data"]["cups"], 3);
    assert_eq!(version("3")["data"], a.data(&hid));
    let diff = a.ok(&["history", &hid, "--diff", "1", "2"]);
    assert!(diff.contains("- data.cups: 2\n+ data.cups: 3"), "{diff}");
    assert!(!a
        .run(&["history", &hid, "--version", "4"], "")
        .status
        .success());

    a.ok(&["undo", &hid, "--to-version", "1"]);
    assert_eq!(a.data(&hid), serde_json::json!({ "cups": 2 }));
    let (text, json) = history(&a, &hid);
    assert_eq!(json[0]["kind"], "undo", "{text}");
    assert_eq!(json[0]["version"], 4);
    assert!(
        text.contains("        set data.cups=2, unset data.with"),
        "{text}"
    );
    let refused = a.run(&["undo", &hid, "--to-version", "9"], "");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("entry has 4 version(s)"));
}

/// An entry older than its ops has no history until something needs one:
/// its first edit signs it as it stood, a `baseline` under the line that
/// wrote it, and the edit is the version after.
#[test]
fn an_entry_older_than_its_ops_opens_its_history_with_a_baseline() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "cups=2"]);
    a.sql("DELETE FROM ops; DELETE FROM sync_local;");
    let hid = a.hid("coffee");
    assert!(a.ok(&["history", &hid]).contains("(no history recorded)"));
    assert!(!a
        .run(&["undo", &hid, "--to-version", "1"], "")
        .status
        .success());
    a.ok(&["update", &hid, "cups=3"]);
    let (text, json) = history(&a, &hid);
    assert_eq!(json[1]["kind"], "baseline", "{text}");
    assert_eq!(json[1]["raw"], "occur coffee cups=2", "{text}");
    assert_eq!(json[0]["raw"], format!("update {hid} cups=3"), "{text}");
    a.ok(&["undo", &hid, "--to-version", "1"]);
    assert_eq!(a.data(&hid)["cups"], 2);
}

// ─── Diagnostics (da-n0ruf) ─────────────────────────────────────────────────

/// `datum check sync`: whether it passed, and what it printed.
fn check_sync(copy: &Copy, json: bool) -> (bool, String) {
    let args: &[&str] = match json {
        true => &["check", "sync", "--json"],
        false => &["check", "sync"],
    };
    let out = copy.run(args, "");
    (out.status.success(), String::from_utf8(out.stdout).unwrap())
}

/// Rows dropped with their ops kept, as `db restore --hard` once left them
/// (da-95y4o): `check sync` names each entry those ops make and this copy
/// has no row for, with its ops, and `sync refold` leaves them as found.
#[test]
fn check_names_the_entries_whose_ops_have_no_row() {
    let a = Copy::new();
    a.ok(&["occur", "tea", "-t", "10:00"]);
    a.ok(&["occur", "juice", "-t", "12:00"]);
    // Clean, it says what it read: each entry and the spec its log declared.
    let (ok, clean) = check_sync(&a, false);
    assert!(
        ok && clean.contains("4 rows read against 4 sync ops, 0 differ"),
        "{clean}"
    );
    a.sql("DELETE FROM entries WHERE field = 'juice' OR slug = 'spec:juice'");

    let (ok, said) = check_sync(&a, false);
    assert!(!ok, "ops with no row are an error: {said}");
    assert!(said.contains("[ERROR] sync_orphan_ops (x2)"), "{said}");
    for named in ["\n  juice:", "\n  spec:juice ("] {
        assert!(said.contains(named), "{named:?} in {said}");
    }
    assert!(said.contains("): create by "), "{said}");
    assert!(!said.contains("tea"), "tea has its row: {said}");
    let json: Value = serde_json::from_str(&check_sync(&a, true).1).unwrap();
    let mut fields: Vec<&str> = json["issues"][0]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["field"].as_str().unwrap())
        .collect();
    fields.sort();
    assert_eq!(fields, ["juice", "spec"], "{json}");

    let orphans =
        "SELECT COUNT(*) FROM ops WHERE kind = ?1 AND hid NOT IN (SELECT hid FROM entries)";
    let refold = a.ok(&["sync", "refold"]);
    assert!(refold.contains("nothing to refold"), "{refold}");
    assert!(
        refold.contains("2 entries with ops and no row left as found"),
        "{refold}"
    );
    assert_eq!(a.query(orphans, "create"), "2");
}

/// Entries whose rows were dropped under their ops, as a purge before
/// #491 left them (da-53whj): `check --fix` deletes each as `datum delete`
/// does, signing the delete, so `check sync` is clean after it and every
/// copy ends with the entry deleted, one that held its row included and
/// whichever copy a new one hears from first.
#[test]
fn check_fix_deletes_the_entries_whose_ops_have_no_row() {
    let (a, applied) = (Copy::new(), Copy::new());
    a.ok(&["occur", "tea", "-t", "10:00"]);
    a.ok(&["occur", "juice", "-t", "12:00"]);
    converged(&a, &applied);
    let juice = a.hid("juice");
    a.sql("DELETE FROM entries WHERE field = 'juice' OR slug = 'spec:juice'");
    let (ok, said) = check_sync(&a, false);
    assert!(
        !ok && said.contains("[ERROR] sync_orphan_ops (x2)"),
        "{said}"
    );
    assert!(said.contains("`datum check --fix` deletes each"), "{said}");

    let fixed = a.ok(&["check", "--fix"]);
    assert!(
        fixed.contains("✓ sync_orphan_ops repaired: 2 entries (2 detected, 0 still mismatched)"),
        "{fixed}"
    );
    let (ok, said) = check_sync(&a, false);
    assert!(ok && !said.contains("sync_orphan_ops"), "{said}");
    assert!(a.deleted(&juice));
    let deletes = "SELECT COUNT(*) FROM ops WHERE kind = ?1 AND body LIKE '%check --fix%'";
    assert_eq!(a.query(deletes, "delete"), "2", "a delete signed for each");

    // The copy that applied the ops deletes the entry too.
    converged(&a, &applied);
    assert!(applied.deleted(&juice));
    // A new copy ends the same whichever copy it hears from first.
    let (x, y) = (Copy::new(), Copy::new());
    a.send(&x);
    applied.send(&x);
    applied.send(&y);
    a.send(&y);
    assert!(x.deleted(&juice) && y.deleted(&juice));
    assert_eq!(x.hash(), y.hash());
    assert_eq!(x.hash(), a.hash());
}

/// An orphan whose entry was logged again, as a re-import does, is written
/// again from its ops and so merged with the live entry (da-t5kxx), as
/// every copy holding both already holds them: no op is signed for it, a
/// delete of it would take the live entry too, and nothing is left for
/// `check sync` to name.
#[test]
fn check_fix_merges_an_orphan_a_live_entry_shares_an_identity_with() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "-t", "10:00", "cups=2"]);
    a.ok(&["occur", "tea", "-t", "11:00"]);
    let old = a.hid("coffee");
    a.sql("DELETE FROM entries");
    a.ok(&["occur", "coffee", "-t", "10:00", "cups=2", "milk=oat"]);
    let new = a.hid("coffee");
    let (_, said) = check_sync(&a, false);
    assert!(said.contains("[ERROR] sync_orphan_ops (x4)"), "{said}");
    let coffee = format!(
        "--fix merges it with the live {}",
        a.query("SELECT slug FROM entries WHERE hid = ?1", &new,)
    );
    assert!(said.contains(&coffee), "{coffee:?} in {said}");
    assert!(
        said.contains("--fix merges it with the live spec:coffee"),
        "{said}"
    );
    assert!(said.contains("\n  tea:"), "{said}");

    let ops = a.ops().len();
    let fixed = a.ok(&["check", "--fix"]);
    assert!(
        fixed.contains("✓ sync_orphan_ops repaired: 4 entries (4 detected, 0 still mismatched)"),
        "{fixed}"
    );
    let (ok, said) = check_sync(&a, false);
    assert!(ok && !said.contains("sync_orphan_ops"), "{said}");
    let signed: Vec<Value> = a.ops().into_iter().skip(ops).collect();
    assert_eq!(
        signed.len(),
        2,
        "tea and spec:tea's deletes alone: {signed:?}"
    );
    assert!(signed.iter().all(|o| o["kind"] == "delete"), "{signed:?}");
    // The coffee shows once, merged, the live entry's data in it.
    let live = "SELECT hid FROM entries WHERE field = ?1 AND deleted_at IS NULL
                  AND superseded_by IS NULL";
    let shown = a.query(live, "coffee");
    assert!([&old, &new].contains(&&shown), "{shown}");
    assert_eq!(
        a.data(&shown),
        serde_json::json!({"cups": 2, "milk": "oat"})
    );
    assert_eq!(
        a.query("SELECT COUNT(*) FROM entries WHERE hid = ?1", &old),
        "1",
        "the orphan has its row again"
    );
    // A copy that applied both sets of ops holds the same.
    let b = Copy::new();
    a.send(&b);
    assert_eq!(a.hash(), b.hash());
    converged(&a, &b);
}

/// A row that is not what its ops fold to (da-yrtk6, da-z3y0o) is named
/// with its column and both values, and `sync refold` writes it again from
/// the ops: the copies verify again, and no op was signed for it.
#[test]
fn check_names_a_row_that_is_not_its_ops_fold_and_refold_writes_it_again() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    converged(&a, &b);
    let hid = b.hid("coffee");
    b.sql(&format!(
        "UPDATE entries SET data = '{{\"cups\":5}}' WHERE hid = '{hid}'"
    ));
    assert_ne!(a.hash(), b.hash());

    let (ok, said) = check_sync(&b, false);
    assert!(!ok, "a stale row is an error: {said}");
    assert!(said.contains("[ERROR] sync_stale_row (x1)"), "{said}");
    assert!(said.contains("\n  coffee:"), "{said}");
    assert!(said.contains("\n    data.cups: row 5, ops 2\n"), "{said}");
    let json: Value = serde_json::from_str(&check_sync(&b, true).1).unwrap();
    let named = &json["issues"][0]["entries"];
    assert_eq!(named[0]["field"], "coffee", "{json}");
    assert_eq!(named[0]["state"], "data.cups", "{json}");
    assert_eq!(named.as_array().unwrap().len(), 1, "{json}");

    let ops = b.ops().len();
    let refold = b.ok(&["sync", "refold"]);
    assert!(refold.contains("refolded 1 row from its ops"), "{refold}");
    assert!(
        refold.contains("\n    data.cups: was 5, now 2\n"),
        "{refold}"
    );
    assert_eq!(b.ops().len(), ops, "a refold signs no op");
    assert_eq!(b.data(&hid)["cups"], 2);
    assert_eq!(a.hash(), b.hash());
    // Done, it says so and what it examined, and the check is clean.
    let again: Value = serde_json::from_str(&b.ok(&["sync", "refold", "--json"])).unwrap();
    assert_eq!(again["refolded"], serde_json::json!([]), "{again}");
    assert_eq!(again["rows"], 2, "{again}");
    assert!(check_sync(&b, false).0);
}

/// `sync refold --dry-run` (da-g8wj9) names each row a refold would write,
/// every column old → new, and leaves the database byte for byte as it was;
/// the refold then makes exactly those changes.
#[test]
fn refold_dry_run_says_what_each_row_loses_and_writes_nothing() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    converged(&a, &b);
    let hid = b.hid("coffee");
    b.sql(&format!(
        "UPDATE entries SET data = '{{\"cups\":5,\"milk\":true}}' WHERE hid = '{hid}'"
    ));
    let (_, said) = check_sync(&b, false);
    assert!(
        said.contains("`datum sync refold --dry-run` says what each would lose"),
        "{said}"
    );

    let files = || {
        std::fs::read_dir(b.tmp.path())
            .unwrap()
            .map(|f| f.unwrap().path())
            .filter(|p| p.to_string_lossy().contains("data.db"))
            .map(|p| (p.clone(), std::fs::read(p).unwrap()))
            .collect::<Vec<_>>()
    };
    // A read first, so the files are as a command leaves them.
    b.ok(&["tail"]);
    let before = files();
    let dry = b.ok(&["sync", "refold", "--dry-run"]);
    assert!(dry.contains("would refold 1 row from its ops"), "{dry}");
    assert!(dry.contains("nothing written"), "{dry}");
    assert!(
        dry.contains("\n    data.cups: holds 5, becomes 2\n"),
        "{dry}"
    );
    assert!(
        dry.contains("\n    data.milk: holds true, becomes ∅"),
        "{dry}"
    );
    let would: Value =
        serde_json::from_str(&b.ok(&["sync", "refold", "--dry-run", "--json"])).unwrap();
    assert!(files() == before, "a dry run writes nothing");
    assert_eq!(b.data(&hid)["cups"], 5);
    assert_eq!(
        would["would_refold"].as_array().unwrap().len(),
        1,
        "{would}"
    );
    assert!(would.get("still").is_none(), "{would}");

    let done: Value = serde_json::from_str(&b.ok(&["sync", "refold", "--json"])).unwrap();
    assert_eq!(done["refolded"], would["would_refold"], "{done}");
    assert_eq!(a.hash(), b.hash());
}

/// A copy whose rows were folded under an older fold rule refolds them as
/// it opens (da-sqpjt): the row an older build's rule left — stood in for
/// by one set by hand — is written again from its ops by the next command,
/// whatever it is, signing nothing, and the copy verifies with one that
/// never held it. On the current rule an open refolds nothing.
#[test]
fn a_copy_folded_under_an_older_rule_refolds_as_it_opens() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    converged(&a, &b);
    let hid = b.hid("coffee");
    let stale = format!("UPDATE entries SET data = '{{\"cups\":5}}' WHERE hid = '{hid}'");
    let rule = "SELECT value FROM _meta WHERE key = ?1";
    let current = b.query(rule, "fold_rule");

    b.sql(&stale);
    b.ok(&["tail"]);
    assert_eq!(
        b.data(&hid)["cups"],
        5,
        "on the current rule, an open refolds nothing"
    );
    assert!(!check_sync(&b, false).0);

    b.sql("UPDATE _meta SET value = '0' WHERE key = 'fold_rule'");
    let ops = b.ops().len();
    assert_eq!(
        b.data(&hid)["cups"],
        2,
        "the open under `sync pipe ops` refolded it"
    );
    assert_eq!(b.query(rule, "fold_rule"), current);
    assert_eq!(b.ops().len(), ops, "a refold signs no op");
    assert_eq!(a.hash(), b.hash());
    assert!(check_sync(&b, false).0);
    let kept = "SELECT json_extract(snapshot, '$.data.cups') FROM entry_history
                WHERE entry_hid = ?1 AND change_kind = 'sync_merge'
                ORDER BY version DESC LIMIT 1";
    assert_eq!(
        b.query(kept, &hid),
        "5",
        "the row as it stood is in its history"
    );

    // A copy from before there was a rule refolds too.
    b.sql(&format!(
        "{stale}; DELETE FROM _meta WHERE key = 'fold_rule'"
    ));
    b.ok(&["tail"]);
    assert_eq!(b.data(&hid)["cups"], 2);
    assert_eq!(b.query(rule, "fold_rule"), current);

    // A refold that fails is said, and the open goes on, stamped.
    b.sql(&format!(
        "{stale}; UPDATE _meta SET value = '0' WHERE key = 'fold_rule';
         CREATE TRIGGER no_refold BEFORE UPDATE ON entries
         BEGIN SELECT RAISE(ABORT, 'refused here'); END"
    ));
    let out = b.run(&["tail"], "");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains("⚠ refolding this database's rows under the current fold rule failed")
            && said.contains("refused here"),
        "{said}"
    );
    assert_eq!(b.query(rule, "fold_rule"), current);
}

/// A build older than the stamp does not fold back to its own rule as it
/// opens, but a row it writes is folded by that rule, so the write lowers
/// the stamp to it (da-w13ei) and a newer build's next open refolds — which
/// is the open above, from a stamp below its rule.
#[test]
fn a_write_under_an_older_rule_lowers_a_newer_stamp() {
    let b = Copy::new();
    b.ok(&["occur", "coffee", "cups=2"]);
    let rule = "SELECT value FROM _meta WHERE key = ?1";
    let current: u32 = b.query(rule, "fold_rule").parse().unwrap();
    let newer = (current + 1).to_string();
    b.sql(&format!(
        "UPDATE _meta SET value = '{newer}' WHERE key = 'fold_rule'"
    ));

    b.ok(&["tail"]);
    assert_eq!(b.query(rule, "fold_rule"), newer, "a read leaves it");
    b.ok(&["occur", "tea"]);
    assert_eq!(b.query(rule, "fold_rule"), current.to_string());
}

/// The `conflicts` column is the fold's as well: one this copy lost is
/// found by name and written again.
#[test]
fn a_lost_conflicts_column_is_found_and_refolded() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid("coffee");
    a.ok(&["update", &hid, "cups=4"]);
    b.ok(&["update", &hid, "cups=5"]);
    converged(&a, &b);
    assert!(!a.conflicts(&hid).is_null());
    b.sql("UPDATE entries SET conflicts = NULL");
    assert_ne!(a.hash(), b.hash());

    let (ok, said) = check_sync(&b, false);
    assert!(
        !ok && said.contains("\n    conflicts: row ∅, ops {"),
        "{said}"
    );
    b.ok(&["sync", "refold"]);
    assert_eq!(b.conflicts(&hid), a.conflicts(&hid));
    assert_eq!(a.hash(), b.hash());
}

/// Ops that fold to no row this build can store — here a verb it does not
/// know — are a finding of their own, naming the entry and why, and the
/// check goes on past them: the stale row beside it is still found, and
/// still refolded.
#[test]
fn ops_that_fold_to_no_storable_row_are_named_and_the_check_goes_on() {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "cups=2"]);
    a.ok(&["occur", "tea", "cups=1"]);
    let (coffee, tea) = (a.hid("coffee"), a.hid("tea"));
    a.sql(&format!(
        "UPDATE ops SET body = json_set(body, '$.effect.create.verb', 'bogus') WHERE hid = '{coffee}';
         UPDATE entries SET data = '{{\"cups\":5}}' WHERE hid = '{tea}'"
    ));

    let (ok, said) = check_sync(&a, false);
    assert!(!ok, "{said}");
    assert!(said.contains("[ERROR] sync_unfoldable (x1)"), "{said}");
    let named = said.lines().find(|l| l.starts_with("  coffee:")).unwrap();
    assert!(named.contains("bogus"), "why is beside it: {said}");
    assert!(said.contains("[ERROR] sync_stale_row (x1)"), "{said}");
    assert!(said.contains("\n    data.cups: row 5, ops 1\n"), "{said}");

    let refold = a.ok(&["sync", "refold"]);
    assert!(refold.contains("refolded 1 row from its ops"), "{refold}");
    assert!(
        refold.contains("1 entry whose ops fold to no row left as found"),
        "{refold}"
    );
    assert_eq!(a.data(&tea)["cups"], 1);
    assert_eq!(a.data(&coffee)["cups"], 2);
}

/// Every live row on `copy`, of every field, specs and aliases included.
fn live_rows(copy: &Copy) -> usize {
    copy.query(
        "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL
           AND superseded_by IS NULL AND ?1 = ?1",
        "",
    )
    .parse()
    .unwrap()
}

/// `delete --all` is an ordinary delete of every row, specs and aliases
/// too: it reaches the other copy like any delete, one bare `undo` on the
/// copy that ran it brings every row back on both, and `redo` deletes them
/// again (da-97gth).
#[test]
fn delete_all_wipes_both_copies_and_one_undo_restores_both() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.ok(&["switch", "mood", "happy"]);
    a.ok(&["alias", "set", "c", "occur coffee"]);
    b.ok(&["occur", "tea"]);
    converged(&a, &b);
    let all = live_rows(&a);
    assert!(all >= 5, "three entries, an alias and their specs: {all}");

    let said = a.ok(&["delete", "--all"]);
    assert!(
        said.contains(&format!("Deleted {all} entries")) && said.contains("datum undo"),
        "{said}"
    );
    converged(&a, &b);
    assert_eq!((live_rows(&a), live_rows(&b)), (0, 0));

    a.ok(&["undo"]);
    converged(&a, &b);
    assert_eq!((live_rows(&a), live_rows(&b)), (all, all));

    a.ok(&["redo"]);
    converged(&a, &b);
    assert_eq!((live_rows(&a), live_rows(&b)), (0, 0));
}

/// da-s9ngd: two copies that each logged the same note made two events, and
/// keep both.
#[test]
fn an_identical_note_on_each_copy_is_two_notes() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["note", "diary", "hello"]);
    b.ok(&["note", "diary", "hello"]);
    converged(&a, &b);
    for copy in [&a, &b] {
        let notes = copy.query(
            "SELECT COUNT(*) FROM entries WHERE field = ?1 AND deleted_at IS NULL",
            "diary",
        );
        assert_eq!(notes, "2");
    }
}

/// A zone the neighbours gave is marked a guess (da-h8dvi) until one is
/// stated — `retime -z`, or an `update` putting the entry in another zone —
/// and the marker's removal is in the op, so every copy drops it
/// (da-n79py). A retime naming no zone keeps it.
#[test]
fn stating_the_zone_drops_the_neighbours_marker_on_every_copy() {
    let (a, b) = (Copy::new(), Copy::new());
    for day in [12, 13, 14, 16, 17, 18] {
        let date = format!("2024-06-{day}");
        a.ok(&[
            "occur",
            "coffee",
            "-z",
            "Pacific/Chatham",
            "-d",
            &date,
            "-t",
            "12:00",
        ]);
    }
    let meta = |copy: &Copy, hid: &str| {
        copy.query(
            "SELECT coalesce(extra_metadata, 'none') FROM entries WHERE hid = ?1",
            hid,
        )
    };
    let stated: [&[&str]; 3] = [
        &["retime", "-z", "Asia/Tokyo"],
        &["retime", "-z", "Pacific/Chatham"],
        &["update", "occur_time=2024-06-15T12:00+09:00"],
    ];
    for (i, how) in stated.into_iter().enumerate() {
        let field = format!("tea{i}");
        a.ok(&["occur", &field, "-d", "2024-06-15", "-t", "12:00"]);
        let hid = a.hid(&field);
        a.ok(&["retime", &hid, "-t", "12:30"]);
        converged(&a, &b);
        assert_eq!(meta(&b, &hid), r#"{"tz_from":"neighbours"}"#, "{how:?}");

        a.ok(&[&how[..1], &[hid.as_str()], &how[1..]].concat());
        converged(&a, &b);
        assert_eq!(
            (meta(&a, &hid), meta(&b, &hid)),
            ("none".into(), "none".into()),
            "{how:?}"
        );
    }
}

/// da-ftls7: `rename` moves a field's entries of every shape and its spec,
/// each an edit op, so a second copy folds to the same rows; one `undo`
/// takes it all back on both, and renaming onto a field that exists is
/// refused.
#[test]
fn rename_moves_every_shape_and_the_spec_on_every_copy_and_undoes() {
    let (a, b) = (Copy::new(), Copy::new());
    a.ok(&["setup", "mood", "--verb", "switch", "--add-state", "happy"]);
    for line in [
        &["occur", "mood", "-t", "-60"][..],
        &["switch", "mood", "happy", "-t", "-50"],
        &["end", "mood", "-t", "-40"],
        &["start", "mood", "10", "-t", "-20"],
        &["start", "mood", "-t", "-5"],
        &["note", "mood", "a", "jot"],
        &["add", "mood", "--id", "one", "x=1"],
        &["occur", "coffee"],
    ] {
        a.ok(line);
    }
    converged(&a, &b);
    let rows = |copy: &Copy, field: &str| -> Vec<(String, String)> {
        let conn = rusqlite::Connection::open(copy.db()).unwrap();
        let mut stmt = conn
            .prepare("SELECT hid, slug FROM entries WHERE field = ?1 ORDER BY hid")
            .unwrap();
        let rows = stmt.query_map([field], |r| Ok((r.get(0)?, r.get(1)?)));
        rows.unwrap().map(Result::unwrap).collect()
    };
    let mood = rows(&a, "mood");
    assert_eq!(mood.len(), 7);

    let said = a.ok(&["rename", "mood", "feeling", "--dry-run"]);
    assert!(
        said.contains("would rename mood → feeling: 7 entries"),
        "{said}"
    );
    assert_eq!(rows(&a, "mood"), mood, "a dry run moves nothing");

    let said = a.ok(&["rename", "mood", "feeling"]);
    assert!(
        said.contains("aliases now: feeling, feelinge, happy"),
        "{said}"
    );
    let moved: Vec<_> = mood
        .iter()
        .map(|(hid, slug)| (hid.clone(), slug.replacen("mood:", "feeling:", 1)))
        .collect();
    assert_eq!(
        rows(&a, "feeling"),
        moved,
        "each keeps its hid, its slug follows"
    );
    assert_eq!(a.ok(&["tail", "mood"]).trim(), "[No data]");
    assert_eq!(
        a.ok(&["tail", "feeling"]).lines().count(),
        8,
        "a header and 7 rows"
    );
    let spec = a.query(
        "SELECT json_extract(data, '$.field') FROM entries WHERE slug = ?1",
        "spec:feeling",
    );
    assert_eq!(spec, "feeling");
    converged(&a, &b);
    assert_eq!(rows(&b, "feeling"), moved);
    assert!(rows(&b, "mood").is_empty());

    let out = a.run(&["rename", "feeling", "coffee"], "");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && err.contains("`coffee` already has entries"),
        "{err}"
    );

    a.ok(&["undo"]);
    assert_eq!(rows(&a, "mood"), mood, "one undo restores every row");
    assert!(rows(&a, "feeling").is_empty());
    assert!(a.ok(&["setup", "mood"]).contains("happy"));
    converged(&a, &b);
    assert_eq!(rows(&b, "mood"), mood);

    // An OLD entry logged after the rename, at a renamed one's instant, is
    // an entry of its own on every copy, not a join into the renamed row.
    a.ok(&["rename", "mood", "feeling"]);
    converged(&a, &b);
    let at = a.query(
        "SELECT occur_time FROM entries WHERE field = ?1 AND state = '\"happy\"'",
        "feeling",
    );
    b.ok(&["switch", "mood", "happy", "-t", &at]);
    let mood_at = "SELECT occur_time FROM entries WHERE field = ?1";
    assert_eq!(b.query(mood_at, "mood"), at, "at the renamed row's instant");
    converged(&a, &b);
    for copy in [&a, &b] {
        assert_eq!(rows(copy, "feeling"), moved);
        assert_eq!(rows(copy, "mood").len(), 1);
    }
}

/// da-ftls7 (vremya's review): an undo that would put an entry back on a
/// slug another row took since (the old name logged again after a rename)
/// is refused with a sentence naming that row, never a raw UNIQUE error. A
/// deleted row there is refused too (da-8mh8i). Nothing is written, so the
/// copies still agree.
#[test]
fn undoing_a_rename_onto_a_reused_old_slug_is_refused_naming_the_row() {
    let (a, b) = (Copy::new(), Copy::new());
    let at = "2026-10-19T09:00";
    a.ok(&["switch", "mood", "happy", "-t", at]);
    let happy = a.hid("mood");
    a.ok(&["rename", "mood", "feeling"]);
    a.ok(&["switch", "mood", "calm", "-t", at]);
    let calm = a.hid("mood");
    converged(&a, &b);
    let refused = |args: &[&str], held: &str| {
        let out = a.run(args, "");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{err}");
        assert!(err.contains(&format!("{held} holds it now")), "{err}");
        assert!(!err.contains("UNIQUE"), "{err}");
    };
    // Live: the calm entry holds mood:T.
    refused(&["undo", &happy], &format!("entry {calm}"));
    // Deleted: the calm entry undone, its tombstone still holds it.
    a.ok(&["undo"]);
    refused(&["undo"], &format!("deleted entry {calm}"));
    assert_eq!(
        a.query("SELECT field FROM entries WHERE hid = ?1", &happy),
        "feeling"
    );
    converged(&a, &b);
}
