//! `datum rename OLD NEW --merge` (da-icna7): a mistyped field becomes the
//! one it was meant to be. vividn, 2026-10-31, on two entries meeting: «do the
//! conflicts path but list any conflicts. If the data is otherwise identical,
//! then there is no conflict and they can just become one entry». The real
//! binary.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use tempfile::TempDir;

const T: &str = "2026-03-01T10:00:00Z";

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self, db: &str) -> String {
        self.tmp.path().join(db).to_string_lossy().into_owned()
    }

    fn run(&self, db: &str, args: &[&str], stdin: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .arg("--db")
            .arg(self.path(db))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut pipe = child.stdin.take().unwrap();
        pipe.write_all(stdin.as_bytes()).unwrap();
        drop(pipe);
        child.wait_with_output().unwrap()
    }

    fn ok(&self, db: &str, args: &[&str]) -> String {
        let out = self.run(db, args, "");
        assert!(
            out.status.success(),
            "`datum {}`: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn query(&self, db: &str, sql: &str) -> Vec<String> {
        rusqlite::Connection::open(self.path(db))
            .unwrap()
            .prepare(sql)
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// The hid of `field`'s live entry at `at`.
    fn hid(&self, db: &str, field: &str, at: &str) -> String {
        self.query(
            db,
            &format!(
                "SELECT hid FROM entries WHERE field = '{field}' AND deleted_at IS NULL
                   AND occur_time = '{}'",
                at
            ),
        )
        .pop()
        .unwrap_or_else(|| panic!("{field} has no entry at {at}"))
    }

    /// `field`'s live entries, as `state` per row the stream shows.
    fn states(&self, db: &str, field: &str) -> Vec<String> {
        self.query(
            db,
            &format!(
                "SELECT COALESCE(state, 'null') FROM entries WHERE field = '{field}'
                   AND deleted_at IS NULL AND superseded_by IS NULL ORDER BY occur_time"
            ),
        )
    }

    /// What the whole table holds, row by row: a dry run writes none of it.
    fn dump(&self, db: &str) -> Vec<String> {
        let mut rows = self.query(
            db,
            "SELECT hid || slug || field || COALESCE(deleted_at, '') || modify_time
               || COALESCE(superseded_by, '') FROM entries",
        );
        rows.extend(self.query(db, "SELECT endpoint || seq FROM ops"));
        rows.sort();
        rows
    }

    /// `db`'s conflicts as `conflicts list --show json`.
    fn conflicts(&self, db: &str) -> Value {
        serde_json::from_str(&self.ok(db, &["conflicts", "list", "--show", "json"])).unwrap()
    }

    fn sync(&self, from: &str, to: &str) {
        let vector = self.ok(to, &["sync", "pipe", "vector"]);
        let ops = self.ok(from, &["sync", "pipe", "ops", "--since", vector.trim()]);
        let out = self.run(to, &["sync", "pipe", "apply"], &ops);
        assert!(out.status.success());
    }

    /// A row's columns, for two copies to agree on.
    fn rows(&self, db: &str) -> Vec<String> {
        let mut rows = self.query(
            db,
            "SELECT hid || '|' || slug || '|' || field || '|' || COALESCE(state, '') || '|'
               || COALESCE(data, '') || '|' || (deleted_at IS NOT NULL) || '|'
               || COALESCE(superseded_by, '') || '|' || COALESCE(conflicts, '') FROM entries",
        );
        rows.sort();
        rows
    }
}

/// `coffe` and `coffee` with an entry each at other instants, and one of each
/// at `T` saying `old` and `new`.
fn meeting(env: &Env, db: &str, old: &str, new: &str) {
    env.ok(db, &["occur", "coffe", "-t", "2026-03-01T08:00:00Z"]);
    env.ok(db, &["occur", "coffee", "-t", "2026-03-01T09:00:00Z"]);
    let (old, new) = (format!("cups={old}"), format!("cups={new}"));
    env.ok(db, &["occur", "coffe", "-t", T, &old]);
    env.ok(db, &["occur", "coffee", "-t", T, &new]);
}

#[test]
fn a_name_that_exists_is_refused_naming_merge_and_nothing_moves() {
    let env = Env::new();
    env.ok("a", &["occur", "coffe"]);
    env.ok("a", &["occur", "coffee", "-q"]);
    let before = env.dump("a");
    let out = env.run("a", &["rename", "coffe", "coffee"], "");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--merge"));
    assert_eq!(env.dump("a"), before);
}

#[test]
fn a_merge_moves_every_entry_keeping_its_id() {
    let env = Env::new();
    env.ok("a", &["occur", "coffe", "-t", "2026-03-01T08:00:00Z"]);
    env.ok("a", &["occur", "coffe", "-t", "2026-03-01T08:30:00Z"]);
    env.ok("a", &["occur", "coffee", "-t", "2026-03-01T09:00:00Z"]);
    let (one, two) = (
        env.hid("a", "coffe", "2026-03-01T08:00:00+00:00"),
        env.hid("a", "coffe", "2026-03-01T08:30:00+00:00"),
    );
    let said = env.ok("a", &["rename", "coffe", "coffee", "--merge"]);
    assert!(said.contains("coffe → coffee"), "{said}");
    assert_eq!(env.states("a", "coffe").len(), 0);
    assert_eq!(env.states("a", "coffee").len(), 3, "a track row of its own");
    for hid in [&one, &two] {
        let field = env.query(
            "a",
            &format!("SELECT field FROM entries WHERE hid = '{hid}'"),
        );
        assert_eq!(field, ["coffee"]);
    }
    let slug = env.query(
        "a",
        &format!("SELECT slug FROM entries WHERE hid = '{one}'"),
    );
    assert!(slug[0].starts_with("coffee:"), "{slug:?}");
    let check = env.ok("a", &["check", "sync"]);
    assert!(!check.contains("sync_stale_row"), "{check}");
}

#[test]
fn entries_that_differ_at_one_instant_both_live_and_are_listed() {
    let env = Env::new();
    meeting(&env, "a", "1", "2");
    let [was, onto] = [
        env.hid("a", "coffe", "2026-03-01T10:00:00+00:00"),
        env.hid("a", "coffee", "2026-03-01T10:00:00+00:00"),
    ];
    let said = env.ok("a", &["rename", "coffe", "coffee", "--merge"]);
    assert!(said.contains("conflict at coffee:"), "{said}");
    assert!(said.contains(&was) && said.contains(&onto), "{said}");
    assert!(said.contains("data.cups"), "{said}");
    // Both rows live, NEW's holding the slug, and the fold names the clash.
    let under = env.query(
        "a",
        &format!(
            "SELECT json_extract(data, '$.cups') || '' FROM entries
             WHERE superseded_by = '{onto}' AND deleted_at IS NULL"
        ),
    );
    assert_eq!(under, ["1"]);
    let list = env.conflicts("a");
    assert_eq!(list.as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list[0]["hid"], onto.as_str());
    let sides: Vec<_> = list[0]["conflicts"]["data.cups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["value"].clone())
        .collect();
    assert_eq!(sides.len(), 2, "{list}");
    assert!(sides.contains(&Value::from(1)) && sides.contains(&Value::from(2)));
    // The old row is a deletion the record keeps, saying where it went.
    let comment = env.query(
        "a",
        &format!("SELECT data FROM entries WHERE hid = '{was}' AND deleted_at IS NOT NULL"),
    );
    assert!(comment[0].contains("renamed into coffee"), "{comment:?}");
    let check = env.ok("a", &["check", "sync"]);
    assert!(!check.contains("sync_stale_row"), "{check}");
    // And it can be settled.
    env.ok("a", &["conflicts", "resolve", &onto, "cups=a"]);
    assert_eq!(env.conflicts("a").as_array().unwrap().len(), 0);
}

#[test]
fn identical_entries_become_one_the_older_staying() {
    for older_is_old in [false, true] {
        let env = Env::new();
        let (first, second) = match older_is_old {
            true => ("coffe", "coffee"),
            false => ("coffee", "coffe"),
        };
        env.ok("a", &["occur", first, "-t", T, "cups=1", "-z", "UTC"]);
        env.ok("a", &["occur", second, "-t", T, "cups=1", "-z", "UTC"]);
        let at = "2026-03-01T10:00:00+00:00";
        let (older, younger) = (env.hid("a", first, at), env.hid("a", second, at));
        let said = env.ok("a", &["rename", "coffe", "coffee", "--merge"]);
        assert!(said.contains("became one"), "{said}");
        assert!(!said.contains("conflict at"), "{said}");
        assert_eq!(env.hid("a", "coffee", at), older, "the older stays");
        let gone = env.query(
            "a",
            &format!("SELECT hid FROM entries WHERE hid = '{younger}' AND deleted_at IS NOT NULL"),
        );
        assert_eq!(
            gone,
            std::slice::from_ref(&younger),
            "a deletion the record keeps"
        );
        assert_eq!(env.conflicts("a").as_array().unwrap().len(), 0);
        let before = env.states("a", "coffee").len();
        // One undo takes it back, one redo makes it one again.
        env.ok("a", &["undo"]);
        assert_eq!(env.hid("a", first, at), older);
        assert_eq!(env.hid("a", second, at), younger);
        env.ok("a", &["redo"]);
        assert_eq!(env.hid("a", "coffee", at), older);
        assert_eq!(env.states("a", "coffee").len(), before);
    }
}

#[test]
fn the_new_names_spec_wins_and_a_difference_is_said() {
    let env = Env::new();
    env.ok("a", &["setup", "mood", "--verb", "switch"]);
    env.ok("a", &["setup", "moood", "--verb", "occur"]);
    env.ok("a", &["occur", "moood", "-t", T]);
    for args in [
        &["rename", "moood", "mood", "--merge", "--dry-run"][..],
        &["rename", "moood", "mood", "--merge"],
    ] {
        let said = env.ok("a", args);
        assert!(said.contains("spec:mood stays"), "{said}");
        assert!(
            said.contains("verb: moood said \"occur\", mood says \"switch\""),
            "{said}"
        );
    }
    let spec = env.ok("a", &["setup", "mood"]);
    assert!(spec.contains("switch"), "{spec}");
    let gone = env.query(
        "a",
        "SELECT hid FROM entries WHERE slug = 'spec:moood' AND deleted_at IS NULL",
    );
    assert!(gone.is_empty());
    assert!(
        !env.run("a", &["moood"], "").status.success(),
        "its alias went"
    );
    env.ok("a", &["mood", "good"]);
}

#[test]
fn a_dry_run_lists_what_the_run_does_and_writes_nothing() {
    let env = Env::new();
    meeting(&env, "a", "1", "2");
    env.ok(
        "a",
        &[
            "occur",
            "coffe",
            "-t",
            "2026-03-01T11:00:00Z",
            "cups=3",
            "-z",
            "UTC",
        ],
    );
    env.ok(
        "a",
        &[
            "occur",
            "coffee",
            "-t",
            "2026-03-01T11:00:00Z",
            "cups=3",
            "-z",
            "UTC",
        ],
    );
    let before = env.dump("a");
    let dry = env.ok("a", &["rename", "coffe", "coffee", "--merge", "--dry-run"]);
    assert_eq!(env.dump("a"), before, "a dry run writes nothing");
    assert!(dry.starts_with("[dry-run] would rename"), "{dry}");
    assert!(dry.contains("would become one"), "{dry}");
    assert!(dry.contains("conflict at coffee:"), "{dry}");
    assert!(dry.contains("data.cups"), "{dry}");
    let n = |s: &str| s.matches("conflict at").count();
    let run = env.ok("a", &["rename", "coffe", "coffee", "--merge"]);
    assert_eq!((n(&dry), n(&dry)), (n(&run), 1), "{run}");
}

#[test]
fn one_undo_takes_the_whole_merge_back_and_redo_merges_again() {
    let env = Env::new();
    meeting(&env, "a", "1", "2");
    let at = "2026-03-01T10:00:00+00:00";
    let (was, onto) = (env.hid("a", "coffe", at), env.hid("a", "coffee", at));
    let fields = || {
        env.query(
            "a",
            "SELECT field || ' ' || COALESCE(json_extract(data, '$.cups'), '') FROM entries
             WHERE deleted_at IS NULL AND superseded_by IS NULL AND field != 'spec'
             ORDER BY field, occur_time",
        )
    };
    let before = fields();
    env.ok("a", &["rename", "coffe", "coffee", "--merge"]);
    env.ok("a", &["undo"]);
    assert_eq!(fields(), before);
    assert_eq!(
        (env.hid("a", "coffe", at), env.hid("a", "coffee", at)),
        (was, onto.clone())
    );
    assert_eq!(env.conflicts("a").as_array().unwrap().len(), 0);
    env.ok("a", &["redo"]);
    assert_eq!(env.conflicts("a")[0]["hid"], onto.as_str());
    env.ok("a", &["undo"]);
    assert_eq!(fields(), before);
    let check = env.ok("a", &["check", "sync"]);
    assert!(!check.contains("sync_stale_row"), "{check}");
}

#[test]
fn a_deleted_row_on_the_slug_moves_aside_and_undo_brings_it_back() {
    let env = Env::new();
    env.ok("a", &["occur", "coffee", "-t", T, "cups=2"]);
    let dead = env.hid("a", "coffee", "2026-03-01T10:00:00+00:00");
    env.ok("a", &["delete", &dead]);
    env.ok("a", &["occur", "coffe", "-t", T, "cups=1"]);
    let was = env.hid("a", "coffe", "2026-03-01T10:00:00+00:00");
    env.ok("a", &["rename", "coffe", "coffee", "--merge"]);
    assert_eq!(
        env.hid("a", "coffee", "2026-03-01T10:00:00+00:00"),
        was,
        "it lands"
    );
    env.ok("a", &["undo"]);
    assert_eq!(env.hid("a", "coffe", "2026-03-01T10:00:00+00:00"), was);
    let slug = env.query(
        "a",
        &format!("SELECT slug FROM entries WHERE hid = '{dead}'"),
    );
    assert!(!slug[0].contains('~'), "{slug:?}");
}

#[test]
fn a_second_copy_ends_with_the_same_rows_and_undo_reaches_it() {
    let env = Env::new();
    meeting(&env, "a", "1", "2");
    env.ok(
        "a",
        &[
            "occur",
            "coffe",
            "-t",
            "2026-03-01T11:00:00Z",
            "cups=3",
            "-z",
            "UTC",
        ],
    );
    env.ok(
        "a",
        &[
            "occur",
            "coffee",
            "-t",
            "2026-03-01T11:00:00Z",
            "cups=3",
            "-z",
            "UTC",
        ],
    );
    env.sync("a", "b");
    assert_eq!(env.rows("a"), env.rows("b"));
    env.ok("a", &["rename", "coffe", "coffee", "--merge"]);
    env.sync("a", "b");
    assert_eq!(env.rows("a"), env.rows("b"));
    assert_eq!(env.conflicts("b").as_array().unwrap().len(), 1);
    assert!(!env.ok("b", &["check", "sync"]).contains("sync_stale_row"));
    env.ok("a", &["undo"]);
    env.sync("a", "b");
    assert_eq!(env.rows("a"), env.rows("b"));
    assert_eq!(env.conflicts("b").as_array().unwrap().len(), 0);
}

#[test]
fn an_entry_with_a_row_merged_into_it_is_refused_before_anything_is_written() {
    let env = Env::new();
    env.ok("a", &["occur", "coffe", "-t", T, "cups=1"]);
    env.ok("b", &["occur", "coffe", "-t", T, "cups=2"]);
    env.sync("b", "a");
    env.ok("a", &["occur", "coffee", "-t", T, "cups=3"]);
    let before = env.dump("a");
    let out = env.run("a", &["rename", "coffe", "coffee", "--merge"], "");
    assert!(!out.status.success());
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("merged into it"), "{said}");
    assert_eq!(env.dump("a"), before);
}

/// Two id templates: one state at one instant, under two slugs. They stay two
/// slugs, and a state there is still undefined, so the fold sets one aside and
/// names the clash, however alike they are.
#[test]
fn entries_sharing_only_an_instant_take_the_conflicts_path() {
    let env = Env::new();
    env.ok("a", &["switch", "mood", "good", "-t", T, "--id", "one"]);
    env.ok("a", &["switch", "moood", "good", "-t", T, "--id", "two"]);
    let said = env.ok("a", &["rename", "moood", "mood", "--merge"]);
    assert!(said.contains("conflict at mood:two"), "{said}");
    assert!(!said.contains("became one"), "{said}");
    let live = env.query(
        "a",
        "SELECT slug FROM entries WHERE field = 'mood' AND deleted_at IS NULL ORDER BY slug",
    );
    assert_eq!(live, ["mood:one", "mood:two"], "both alive");
}

/// With the write lock held the merge is queued as a command is, and lands as
/// the same rows on the next open.
#[test]
fn a_merge_queued_behind_the_lock_lands_as_it_said() {
    let env = Env::new();
    meeting(&env, "a", "1", "2");
    let held = rusqlite::Connection::open(env.path("a")).unwrap();
    held.execute_batch("BEGIN IMMEDIATE").unwrap();
    let spool = format!("{}.spool", env.path("a"));
    let dry = env.ok("a", &["rename", "coffe", "coffee", "--merge", "--dry-run"]);
    assert!(dry.contains("conflict at coffee:"), "{dry}");
    let queued = std::fs::read_to_string(&spool).unwrap_or_default();
    assert!(
        queued.trim().is_empty(),
        "a dry run queues nothing: {queued}"
    );
    let said = env.ok("a", &["rename", "coffe", "coffee", "--merge"]);
    assert!(said.contains("conflict at coffee:"), "{said}");
    assert!(!std::fs::read_to_string(&spool).unwrap().is_empty());
    held.execute_batch("ROLLBACK").unwrap();
    drop(held);
    env.ok("a", &["tail", "coffee"]);
    assert!(env.states("a", "coffe").is_empty());
    assert_eq!(env.conflicts("a").as_array().unwrap().len(), 1);
    assert!(!env.ok("a", &["check", "sync"]).contains("sync_stale_row"));
    assert!(!env.ok("a", &["check", "spool"]).contains("error"));
}

/// NEW's row has no id template (a row from before them), the older OLD one
/// does: it still lands on NEW's slug.
#[test]
fn an_older_entry_lands_on_the_slug_of_a_row_with_no_id_template() {
    let env = Env::new();
    env.ok("a", &["occur", "coffe", "-t", T, "cups=1", "-z", "UTC"]);
    env.ok("a", &["occur", "coffee", "-t", T, "cups=1", "-z", "UTC"]);
    rusqlite::Connection::open(env.path("a"))
        .unwrap()
        .execute(
            "UPDATE entries SET id_structure = NULL WHERE field = 'coffee'",
            [],
        )
        .unwrap();
    let was = env.hid("a", "coffe", "2026-03-01T10:00:00+00:00");
    env.ok("a", &["rename", "coffe", "coffee", "--merge"]);
    let slug = env.query(
        "a",
        &format!("SELECT slug FROM entries WHERE hid = '{was}'"),
    );
    assert!(slug[0].starts_with("coffee:"), "{slug:?}");
}
