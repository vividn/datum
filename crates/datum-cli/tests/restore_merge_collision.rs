//! `datum db restore --merge` when a backup entry's identity is held by a
//! different live entry (da-0v1zt, the redo of da-orh2e): the backup's
//! entry is merged into the live one by the rule a sync merges two with —
//! the later-created wins a key both have — instead of the whole restore
//! aborting; the dry run counts what the run
//! does, and `undo` unmerges. The real binary, two databases.

use std::process::Command;

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
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .arg("--db")
            .arg(self.path(db))
            .args(args)
            .output()
            .expect("spawn datum");
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

    /// `db`'s rows at `slug` as (superseded, data).
    fn at(&self, db: &str, slug: &str) -> Vec<(bool, Option<String>)> {
        self.conn(db)
            .prepare("SELECT superseded_by IS NOT NULL, data FROM entries WHERE slug = ?1")
            .unwrap()
            .query_map([slug], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// Merge `backup` into `db`: the dry run's JSON, then the run's.
    fn merge(&self, db: &str, backup: &str) -> (serde_json::Value, serde_json::Value) {
        let json = |dry: bool| {
            let mut args = vec!["db", "restore", backup, "--merge", "--show", "json"];
            if dry {
                args.push("--dry-run");
            }
            serde_json::from_str::<serde_json::Value>(&self.ok(db, &args)).unwrap()
        };
        (json(true), json(false))
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
/// `habit:2026-10-04`. Merging Y's backup into X leaves one entry: Y's
/// `note` over X's (the later-created), Y's `cup` beside it.
fn two_devices() -> (Env, String) {
    let env = Env::new();
    env.ok("x.db", &["occur", "habit", "-d", "2026-10-04", "note=x"]);
    std::thread::sleep(std::time::Duration::from_millis(5));
    env.ok(
        "y.db",
        &["occur", "habit", "-d", "2026-10-04", "note=y", "cup=2"],
    );
    env.ok("y.db", &["occur", "tea"]);
    let backup = env.path("y.jsonl");
    env.ok("y.db", &["db", "backup", &backup]);
    (env, backup)
}

#[test]
fn two_devices_same_day_entries_merge_into_one() {
    let (env, backup) = two_devices();

    let (dry, real) = env.merge("x.db", &backup);
    assert_same_counts(&dry, &real);
    // tea and tea's spec; habit's identical spec is already there.
    assert_eq!(
        (&real["inserted"], &real["skipped"]),
        (&2.into(), &1.into())
    );
    assert_eq!(real["merged"], serde_json::json!(["habit:2026-10-04"]));
    assert!(env.ok("x.db", &["tail"]).contains("tea"));
    let rows = env.at("x.db", "habit:2026-10-04");
    assert_eq!(rows.len(), 1, "one entry: {rows:?}");
    let data: serde_json::Value = serde_json::from_str(rows[0].1.as_deref().unwrap()).unwrap();
    assert_eq!(
        (&rows[0].0, &data["note"], &data["cup"]),
        (&false, &"y".into(), &2.into()),
        "{data}"
    );

    // The live entry's trail is its create and the user's merge — no
    // sync happened (da-2t8mx).
    let kinds: Vec<String> = env
        .conn("x.db")
        .prepare(
            "SELECT h.change_kind FROM entry_history h JOIN entries e ON e.hid = h.entry_hid
             WHERE e.slug = 'habit:2026-10-04' ORDER BY h.version",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(kinds, ["create", "user_edit"], "the live entry's trail");

    // Again: everything is there, so nothing is added.
    let (dry, real) = env.merge("x.db", &backup);
    assert_same_counts(&dry, &real);
    assert_eq!(
        (&real["inserted"], &real["skipped"], &real["merged"]),
        (&0.into(), &4.into(), &serde_json::json!([]))
    );
    assert_eq!(env.at("x.db", "habit:2026-10-04").len(), 1);
}

/// One `undo` reverts the whole restore: the entry merged into is back as
/// it was, the entries added are gone, and nothing is left hidden
/// (da-qvhh4's case).
#[test]
fn undo_unmerges_the_restore() {
    let (env, backup) = two_devices();
    let get = || env.ok("x.db", &["get", "habit:2026-10-04", "--show", "json"]);
    let (before, tail) = (get(), env.ok("x.db", &["tail"]));

    env.merge("x.db", &backup);
    assert_ne!(get(), before, "the restore merged into it");
    env.ok("x.db", &["undo"]);

    assert_eq!(get(), before);
    assert_eq!(env.ok("x.db", &["tail"]), tail);
    let hidden: i64 = env
        .conn("x.db")
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE superseded_by IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hidden, 0);
}

/// A kit's `spec:weighin` (its `kit_id`, a subfield) against the bare one a
/// first `occur weighin` wrote before the kit was installed.
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
    let backup = env.path("k.jsonl");
    env.ok("k.db", &["db", "backup", &backup]);
    env.ok("p.db", &["occur", "weighin"]);

    let (dry, real) = env.merge("p.db", &backup);
    assert_same_counts(&dry, &real);
    assert_eq!(real["merged"], serde_json::json!(["spec:weighin"]));
    let rows = env.at("p.db", "spec:weighin");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].1.as_deref().unwrap_or("").contains("\"kit_id\""),
        "{rows:?}"
    );
}

