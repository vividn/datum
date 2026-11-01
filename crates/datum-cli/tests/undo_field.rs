//! `undo <field>` is that field's last action, and `undo a,b` both as one
//! (da-il29m). vividn, 2026-10-31: «undo <field_name> should undo the last
//! action on the field … So undo field1,field2 should undo both of them».
//! The real binary; the database is read back with SQLite itself.

use std::process::{Command, Output};

use tempfile::TempDir;

struct Env(TempDir);

impl Env {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    fn db(&self) -> std::path::PathBuf {
        self.0.path().join("data.db")
    }

    fn run(&self, line: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(line.split_whitespace())
            .env("DATUM_DB", self.db())
            .env("DATUM_HOME", self.0.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.0.path().join("config"))
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
    }

    fn ok(&self, line: &str) -> String {
        let out = self.run(line);
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "`{line}`: {err}");
        String::from_utf8(out.stdout).unwrap()
    }

    /// What `line` says as it fails, having changed no row.
    fn refused(&self, line: &str) -> String {
        let before = self.rows();
        let out = self.run(line);
        assert!(!out.status.success(), "`{line}` was not refused");
        assert_eq!(self.rows(), before, "`{line}` wrote");
        String::from_utf8(out.stderr).unwrap()
    }

    fn sql(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.db()).unwrap()
    }

    /// Every row, as `slug live`, tombstones and specs too.
    fn rows(&self) -> Vec<String> {
        let conn = self.sql();
        let mut stmt = conn
            .prepare(
                "SELECT slug, deleted_at IS NULL FROM entries
                 WHERE field NOT IN ('setting') ORDER BY slug",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok(format!(
                    "{} {}",
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?
                ))
            })
            .unwrap();
        rows.map(Result::unwrap).collect()
    }

    /// How many live entries `field` has, and whether its spec is live.
    fn live(&self, field: &str) -> (i64, bool) {
        let conn = self.sql();
        let count =
            |sql: &str, arg: &str| -> i64 { conn.query_row(sql, [arg], |r| r.get(0)).unwrap() };
        (
            count(
                "SELECT COUNT(*) FROM entries WHERE field = ?1 AND deleted_at IS NULL",
                field,
            ),
            count(
                "SELECT COUNT(*) FROM entries WHERE slug = 'spec:' || ?1 AND deleted_at IS NULL",
                field,
            ) == 1,
        )
    }

    fn hid(&self, field: &str) -> String {
        self.ok(&format!("get {field}:_::hid")).trim().to_string()
    }
}

#[test]
fn a_field_with_several_entries_walks_back_one_command_at_a_time() {
    let env = Env::new();
    env.ok("moment a -t 9:00");
    env.ok("moment a -t 9:10");
    env.ok("moment a -t 9:20");
    env.ok("moment other");

    env.ok("undo a");
    assert_eq!(env.live("a"), (2, true));
    env.ok("undo a");
    assert_eq!(env.live("a"), (1, true));
    assert_eq!(env.live("other"), (1, true), "another field stood");

    // The newest undo comes back first.
    env.ok("redo");
    assert_eq!(env.live("a"), (2, true));
    env.ok("redo");
    assert_eq!(env.live("a"), (3, true));
    env.refused("redo");
}

#[test]
fn a_field_with_one_entry_goes_by_the_same_rule_and_its_first_spec_with_it() {
    let env = Env::new();
    env.ok("moment a");
    env.ok("undo a");
    assert_eq!(
        env.live("a"),
        (0, false),
        "the spec its first log wrote went"
    );

    // Nothing left: named, and the entry does not come back by slug prefix.
    assert!(env.refused("undo a").contains("field 'a'"));
    assert_eq!(env.live("a"), (0, false));

    env.ok("redo");
    assert_eq!(env.live("a"), (1, true), "the spec came back with it");
}

