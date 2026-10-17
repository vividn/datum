//! A write that can't take the write lock is spooled, never lost (da-9q3fr).
//!
//! Another connection holds the lock for real here, as a migration or a
//! sync apply would, and the real binary runs under it: each command prints
//! what it would write with QUEUED for its action and exits 0 well inside
//! the 5 s it used to wait, and the next command after the holder lets go
//! writes it as it was computed — its time the one it ran at, an undo the
//! entry it took back, a retime the time it moved to.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::Value;
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let db = tmp.path().join("spool.db");
        Self {
            _tmp: tmp,
            home,
            db,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_in(args, "")
    }

    fn run_in(&self, args: &[&str], stdin: &str) -> Output {
        use std::io::Write;
        use std::process::Stdio;
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("NO_COLOR", "1")
            .args(args)
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

    fn ok(&self, args: &[&str]) -> String {
        self.ok_in(args, "")
    }

    fn ok_in(&self, args: &[&str], stdin: &str) -> String {
        let out = self.run_in(args, stdin);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// Send `other` the ops it lacks.
    fn send(&self, other: &Env) {
        let vector = other.ok(&["sync", "pipe", "vector"]);
        let ops = self.ok(&["sync", "pipe", "ops", "--since", vector.trim()]);
        other.ok_in(&["sync", "pipe", "apply"], &ops);
    }

    fn query(&self, sql: &str) -> Option<String> {
        Connection::open(&self.db)
            .unwrap()
            .query_row(sql, [], |r| r.get(0))
            .unwrap()
    }

    /// Run a command under the held lock: it succeeds, says QUEUED, and
    /// does not wait out the lock.
    fn queued(&self, args: &[&str]) -> String {
        let began = Instant::now();
        let out = self.run(args);
        let took = began.elapsed();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(stdout.contains("QUEUED"), "{args:?} printed: {stdout}");
        // ~0.8 s here: the 750 ms it waits for the lock, then the snapshot.
        // Two seconds leaves a loaded runner room and is still far from the 5 s.
        assert!(took < Duration::from_secs(2), "{args:?} took {took:?}");
        stdout
    }

    /// Hold the write lock until the connection is dropped.
    fn hold(&self) -> Connection {
        let c = Connection::open(&self.db).unwrap();
        c.execute_batch("BEGIN IMMEDIATE").unwrap();
        c
    }

    fn spool(&self) -> PathBuf {
        let mut p = self.db.clone().into_os_string();
        p.push(".spool");
        p.into()
    }

    /// `(field, occur_time, deleted, data)` of each entry not a spec, by hid.
    fn rows(&self) -> Vec<(String, String, String, bool, Option<String>)> {
        let c = Connection::open(&self.db).unwrap();
        let mut s = c
            .prepare(
                "SELECT hid, field, occur_time, deleted_at IS NOT NULL, data FROM entries
                 WHERE field != 'spec' ORDER BY rowid",
            )
            .unwrap();
        s.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
    }

    fn hid_of(&self, field: &str) -> String {
        self.rows()
            .into_iter()
            .find(|r| r.1 == field)
            .unwrap_or_else(|| panic!("no {field} entry"))
            .0
    }

    /// `check sync` is clean, and the ops held are signed under `keys`
    /// keys: a drain or a snapshot that minted this copy another would make
    /// it a new device to its chain.
    fn check_sync_clean(&self, keys: usize) {
        let held = self.query("SELECT CAST(COUNT(DISTINCT endpoint) AS TEXT) FROM ops");
        assert_eq!(held, Some(keys.to_string()), "ops signed under another key");
        let out = self.run(&["check", "sync"]);
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && text.contains("No consistency issues"),
            "{text}"
        );
    }
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[test]
fn logs_under_a_held_lock_are_queued_and_land_at_the_time_they_ran() {
    let env = Env::new();
    assert!(env.run(&["occur", "coffee"]).status.success());
    let before = Utc::now();
    let holder = env.hold();
    env.queued(&["occur", "tea"]);
    env.queued(&["start", "work"]);
    env.queued(&["end", "work"]);
    let ran = Utc::now();
    assert_eq!(env.rows().len(), 1, "nothing lands while the lock is held");
    let spooled = std::fs::read(env.spool()).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    drop(holder);

    let out = env.run(&["tail"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("3 queued change(s) written"), "{err}");
    assert!(!env.spool().exists(), "a drained spool is gone");
    let rows = env.rows();
    let fields: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
    assert_eq!(fields, ["coffee", "tea", "work", "work"]);
    for (_, field, time, _, _) in &rows[1..] {
        let t = at(time);
        assert!(
            before <= t && t <= ran,
            "{field} at {t}, ran {before}..{ran}"
        );
    }
    env.check_sync_clean(1);

    // A drain cut off before it rewrote the spool runs again over lines
    // that landed: they are skipped, and nothing more is written.
    std::fs::write(env.spool(), spooled).unwrap();
    let out = env.run(&["tail"]);
    assert!(!String::from_utf8_lossy(&out.stderr).contains("queued"));
    assert_eq!(env.rows(), rows);
    assert!(!env.spool().exists());
}

#[test]
fn an_undo_a_retime_and_an_update_under_a_held_lock_land_as_computed() {
    let env = Env::new();
    assert!(env.run(&["occur", "coffee"]).status.success());
    assert!(env.run(&["occur", "tea"]).status.success());
    let (coffee, tea) = (env.hid_of("coffee"), env.hid_of("tea"));
    let was = at(&env.rows()[0].2);

    let holder = env.hold();
    // The undo takes back the tea: the newest change when it ran.
    let out = env.queued(&["undo"]);
    assert!(out.contains(&tea[..5]), "{out}");
    env.queued(&["retime", &coffee, "+30"]);
    env.queued(&["update", &coffee, "size=large"]);
    // A command spooled after them runs on what they did: this undo takes
    // back the queued update, not the tea it would find in the file.
    let out = env.queued(&["undo"]);
    assert!(out.contains(&coffee[..5]), "{out}");
    env.queued(&["update", &coffee, "size=small"]);
    drop(holder);

    assert!(env.run(&["tail"]).status.success());
    let rows = env.rows();
    let (_, _, time, deleted, data) = &rows[0];
    assert!(!deleted);
    assert_eq!(at(time), was + chrono::Duration::minutes(30));
    assert_eq!(data.as_deref(), Some(r#"{"size":"small"}"#));
    assert!(rows[1].3, "the undone tea is deleted");
    env.check_sync_clean(1);

    // The undo stack moved as the queued commands moved it: a real undo
    // now takes back the last of them, the second update.
    assert!(env.run(&["undo"]).status.success());
    assert_eq!(env.rows()[0].4, None);
}

/// A spooled edit and another copy's edit of the same key, landed between
/// the spool and its drain, saw nothing of each other (the spooled op keeps
/// the `deps` of the snapshot it was made on), so the fold keeps both as a
/// conflict, the same on both copies. The value shown until it is resolved
/// is the later HLC's, the drained op's: signed at the drain, it is this
/// copy's newest, and an HLC never runs behind an op its copy has applied.
#[test]
fn a_spooled_edit_and_a_peers_edit_meanwhile_are_a_conflict() {
    let (a, b) = (Env::new(), Env::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid_of("coffee");

    let holder = a.hold();
    a.queued(&["update", &hid, "cups=4"]);
    drop(holder);
    // The peer's edit, made without seeing it, lands first: a spooler at
    // work holds the spool, so this command's drain leaves it.
    b.ok(&["update", &hid, "cups=5"]);
    let spooling = std::fs::File::create(a.spool().with_extension("spool.lock")).unwrap();
    spooling.lock().unwrap();
    b.send(&a);
    assert!(a.spool().exists(), "the spool waited for its lock");
    let data = format!("SELECT data FROM entries WHERE hid = '{hid}'");
    assert_eq!(a.query(&data).as_deref(), Some(r#"{"cups":5}"#));
    drop(spooling);

    a.ok(&["tail"]);
    assert!(!a.spool().exists());
    a.send(&b);
    for copy in [&a, &b] {
        assert_eq!(copy.query(&data).as_deref(), Some(r#"{"cups":4}"#));
        let conflicts = copy.ok(&["conflicts"]);
        assert!(conflicts.contains("data.cups"), "{conflicts}");
        copy.check_sync_clean(2);
    }
    let conflicts = format!("SELECT conflicts FROM entries WHERE hid = '{hid}'");
    let sides: Value = serde_json::from_str(&a.query(&conflicts).unwrap()).unwrap();
    let mut values: Vec<i64> = sides["data.cups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["value"].as_i64().unwrap())
        .collect();
    values.sort();
    assert_eq!(values, [4, 5]);
    assert_eq!(a.query(&conflicts), b.query(&conflicts));
}

/// A spooled command waiting on its person holds nothing another needs: an
/// `edit` whose editor stays open does not keep a second command from
/// queueing, nor the drain from landing it once the lock is free.
#[test]
fn a_spooled_edit_waiting_on_its_editor_holds_back_no_other_command() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    assert!(env.run(&["occur", "coffee"]).status.success());
    let hid = env.hid_of("coffee");
    let editor = env.home.join("slow-editor");
    std::fs::write(&editor, "#!/bin/sh\nsleep 6\n").unwrap();
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();

    let holder = env.hold();
    let mut editing = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", &env.db)
        .env("DATUM_HOME", &env.home)
        .env("DATUM_CONFIG_HOME", &env.home)
        .env("DATUM_EDITOR", &editor)
        // Killed below with its editor open, so its tempfile is never
        // dropped: keep it in the test's own dir, not /tmp (da-psyl3).
        .env("TMPDIR", &env.home)
        .env("NO_COLOR", "1")
        .args(["edit", &hid])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    // Past its wait for the lock and into the editor.
    std::thread::sleep(Duration::from_millis(1500));
    // Run against a deadline: the bug this guards is a wait on a person.
    let mut tea = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", &env.db)
        .env("DATUM_HOME", &env.home)
        .env("DATUM_CONFIG_HOME", &env.home)
        .env("NO_COLOR", "1")
        .args(["occur", "tea"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let began = Instant::now();
    while tea.try_wait().unwrap().is_none() {
        if began.elapsed() > Duration::from_secs(3) {
            tea.kill().unwrap();
            editing.kill().unwrap();
            panic!("a second command waited on the open editor");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = tea.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("QUEUED"));
    drop(holder);
    assert!(env.run(&["tail"]).status.success());
    assert!(env.rows().iter().any(|r| r.1 == "tea"), "the tea landed");
    assert!(
        editing.try_wait().unwrap().is_none(),
        "the editor is still open"
    );
    editing.kill().unwrap();
    editing.wait().unwrap();
}

/// Run `child` to its end within `secs`, or kill it and fail naming `what`.
fn finish(mut child: std::process::Child, secs: u64, what: &str) -> Output {
    let began = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if began.elapsed() > Duration::from_secs(secs) {
            child.kill().unwrap();
            panic!("{what} did not end in {secs} s");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().unwrap()
}

/// Wait for `path` to exist, as a script's signal.
fn appears(path: &std::path::Path) {
    let began = Instant::now();
    while !path.exists() {
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{path:?} never appeared"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

impl Env {
    fn spawn(&self, args: &[&str], editor: Option<&std::path::Path>) -> std::process::Child {
        use std::process::Stdio;
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("NO_COLOR", "1")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(e) = editor {
            c.env("DATUM_EDITOR", e);
        }
        c.spawn().unwrap()
    }

    /// Hold the spool's lock, as a spooler at work does.
    fn hold_spool(&self) -> std::fs::File {
        let f = std::fs::File::create(self.spool().with_extension("spool.lock")).unwrap();
        f.lock().unwrap();
        f
    }
}

/// An editor that counts its opens, edits `cups` 2 → 3, says it saved, and
/// stays open until `go` appears.
fn counting_editor(dir: &std::path::Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let editor = dir.join("editor");
    let script = format!(
        "#!/bin/sh\necho opened >> '{d}/opens'\n\
         sed 's/\"cups\": 2/\"cups\": 3/' \"$1\" > \"$1.new\" && mv \"$1.new\" \"$1\"\n\
         touch '{d}/saved'\nwhile [ ! -f '{d}/go' ]; do sleep 0.05; done\n",
        d = dir.display()
    );
    std::fs::write(&editor, script).unwrap();
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
    editor
}

/// An edit runs on a snapshot from the start (vividn, 15:31:13: «it shouldn't
/// rerun anything. It should just write the database at the right spot»):
/// the lock taken while the editor is open leaves the editor opened once,
/// and what was saved is what queues and lands.
#[test]
fn an_edit_whose_save_meets_the_lock_opens_the_editor_once() {
    let env = Env::new();
    assert!(env.run(&["occur", "coffee", "cups=2"]).status.success());
    let hid = env.hid_of("coffee");
    let dir = env.home.clone();
    let editing = env.spawn(&["edit", &hid], Some(&counting_editor(&dir)));
    appears(&dir.join("saved"));
    let holder = env.hold();
    std::fs::write(dir.join("go"), "").unwrap();
    let out = finish(editing, 10, "the edit");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(err.contains("is queued in"), "{err}");
    let opens = std::fs::read_to_string(dir.join("opens")).unwrap();
    assert_eq!(opens.lines().count(), 1, "the editor opened {opens:?}");
    drop(holder);

    assert!(env.run(&["tail"]).status.success());
    assert_eq!(env.rows()[0].4.as_deref(), Some(r#"{"cups":3}"#));
    env.check_sync_clean(1);
}

/// A question about a stale `_LAST` is about the entry the person was
/// shown, and the change lands there: not on the entry another process
/// moved `_LAST` to before the write could take the lock.
#[test]
fn a_question_about_last_lands_on_the_entry_the_person_saw() {
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};
    let env = Env::new();
    assert!(env.run(&["occur", "coffee", "cups=2"]).status.success());
    // `_LAST` an hour old: a bare update asks before it writes.
    Connection::open(&env.db)
        .unwrap()
        .execute(
            "UPDATE last_docs SET time = ?1",
            [(Utc::now() - chrono::Duration::hours(1)).to_rfc3339()],
        )
        .unwrap();

    let mut asking = env.spawn(&["update", "cups=5"], None);
    let said = Arc::new(Mutex::new(String::new()));
    let mut err = asking.stderr.take().unwrap();
    let heard = said.clone();
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        while let Ok(n) = err.read(&mut buf) {
            if n == 0 {
                break;
            }
            heard
                .lock()
                .unwrap()
                .push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    });
    let began = Instant::now();
    while !said.lock().unwrap().contains("[Y/n]") {
        assert!(began.elapsed() < Duration::from_secs(10), "never asked");
        std::thread::sleep(Duration::from_millis(20));
    }
    // Another process logs (moving `_LAST` to its tea) and then holds the
    // lock, while the person reads the question about the coffee.
    env.ok(&["occur", "tea"]);
    let holder = env.hold();
    let mut answer = asking.stdin.take().unwrap();
    answer.write_all(b"y\n").unwrap();
    drop(answer);
    let out = finish(asking, 10, "the update");
    reader.join().unwrap();
    let said = said.lock().unwrap().clone();
    assert!(out.status.success(), "{said}");
    assert!(said.contains("is queued in"), "{said}");
    assert_eq!(said.matches("[Y/n]").count(), 1, "asked again: {said}");
    drop(holder);

    assert!(env.run(&["tail"]).status.success());
    let data = |field: &str| env.rows().into_iter().find(|r| r.1 == field).unwrap().4;
    assert_eq!(data("coffee").as_deref(), Some(r#"{"cups":5}"#));
    assert_eq!(data("tea"), None, "the change went to the entry moved to");
    env.check_sync_clean(1);
}

/// An edit made while another copy edited the same entry is a sync
/// conflict: the snapshot it was made on had not seen the other edit, and
/// its ops land with the `deps` of that snapshot.
#[test]
fn an_edit_and_a_peers_edit_meanwhile_are_a_conflict() {
    let (a, b) = (Env::new(), Env::new());
    a.ok(&["occur", "coffee", "cups=2"]);
    a.send(&b);
    let hid = a.hid_of("coffee");
    let dir = a.home.clone();
    let editing = a.spawn(&["edit", &hid], Some(&counting_editor(&dir)));
    appears(&dir.join("saved"));
    // While the editor is open, the other copy's edit reaches this one.
    b.ok(&["update", &hid, "cups=5"]);
    b.send(&a);
    std::fs::write(dir.join("go"), "").unwrap();
    let out = finish(editing, 10, "the edit");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    a.send(&b);
    let conflicts = format!("SELECT conflicts FROM entries WHERE hid = '{hid}'");
    let sides: Value = serde_json::from_str(&a.query(&conflicts).unwrap()).unwrap();
    let mut values: Vec<i64> = sides["data.cups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["value"].as_i64().unwrap())
        .collect();
    values.sort();
    assert_eq!(values, [3, 5]);
    assert_eq!(a.query(&conflicts), b.query(&conflicts));
    for copy in [&a, &b] {
        copy.check_sync_clean(2);
    }
}

/// A local write committed between a line's snapshot and its drain moves
/// the seqs the queued ops are signed at. Their `deps` name this copy's ops
/// by seq, but an op of this copy saw every earlier op of it whatever its
/// deps say (`Op::saw`), so nothing is miscounted: the queued update lands
/// on the queued entry with no conflict. And a write that finds changes
/// still queued queues behind them, so this copy's ops keep the order the
/// commands ran in.
#[test]
fn a_local_write_between_the_queue_and_the_drain_shifts_no_dep() {
    let env = Env::new();
    assert!(env.run(&["occur", "coffee"]).status.success());
    let holder = env.hold();
    env.queued(&["occur", "tea"]);
    // A bare update may ask, so it runs on a snapshot and queues its ops.
    let out = env.run(&["update", "size=large"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("is queued in"));
    drop(holder);
    let spooled_seq = |slug: &str| -> i64 {
        let text = std::fs::read_to_string(env.spool()).unwrap();
        text.lines()
            .flat_map(|l| {
                serde_json::from_str::<Value>(l).unwrap()["ops"]
                    .as_array()
                    .unwrap()
                    .clone()
            })
            .find(|o| o["slug"].as_str().unwrap().starts_with(slug) && o["kind"] == "create")
            .unwrap()["seq"]
            .as_i64()
            .unwrap()
    };
    let tea_on_snapshot = spooled_seq("tea:");

    // The drain steps aside (a spooler holds the spool), and a command
    // that does not spool commits this copy's ops in between.
    let spooling = env.hold_spool();
    let kit = env.home.join("k.toml");
    std::fs::write(
        &kit,
        "[kit]\nname = \"k\"\n\n[[field]]\nname = \"nap\"\nverb = \"start\"\n",
    )
    .unwrap();
    env.ok(&["kit", "install", &kit.to_string_lossy()]);
    // A write behind queued changes queues, rather than landing first.
    let juice = env.spawn(&["occur", "juice"], None);
    std::thread::sleep(Duration::from_millis(300));
    drop(spooling);
    let out = finish(juice, 10, "the juice");
    assert!(String::from_utf8_lossy(&out.stdout).contains("QUEUED"));

    assert!(env.run(&["tail"]).status.success());
    let seq = |slug: &str| -> i64 {
        env.query(&format!(
            "SELECT CAST(seq AS TEXT) FROM ops WHERE slug LIKE '{slug}%' AND kind = 'create'"
        ))
        .unwrap()
        .parse()
        .unwrap()
    };
    assert!(
        seq("tea:") > tea_on_snapshot,
        "the kit's ops moved the queued seqs"
    );
    assert!(
        seq("tea:") < seq("juice:"),
        "the juice landed after the tea"
    );
    let tea = env.hid_of("tea");
    let row = format!("SELECT data FROM entries WHERE hid = '{tea}'");
    assert_eq!(env.query(&row).as_deref(), Some(r#"{"size":"large"}"#));
    let conflicts = format!("SELECT conflicts FROM entries WHERE hid = '{tea}'");
    assert_eq!(env.query(&conflicts), None);
    env.check_sync_clean(1);
}

/// A line that fails for a reason other than the lock holds every later
/// one behind it (da-3j6g7): `check` names it, when it was queued and why,
/// as an error, `sync status` says so too, and `check spool --fix` sets it
/// aside so the rest land.
#[test]
fn a_stuck_line_is_a_check_error_and_fix_sets_it_aside() {
    let env = Env::new();
    env.ok(&["occur", "coffee"]);
    let holder = env.hold();
    env.queued(&["occur", "tea"]);
    drop(holder);
    let stuck =
        r#"{"at":"2026-10-15T10:00:00+00:00","id":"stuck","last":null,"ops":[{"bogus":1}]}"#;
    let queued = std::fs::read_to_string(env.spool()).unwrap();
    std::fs::write(env.spool(), format!("{stuck}\n{queued}")).unwrap();
    let out = env.run(&["tail"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("cannot land") && err.contains("datum check spool"),
        "{err}"
    );

    let out = env.run(&["check", "spool"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("[ERROR] spool_stuck (x2)"), "{text}");
    assert!(
        text.contains("queued 2026-10-15T10:00:00+00:00, fails: ")
            && text.contains("missing field"),
        "{text}"
    );
    let status = env.ok(&["sync", "status"]);
    assert!(
        status.contains("⚠ 2 change(s) queued in") && status.contains("cannot land"),
        "{status}"
    );

    let out = env.run(&["check", "spool", "--fix"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("[WARN ] spool_set_aside") && !text.contains("spool_stuck"),
        "{text}"
    );
    let fields: Vec<String> = env.rows().into_iter().map(|r| r.1).collect();
    assert_eq!(fields, ["coffee", "tea"]);
    assert!(!env.spool().exists());
    let mut rejected = env.spool().into_os_string();
    rejected.push(".rejected");
    let rejected = std::fs::read_to_string(rejected).unwrap();
    assert!(rejected.ends_with(&format!("\t{stuck}\n")), "{rejected}");
    env.check_sync_clean(1);
}

/// A REPL line is a command (da-3j6g7): under a held lock a write prints
/// QUEUED instead of waiting the lock out, `check` names it waiting, and
/// it lands at the next command.
#[test]
fn a_repl_write_under_a_held_lock_is_queued_and_lands_later() {
    let env = Env::new();
    env.ok(&["occur", "coffee"]);
    let holder = env.hold();
    let began = Instant::now();
    let out = env.ok_in(&[], "occur tea\n");
    assert!(out.contains("QUEUED"), "{out}");
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "{:?}",
        began.elapsed()
    );
    assert_eq!(env.rows().len(), 1, "nothing lands while the lock is held");
    let text = env.ok(&["check", "spool"]);
    assert!(text.contains("[WARN ] spool_waiting (x1)"), "{text}");
    drop(holder);

    let out = env.run(&["tail"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("1 queued change(s) written"));
    let fields: Vec<String> = env.rows().into_iter().map(|r| r.1).collect();
    assert_eq!(fields, ["coffee", "tea"]);
    env.check_sync_clean(1);
}

/// da-owy96: a change queued before a purge is the old copy's data, so the
/// purge takes it with the rest and says so, and the next command lands
/// nothing.
#[test]
fn a_purge_removes_the_changes_queued_before_it() {
    let env = Env::new();
    env.ok(&["occur", "coffee"]);
    let holder = env.hold();
    env.queued(&["occur", "tea"]);
    drop(holder);

    let out = env.ok(&["db", "purge", "--yes"]);
    assert!(
        out.contains("removed: 1 queued change(s), never written"),
        "{out}"
    );
    assert!(!env.spool().exists(), "the spool outlived the purge");
    let out = env.run(&["tail"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("queued"), "{err}");
    assert_eq!(env.rows(), [], "the purge left an entry");
}

/// da-owy96: a write the purge's wipe turns away snapshots the copy only
/// once the wipe is done, so it queues nothing of the old copy: an update
/// of an entry the purge took finds none.
#[test]
fn a_write_during_a_purge_queues_nothing_of_the_old_copy() {
    let env = Env::new();
    env.ok(&["occur", "coffee"]);
    let hid = env.hid_of("coffee");
    let holder = env.hold();
    let purge = env.spawn(&["db", "purge", "--yes"], None);
    // Into the purge's wait for the write lock, holding the spool.
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(3) {
        let held = std::fs::File::open(env.spool().with_extension("spool.lock"))
            .is_ok_and(|f| f.try_lock().is_err());
        if held {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let update = env.spawn(&["update", &hid, "n=1"], None);
    // Past its wait for the write lock, and at the spool's.
    std::thread::sleep(Duration::from_millis(1500));
    drop(holder);
    let purged = finish(purge, 10, "the purge");
    assert!(
        purged.status.success(),
        "{}",
        String::from_utf8_lossy(&purged.stderr)
    );
    let out = finish(update, 10, "the update");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(!said.contains("QUEUED"), "queued on the old copy: {said}");
    let out = env.run(&["tail"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("queued"), "{err}");
    assert_eq!(env.rows(), [], "the purge left an entry");
}

impl Env {
    fn beside(&self, ext: &str) -> PathBuf {
        let mut p = self.spool().into_os_string();
        p.push(ext);
        p.into()
    }

    fn fields(&self) -> Vec<String> {
        self.rows().into_iter().map(|r| r.1).collect()
    }

    /// Cut the spool to half its bytes, as a crash mid-append leaves it:
    /// what was cut off.
    fn tear(&self) -> Vec<u8> {
        let bytes = std::fs::read(self.spool()).unwrap();
        let (kept, _) = bytes.split_at(bytes.len() / 2);
        std::fs::write(self.spool(), kept).unwrap();
        kept.to_vec()
    }
}

/// A torn last line (da-29om3) is set aside, and the writes after it queue
/// and land as if it were not there.
#[test]
fn a_torn_last_line_is_set_aside_and_later_writes_queue_and_land() {
    let env = Env::new();
    env.ok(&["occur", "coffee"]);
    let holder = env.hold();
    env.queued(&["occur", "walk"]);
    let torn = env.tear();

    let out = env.run(&["occur", "juice"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("QUEUED"));
    assert!(
        err.contains("set aside") && err.contains(".spool.torn"),
        "{err}"
    );
    let aside = std::fs::read(env.beside(".torn")).unwrap();
    assert!(aside.ends_with(&[&torn[..], b"\n"].concat()), "{aside:?}");
    drop(holder);

    env.ok(&["occur", "nap"]);
    assert_eq!(env.fields(), ["coffee", "juice", "nap"]);
    assert!(!env.spool().exists());
    env.check_sync_clean(1);
}

/// A line in the middle that does not parse is set aside, and the lines
/// either side of it land in order.
#[test]
fn a_garbage_line_in_the_middle_is_set_aside_and_the_rest_land_in_order() {
    let env = Env::new();
    env.ok(&["occur", "coffee"]);
    let holder = env.hold();
    env.queued(&["occur", "tea"]);
    let mut spool = std::fs::read(env.spool()).unwrap();
    spool.extend_from_slice(b"{\"id\": not json\n");
    std::fs::write(env.spool(), spool).unwrap();
    let out = env.run(&["occur", "cake"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("QUEUED"));
    assert!(
        err.contains("set aside") && err.contains(".spool.rejected"),
        "{err}"
    );
    drop(holder);

    env.ok(&["tail"]);
    assert_eq!(env.fields(), ["coffee", "tea", "cake"]);
    let aside = String::from_utf8(std::fs::read(env.beside(".rejected")).unwrap()).unwrap();
    assert!(aside.ends_with("\t{\"id\": not json\n"), "{aside}");
    assert!(!env.spool().exists());
    env.check_sync_clean(1);
}

/// The repro of da-29om3: a queued line torn, then three writes, the first
/// under the lock. Each lands; none is lost behind the torn line.
#[test]
fn writes_after_a_torn_line_are_not_lost() {
    let env = Env::new();
    let holder = env.hold();
    env.queued(&["occur", "walk"]);
    env.tear();
    env.queued(&["occur", "juice"]);
    drop(holder);
    env.ok(&["occur", "nap"]);
    env.ok(&["occur", "nap2"]);
    assert_eq!(env.fields(), ["juice", "nap", "nap2"]);
    assert!(env.beside(".torn").exists());
}

/// A torn line with a multibyte character cut in half is not UTF-8: it is
/// set aside like any other.
#[test]
fn a_torn_line_cut_mid_character_is_set_aside() {
    let env = Env::new();
    let holder = env.hold();
    env.queued(&["note", "café", "ééééééééééééééééééééééééééééééééééééééé"]);
    let bytes = std::fs::read(env.spool()).unwrap();
    let cut = bytes.iter().rposition(|b| *b == 0xc3).unwrap() + 1;
    std::fs::write(env.spool(), &bytes[..cut]).unwrap();
    env.queued(&["occur", "juice"]);
    drop(holder);
    env.ok(&["tail"]);
    assert_eq!(env.fields(), ["juice"]);
    assert!(env.beside(".torn").exists());
}

/// A queued line that reads but will not land holds no write back: the
/// snapshot stops short of it, and the command still queues its own line.
#[test]
fn a_line_that_will_not_land_does_not_stop_a_write_queueing() {
    let env = Env::new();
    env.ok(&["occur", "coffee"]);
    let holder = env.hold();
    std::fs::write(env.spool(), "{\"id\":\"stuck\",\"ops\":[{\"op\":1}]}\n").unwrap();
    let out = env.run(&["occur", "juice"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("QUEUED"));
    assert!(err.contains("ran without some changes queued"), "{err}");
    let spool = std::fs::read_to_string(env.spool()).unwrap();
    assert_eq!(spool.lines().count(), 2, "{spool}");
    assert!(
        spool.contains("\"stuck\"") && spool.contains("juice"),
        "{spool}"
    );
    drop(holder);

    // A drain that sets a line aside and then stops at the stuck one says
    // both.
    let mut torn = std::fs::read(env.spool()).unwrap();
    torn.extend_from_slice(b"{\"id\":\"to");
    std::fs::write(env.spool(), torn).unwrap();
    let out = env.run(&["tail"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(".spool.torn") && err.contains("cannot land"),
        "{err}"
    );
}

/// A purge counts every line of a damaged spool, the ones that do not read
/// too, and counting sets nothing aside: it reads under no lock of its own.
#[test]
fn a_purge_counts_the_lines_that_do_not_read_and_sets_none_aside() {
    let env = Env::new();
    env.ok(&["occur", "coffee"]);
    let holder = env.hold();
    env.queued(&["occur", "tea"]);
    drop(holder);
    let mut spool = std::fs::read(env.spool()).unwrap();
    spool.extend_from_slice(b"not json\n{\"id\":\"to");
    std::fs::write(env.spool(), spool).unwrap();

    let out = env.ok(&["db", "purge", "--yes"]);
    assert!(out.contains("removed: 3 queued change(s)"), "{out}");
    assert!(!env.beside(".rejected").exists() && !env.beside(".torn").exists());
}