/// The backup's entry whose hid a tombstone holds (retimed off the slug,
/// then deleted) merges into the live entry at the slug like any other:
/// the tombstone's ops are not its own, and no row is written for it.
#[test]
fn an_entry_whose_hid_a_tombstone_holds_merges_into_the_live_one() {
    let env = Env::new();
    env.ok("x.db", &["occur", "habit", "-d", "2026-10-04", "note=x"]);
    let backup = env.path("x.jsonl");
    env.ok("x.db", &["db", "backup", &backup]);
    let hid: String = env
        .conn("x.db")
        .query_row("SELECT hid FROM entries WHERE field = 'habit'", [], |r| {
            r.get(0)
        })
        .unwrap();
    env.ok("x.db", &["retime", &hid, "-d", "2026-10-05"]);
    env.ok("x.db", &["del", &hid]);
    std::thread::sleep(std::time::Duration::from_millis(5));
    env.ok("x.db", &["occur", "habit", "-d", "2026-10-04", "note=z"]);

    let (dry, real) = env.merge("x.db", &backup);
    assert_same_counts(&dry, &real);
    assert_eq!(real["merged"], serde_json::json!(["habit:2026-10-04"]));
    let rows = env.at("x.db", "habit:2026-10-04");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].1.as_deref().unwrap().contains(r#""note":"z""#),
        "the later-created wins: {rows:?}"
    );
    assert!(env.ok("x.db", &["tail"]).contains("habit"));
}

/// Two `mood` states at one instant under two ids (`--id`): the live one
/// is kept as it is — nothing merged, nothing for `undo` — and the restore
/// names it.
#[test]
fn a_state_at_a_live_states_instant_keeps_the_live_one() {
    let env = Env::new();
    let at = ["-t", "2026-10-04 10:00"];
    env.ok(
        "x.db",
        &[&["switch", "mood", "happy", "--id", "a"][..], &at].concat(),
    );
    env.ok(
        "y.db",
        &[&["switch", "mood", "sad", "--id", "b"][..], &at].concat(),
    );
    let backup = env.path("y.jsonl");
    env.ok("y.db", &["db", "backup", &backup]);

    let (dry, real) = env.merge("x.db", &backup);
    assert_same_counts(&dry, &real);
    assert_eq!(real["merged"], serde_json::json!(["mood:a"]));
    let rows = env.at("x.db", "mood:a");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(env.at("x.db", "mood:b").len(), 0, "no second row");
    let (state, versions): (String, i64) = env
        .conn("x.db")
        .query_row(
            "SELECT state, (SELECT COUNT(*) FROM entry_history WHERE entry_hid = hid)
             FROM entries WHERE slug = 'mood:a'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((state.as_str(), versions), (r#""happy""#, 1));
}