#[test]
fn a_spec_written_on_its_own_is_the_fields_action_too() {
    let env = Env::new();
    env.ok("setup a --verb occur");
    env.ok("moment a");

    env.ok("undo a");
    assert_eq!(
        env.live("a"),
        (0, true),
        "the entry, and a declared spec stays"
    );
    env.ok("undo a");
    assert_eq!(env.live("a"), (0, false), "then the setup");
    assert!(env.refused("undo a").contains("field 'a'"));
}

#[test]
fn a_field_that_was_only_a_spec_is_still_named_once_it_is_gone() {
    let env = Env::new();
    env.ok("moment other");
    let hid = env.hid("other");
    // The name starts a live entry's hid, which must not be taken back for it.
    let name = &hid[..3];
    env.ok(&format!("setup {name}x --verb occur"));
    env.ok(&format!("undo {name}x"));
    assert_eq!(env.live(&format!("{name}x")), (0, false));

    let err = env.refused(&format!("undo {name}"));
    assert!(err.contains(&format!("field '{name}x'")), "{err}");
    assert_eq!(env.live("other"), (1, true));
}

#[test]
fn an_array_is_one_undo_and_one_redo_across_two_commands() {
    let env = Env::new();
    env.ok("moment a");
    env.ok("moment b");
    let hids = (env.hid("a"), env.hid("b"));
    env.ok("undo a,b");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (0, false)));

    env.ok("redo");
    assert_eq!((env.live("a"), env.live("b")), ((1, true), (1, true)));
    assert_eq!((env.hid("a"), env.hid("b")), hids, "under the same hids");
    env.refused("redo");
}

#[test]
fn an_array_of_one_commands_fields_is_that_command() {
    let env = Env::new();
    env.ok("moment a,b");
    env.ok("undo a,b");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (0, false)));
    env.refused("undo");
    env.ok("redo");
    assert_eq!((env.live("a"), env.live("b")), ((1, true), (1, true)));
}

#[test]
fn an_array_with_a_field_with_nothing_to_undo_writes_nothing_and_names_it() {
    let env = Env::new();
    env.ok("moment a");
    env.ok("moment c");
    env.ok("undo c");

    // `c` is all tombstones; `a` has a command to take back.
    let err = env.refused("undo a,c");
    assert!(
        err.contains("field 'c'") && err.contains("nothing was undone"),
        "{err}"
    );
    assert_eq!(env.live("a"), (1, true));
}

#[test]
fn undoing_a_field_of_a_batch_leaves_the_rest_for_a_bare_undo() {
    let env = Env::new();
    env.ok("moment a,b");

    env.ok("undo a");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (1, true)));

    // A bare undo takes back `b` and says nothing of `a`.
    let out = env.ok("undo");
    assert!(out.contains('b') && !out.contains("a:"), "{out}");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (0, false)));
    assert!(
        env.refused("undo").contains("Nothing to undo"),
        "no step left over"
    );

    // Each redo re-applies its own undo, newest first.
    env.ok("redo");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (1, true)));
    env.ok("redo");
    assert_eq!((env.live("a"), env.live("b")), ((1, true), (1, true)));
    env.refused("redo");

    // Whole again: a bare undo takes both.
    env.ok("undo");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (0, false)));
}

#[test]
fn a_field_undo_of_an_update_batch_writes_no_second_revert() {
    let env = Env::new();
    env.ok("moment a x=1");
    env.ok("moment b x=1");
    env.ok("update a:_,b:_ x=2");

    env.ok("undo a");
    let value = |field: &str| env.ok(&format!("get {field}:_::x")).trim().to_string();
    assert_eq!((value("a"), value("b")), ("1".into(), "2".into()));
    let a = env.hid("a");
    let trail = |hid: &str| -> i64 {
        env.sql()
            .query_row(
                "SELECT COUNT(*) FROM entry_history WHERE entry_hid = ?1",
                [hid],
                |r| r.get(0),
            )
            .unwrap()
    };
    let before = trail(&a);

    // The bare undo takes `b` alone: `a` is not moved again.
    env.ok("undo");
    assert_eq!((value("a"), value("b")), ("1".into(), "1".into()));
    assert_eq!(trail(&a), before);
}

