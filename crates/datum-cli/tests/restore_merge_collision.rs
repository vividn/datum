//! `datum db restore --merge` when a backup entry's identity is held by a
//! different live entry (da-0v1zt, da-4ldvb): the backup's entry comes in
//! as a sync from a copy frozen at the backup would bring it — a row and an
//! op of its own, which did not see the live entry's — so the fold merges
//! the two and names what they give differently in the entry's `conflicts`,
//! the same on every synced copy. The live entry keeps its identity, the
//! dry run counts what the run does, and one `undo` takes the whole restore
//! back. The real binary.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};
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

    fn path(&self, name: &str) -> String {
        self.tmp.path().join(name).to_string_lossy().into_owned()
    }

    /// Run `datum` on database `db`, asserting it succeeds; its stdout.
    fn ok(&self, db: &str, args: &[&str]) -> String {
        self.ok_in(db, args, "")
    }

    fn ok_in(&self, db: &str, args: &[&str], stdin: &str) -> String {
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
            .expect("spawn datum");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn conn(&self, db: &str) -> rusqlite::Connection {
        rusqlite::Connection::open(self.path(db)).unwrap()
    }

    fn query<T: rusqlite::types::FromSql>(&self, db: &str, sql: &str) -> Vec<T> {
        self.conn(db)
            .prepare(sql)
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// The entry `db` shows at `slug` — its hid, state and data — and how
    /// many live rows are under it.
    fn shown(&self, db: &str, slug: &str) -> (Value, usize) {
        let row = format!(
            "SELECT json_object('hid', hid, 'state', json(state), 'data', json(data))
             FROM entries WHERE slug = '{slug}' AND superseded_by IS NULL"
        );
        let under = format!(
            "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL AND superseded_by =
               (SELECT hid FROM entries WHERE slug = '{slug}' AND superseded_by IS NULL)"
        );
        (
            serde_json::from_str(&self.query::<String>(db, &row)[0]).unwrap(),
            self.query::<usize>(db, &under)[0],
        )
    }

    /// Each side of `key` in the `conflicts` of the entry shown at `slug`,
    /// as (device, value).
    fn sides(&self, db: &str, slug: &str, key: &str) -> Vec<(String, Value)> {
        let sql = format!(
            "SELECT conflicts FROM entries WHERE slug = '{slug}' AND superseded_by IS NULL"
        );
        let conflicts: Option<String> = self.query(db, &sql).pop().unwrap();
        let conflicts: Value = conflicts.map_or(Value::Null, |c| serde_json::from_str(&c).unwrap());
        let sides = conflicts[key].as_array().cloned().unwrap_or_default();
        sides
            .iter()
            .map(|s| {
                (
                    s["device"].as_str().unwrap().to_string(),
                    s["value"].clone(),
                )
            })
            .collect()
    }

    /// Merge `backup` into `db`: the dry run's JSON, then the run's.
    fn merge(&self, db: &str, backup: &str) -> (Value, Value) {
        let json = |dry: bool| {
            let mut args = vec!["db", "restore", backup, "--merge", "--show", "json"];
            if dry {
                args.push("--dry-run");
            }
            serde_json::from_str::<Value>(&self.ok(db, &args)).unwrap()
        };
        (json(true), json(false))
    }

    /// `db`'s backup, at a path.
    fn backup(&self, db: &str) -> String {
        let path = self.path(&format!("{db}.jsonl"));
        self.ok(db, &["db", "backup", &path]);
        path
    }

    /// Send `to` the ops of `from` it lacks.
    fn sync(&self, from: &str, to: &str) {
        let vector = self.ok(to, &["sync", "pipe", "vector"]);
        let ops = self.ok(from, &["sync", "pipe", "ops", "--since", vector.trim()]);
        self.ok_in(to, &["sync", "pipe", "apply"], &ops);
    }
}

/// The dry run's `would_*` counts are the run's.
fn assert_same_counts(dry: &serde_json::Value, real: &serde_json::Value) {
    for (would, did) in [
        ("insert", "inserted"),
        ("skip", "skipped"),
        ("update", "updated"),
    ] {
        assert_eq!(dry[format!("would_{would}")], real[did], "{did}");
    }
    assert_eq!(dry["merged"], real["merged"]);
}

/// X's and Y's `habit` for one day, created in that order, both take
/// `habit:2026-10-04`, each with a key the other lacks, so the two merged
/// are neither one's content; Y also has a `tea`. Each one's backup.
fn two_devices() -> (Env, String, String) {
    let env = Env::new();
    env.ok(
        "x.db",
        &["occur", "habit", "-d", "2026-10-04", "note=x", "own=1"],
    );
    std::thread::sleep(std::time::Duration::from_millis(5));
    env.ok(
        "y.db",
        &["occur", "habit", "-d", "2026-10-04", "note=y", "cup=2"],
    );
    env.ok("y.db", &["occur", "tea"]);
    let (x, y) = (env.backup("x.db"), env.backup("y.db"));
    (env, x, y)
}

const HABIT: &str = "habit:2026-10-04";

/// The live side by its device, the backup's as `backup`.
fn live_then_backup(sides: &[(String, Value)], live: &str, backup: &str) {
    let values: Vec<&Value> = sides.iter().map(|s| &s.1).collect();
    assert_eq!(values, [live, backup], "{sides:?}");
    assert_ne!(sides[0].0, "backup", "{sides:?}");
    assert_eq!(sides[1].0, "backup", "{sides:?}");
}

/// Merging Y's backup into X leaves one entry showing both — Y's `note`
/// over X's (the later-created), Y's `cup` beside X's `own` — and X's
/// `note`, which it lost, is a side of the entry's conflict.
#[test]
fn a_backups_entry_merges_with_the_live_one_and_the_clash_is_a_conflict() {
    let (env, _, backup) = two_devices();
    let before = env.shown("x.db", HABIT).0;

    let (dry, real) = env.merge("x.db", &backup);
    assert_same_counts(&dry, &real);
    // tea and tea's spec; habit's identical spec is already there.
    assert_eq!(
        (&real["inserted"], &real["skipped"]),
        (&2.into(), &1.into())
    );
    assert_eq!(real["merged"], json!([HABIT]));
    let (shown, under) = env.shown("x.db", HABIT);
    assert_eq!(under, 1, "the backup's row is under the live one");
    assert_eq!(shown["hid"], before["hid"], "the live entry's identity");
    assert_eq!(shown["data"], json!({ "note": "y", "cup": 2, "own": 1 }));
    live_then_backup(&env.sides("x.db", HABIT, "data.note"), "x", "y");
    assert_eq!(env.sides("x.db", HABIT, "data.cup"), [], "one side only");
    let listed = env.ok("x.db", &["conflicts"]);
    assert!(
        listed.contains("data.note") && listed.contains("backup"),
        "{listed}"
    );

    // Again: the backup's row is here, so nothing is added.
    let (dry, real) = env.merge("x.db", &backup);
    assert_same_counts(&dry, &real);
    assert_eq!(
        (&real["inserted"], &real["skipped"], &real["merged"]),
        (&0.into(), &4.into(), &json!([]))
    );
    assert_eq!(env.shown("x.db", HABIT), (shown, 1));
}

/// A backup's entry saying what the live entry's own row says — under what
/// a merge has since made it show, so no content hash matches — is one
/// event logged twice, and adds nothing.
#[test]
fn a_backup_entry_saying_what_the_live_ones_own_row_says_adds_nothing() {
    let (env, _, backup) = two_devices();
    env.ok(
        "z.db",
        &["occur", "habit", "-d", "2026-10-04", "note=x", "own=1"],
    );
    let same = env.backup("z.db");
    env.merge("x.db", &backup);
    let shown = env.shown("x.db", HABIT);

    let (dry, real) = env.merge("x.db", &same);
    assert_same_counts(&dry, &real);
    assert_eq!(real["merged"], json!([]));
    assert_eq!(env.shown("x.db", HABIT), shown);
}

/// The other way round the backup's entry is the earlier-created, which a
/// sync would give the identity. A restored row takes it from no live
/// entry: Y's stays the entry, and still the later-created, so the value
/// shown.
#[test]
fn the_live_entry_keeps_its_identity_from_an_earlier_created_backup_entry() {
    let (env, backup, _) = two_devices();
    let before = env.shown("y.db", HABIT).0;

    let (dry, real) = env.merge("y.db", &backup);
    assert_same_counts(&dry, &real);
    assert_eq!(real["merged"], json!([HABIT]));
    let (shown, under) = env.shown("y.db", HABIT);
    assert_eq!(under, 1);
    assert_eq!(shown["hid"], before["hid"]);
    assert_eq!(shown["data"], json!({ "note": "y", "cup": 2, "own": 1 }));
    live_then_backup(&env.sides("y.db", HABIT, "data.note"), "y", "x");
}

/// `conflicts resolve` settles it like any other, on every copy: a third
/// copy that syncs with X holds the same conflict, and the resolution it
/// makes clears X's.
#[test]
fn the_conflict_is_the_same_on_a_synced_copy_and_a_resolution_settles_both() {
    let (env, _, backup) = two_devices();
    env.sync("x.db", "z.db");
    env.merge("x.db", &backup);
    env.sync("x.db", "z.db");
    let conflicts = |db: &str| -> Vec<Option<String>> {
        env.query(
            db,
            "SELECT conflicts FROM entries WHERE slug = 'habit:2026-10-04' AND superseded_by IS NULL",
        )
    };
    assert!(conflicts("x.db")[0].is_some());
    assert_eq!(conflicts("x.db"), conflicts("z.db"));
    let verify = |db: &str| env.ok(db, &["sync", "pipe", "verify"]);
    assert_eq!(verify("x.db"), verify("z.db"));

    let hid = env.shown("z.db", HABIT).0["hid"]
        .as_str()
        .unwrap()
        .to_string();
    env.ok("z.db", &["conflicts", "resolve", &hid, "--use-a"]);
    env.sync("z.db", "x.db");
    assert_eq!(conflicts("x.db"), [None]);
    assert_eq!(env.shown("x.db", HABIT).0["data"]["note"], "x");
    assert_eq!(verify("x.db"), verify("z.db"));
}

/// A live entry older than its ops (an import's, or from before ops) is
/// signed as it stood before the backup's comes in, and the clash is named.
#[test]
fn a_live_entry_older_than_its_ops_names_the_clash_too() {
    let (env, _, backup) = two_devices();
    env.conn("x.db")
        .execute_batch("DELETE FROM ops; DELETE FROM sync_local;")
        .unwrap();
    env.merge("x.db", &backup);
    live_then_backup(&env.sides("x.db", HABIT, "data.note"), "x", "y");
}

/// What `db` shows of `habit`, to the byte, and its conflicts.
fn habit(env: &Env, db: &str) -> (String, Vec<(String, Value)>) {
    (
        env.ok(db, &["get", HABIT, "--show", "json"]),
        env.sides(db, HABIT, "data.note"),
    )
}

/// One `undo` takes the whole restore back, a restore that only merged
/// included: the entry merged into is as it was and its conflict gone, the
/// entries added are gone, no live row is left under another. `redo` merges
/// again, conflict and all, and a second `undo` goes on to the command
/// before the restore.
#[test]
fn one_undo_takes_the_whole_restore_back_and_redo_merges_again() {
    for (db, with_tea) in [("x.db", true), ("y.db", false)] {
        let (env, x, y) = two_devices();
        let backup = if with_tea { &y } else { &x };
        env.ok(db, &["update", HABIT, "z=9"]);
        let before = habit(&env, db);
        let tea = || {
            env.query::<String>(
                db,
                "SELECT slug FROM entries WHERE field = 'tea' AND deleted_at IS NULL",
            )
        };
        let teas = tea();

        env.merge(db, backup);
        let merged = habit(&env, db);
        assert_ne!(merged.0, before.0, "{db}: the restore merged into it");
        assert!(!merged.1.is_empty(), "{db}: and left a conflict to undo");
        assert_eq!(tea().len(), 1, "{db}: y's, added or its own");

        env.ok(db, &["undo"]);
        assert_eq!(habit(&env, db), before, "{db}: undo");
        assert_eq!(tea(), teas, "{db}: undo");
        assert_eq!(env.shown(db, HABIT).1, 0, "{db}: nothing left hidden");

        env.ok(db, &["redo"]);
        assert_eq!(habit(&env, db), merged, "{db}: redo");

        env.ok(db, &["undo"]);
        assert_eq!(habit(&env, db), before, "{db}: undo again");
        env.ok(db, &["undo"]);
        let data = &env.shown(db, HABIT).0["data"];
        assert_eq!(data["z"], Value::Null, "{db}: the update before it: {data}");
    }
}

/// The undo is ops like any other change, so a synced copy takes the
/// restore back too, and the two agree at every step.
#[test]
fn undo_of_the_restore_reaches_a_synced_copy() {
    let (env, _, backup) = two_devices();
    env.sync("x.db", "z.db");
    let before = env.shown("z.db", HABIT);
    let verify = |db: &str| env.ok(db, &["sync", "pipe", "verify"]);
    env.merge("x.db", &backup);
    env.sync("x.db", "z.db");
    assert_eq!(env.shown("z.db", HABIT).1, 1, "the restore reached z");
    for (step, merged) in [("undo", false), ("redo", true), ("undo", false)] {
        env.ok("x.db", &[step]);
        env.sync("x.db", "z.db");
        assert_eq!(verify("x.db"), verify("z.db"), "{step}");
        assert_eq!(env.shown("z.db", HABIT) != before, merged, "{step}");
        let sides = env.sides("z.db", HABIT, "data.note");
        assert_eq!(!sides.is_empty(), merged, "{step}: {sides:?}");
    }
}

/// Restoring Y's backup and then really syncing with Y leaves three rows
/// for one habit — X's, Y's own and the restored one — which both copies
/// fold alike whichever of X's and Y's was created first: Y's own may take
/// the identity from X's, as a sync gives it, and the restored row follows.
#[test]
fn a_real_sync_with_the_backups_device_converges() {
    for x_first in [true, false] {
        let env = Env::new();
        let log = |db: &str, own: &str| {
            let note = format!("note={}", &db[..1]);
            env.ok(db, &["occur", "habit", "-d", "2026-10-04", &note, own]);
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        match x_first {
            true => (log("x.db", "own=1"), log("y.db", "cup=2")),
            false => (log("y.db", "cup=2"), log("x.db", "own=1")),
        };
        env.merge("x.db", &env.backup("y.db"));
        env.sync("x.db", "y.db");
        env.sync("y.db", "x.db");
        env.sync("x.db", "y.db");

        let verify = |db: &str| env.ok(db, &["sync", "pipe", "verify"]);
        assert_eq!(verify("x.db"), verify("y.db"), "x first: {x_first}");
        for db in ["x.db", "y.db"] {
            let (shown, under) = env.shown(db, HABIT);
            assert_eq!(under, 2, "{db}, x first: {x_first}");
            let later = if x_first { "y" } else { "x" };
            assert_eq!(
                shown["data"],
                json!({ "note": later, "cup": 2, "own": 1 }),
                "{db}, x first: {x_first}"
            );
        }
    }
}

/// A kit's `spec:weighin` (its `kit_id`, a subfield) against the bare one a
/// first `occur weighin` wrote, the later-created: it declares nothing, so
/// the kit's subfield applies (da-m82n0).
#[test]
fn a_kit_spec_merges_into_an_automatic_one() {
    let env = Env::new();
    let kit = env.path("w.toml");
    std::fs::write(
        &kit,
        "[kit]\nname = \"w\"\n\n[[field]]\nname = \"weighin\"\nverb = \"occur\"\n\n\
         [field.subfields.kg]\nvalue_type = \"number\"\n",
    )
    .unwrap();
    env.ok("k.db", &["kit", "install", &kit, "--yes"]);
    let backup = env.backup("k.db");
    env.ok("p.db", &["occur", "weighin"]);

    let (dry, real) = env.merge("p.db", &backup);
    assert_same_counts(&dry, &real);
    assert_eq!(real["merged"], json!(["spec:weighin"]));
    let (shown, under) = env.shown("p.db", "spec:weighin");
    assert_eq!(under, 1);
    assert_eq!(shown["data"]["kit_id"], "w", "{shown}");
    assert_eq!(shown["data"]["subfields"]["kg"]["value_type"], "number");
    assert_eq!(env.sides("p.db", "spec:weighin", "data.subfields"), []);
}

/// The backup's entry whose hid a tombstone holds (retimed off the slug,
/// then deleted) comes in like any other, under a hid of its own like any
/// other: the tombstone's ops are not its, and the tombstone stays one.
#[test]
fn an_entry_whose_hid_a_tombstone_holds_merges_with_the_live_one() {
    let env = Env::new();
    env.ok("x.db", &["occur", "habit", "-d", "2026-10-04", "note=x"]);
    let backup = env.backup("x.db");
    let hid =
        env.query::<String>("x.db", "SELECT hid FROM entries WHERE field = 'habit'")[0].clone();
    env.ok("x.db", &["retime", &hid, "-d", "2026-10-05"]);
    env.ok("x.db", &["del", &hid]);
    std::thread::sleep(std::time::Duration::from_millis(5));
    env.ok("x.db", &["occur", "habit", "-d", "2026-10-04", "note=z"]);

    let (dry, real) = env.merge("x.db", &backup);
    assert_same_counts(&dry, &real);
    assert_eq!(real["merged"], json!([HABIT]));
    let (shown, under) = env.shown("x.db", HABIT);
    assert_eq!(under, 1);
    assert_ne!(shown["hid"], hid.as_str(), "the live entry's");
    assert_eq!(shown["data"]["note"], "z", "the later-created wins");
    live_then_backup(&env.sides("x.db", HABIT, "data.note"), "z", "x");
    let dead: Vec<bool> = env.query(
        "x.db",
        &format!("SELECT deleted_at IS NOT NULL FROM entries WHERE hid = '{hid}'"),
    );
    assert_eq!(dead, [true]);
}

/// Two `mood` states at one instant under two ids (`--id`): the live one
/// is kept and the backup's set aside, whichever was created first, and the
/// state set aside is a side of the kept one's conflict.
#[test]
fn of_two_states_at_one_instant_the_live_one_is_kept() {
    let env = Env::new();
    let at = ["-t", "2026-10-04 10:00"];
    env.ok(
        "x.db",
        &[&["switch", "mood", "happy", "--id", "a"][..], &at].concat(),
    );
    std::thread::sleep(std::time::Duration::from_millis(5));
    env.ok(
        "y.db",
        &[&["switch", "mood", "sad", "--id", "b"][..], &at].concat(),
    );
    let (x, y) = (env.backup("x.db"), env.backup("y.db"));

    for (db, backup, slug, live, restored) in [
        ("x.db", &y, "mood:a", "happy", "sad"),
        ("y.db", &x, "mood:b", "sad", "happy"),
    ] {
        let (dry, real) = env.merge(db, backup);
        assert_same_counts(&dry, &real);
        assert_eq!(real["merged"], json!([slug]), "{db}");
        let (shown, under) = env.shown(db, slug);
        assert_eq!((&shown["state"], under), (&json!(live), 1), "{db}");
        live_then_backup(&env.sides(db, slug, "state"), live, restored);
        let states: Vec<String> = env.query(
            db,
            "SELECT state FROM entries WHERE field = 'mood'
               AND superseded_by IS NULL AND deleted_at IS NULL",
        );
        assert_eq!(states, [format!(r#""{live}""#)], "{db}: one state there");

        env.ok(db, &["undo"]);
        assert_eq!(env.sides(db, slug, "state"), [], "{db}: undo");
        assert_eq!(env.shown(db, slug), (shown, 0), "{db}: undo");
    }
}