#[test]
fn a_new_command_drops_the_redo_and_the_taken_entry_stays_taken() {
    let env = Env::new();
    env.ok("moment a,b");
    env.ok("undo a");
    env.ok("moment c");
    env.refused("redo");

    env.ok("undo");
    assert_eq!(env.live("c"), (0, false));
    env.ok("undo");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (0, false)));

    // Its redo is `b`'s alone: `a`'s was dropped by `moment c`.
    env.ok("redo");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (1, true)));
}

#[test]
fn redo_restores_in_the_order_of_the_undos_across_a_shared_entry() {
    let env = Env::new();
    env.ok("moment a x=1");
    env.ok("moment b x=1");
    env.ok("update a:_,b:_ x=2");
    env.ok("update a:_ x=3");
    let value = || env.ok("get a:_::x").trim().to_string();

    env.ok("undo a");
    assert_eq!(value(), "2");
    env.ok("undo a");
    assert_eq!(value(), "1");
    env.ok("redo");
    assert_eq!(value(), "2");
    env.ok("redo");
    assert_eq!(value(), "3");
}

#[test]
fn a_whole_hid_is_its_entry_and_a_field_wins_over_a_hid_it_starts() {
    let env = Env::new();
    env.ok("moment coffee");
    let hid = env.hid("coffee");

    // A field whose name starts the hid: the name is the field's.
    let field = format!("{}x", &hid[..3]);
    env.ok(&format!("moment {field}"));
    env.ok(&format!("undo {}", &hid[..3]));
    assert_eq!(env.live(&field), (0, false));
    assert_eq!(env.live("coffee"), (1, true));
    env.ok("redo");

    // A field named as a whole hid, which only a row from elsewhere can be:
    // the hid wins.
    let conn = env.sql();
    conn.execute(
        "UPDATE entries SET field = ?1, slug = ?1 || ':x' WHERE field = ?2",
        [&hid, &field],
    )
    .unwrap();
    drop(conn);
    env.ok(&format!("undo {hid}"));
    assert_eq!(env.live("coffee").0, 0, "the hid's entry went");
    assert_eq!(env.live(&hid).0, 1, "the field's stood");
}

#[test]
fn a_field_with_no_entries_and_a_name_nothing_starts_are_refused() {
    let env = Env::new();
    env.ok("moment a");
    env.refused("undo nosuchfield");
    let err = env.refused("undo a,nosuchfield");
    assert!(
        err.contains("'nosuchfield'") && err.contains("nothing was undone"),
        "{err}"
    );
    let hid = env.hid("a");
    env.ok("moment other");
    let err = env.refused(&format!("undo other,{hid}"));
    assert!(err.contains("give one or the other"), "{err}");
    assert!(env
        .refused("undo a --to-version 1")
        .contains("--to-version"));
}

#[test]
fn an_entry_already_undone_by_id_is_no_action_left_on_the_field() {
    let env = Env::new();
    env.ok("moment a -t 9:00");
    env.ok("moment a -t 9:10");
    let newest = env.hid("a");
    env.ok(&format!("undo {newest}"));
    assert_eq!(env.live("a"), (1, true));

    // The field's last standing change is the 9:00, not the one already gone.
    env.ok("undo a");
    assert_eq!(env.live("a"), (0, false));
}

#[test]
fn a_change_from_another_copy_is_refused_naming_it() {
    let env = Env::new();
    env.ok("moment a");
    let hid = env.hid("a");
    env.sql()
        .execute(
            "INSERT INTO entry_history
                 (entry_hid, version, snapshot, modify_time, client_id, change_kind)
             SELECT entry_hid, version + 1, snapshot, modify_time, client_id, 'sync_merge'
               FROM entry_history WHERE entry_hid = ?1 AND version = 1",
            [&hid],
        )
        .unwrap();
    let err = env.refused("undo a");
    assert!(
        err.contains("a sync brought in") && !err.contains("--skip-changed"),
        "{err}"
    );
}
