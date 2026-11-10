//! `undo` is the whole last command and takes no name; `revert` takes hids or
//! fields, and is a command on the stack like any other (da-k5vg1). vividn,
//! 2026-11-01: «having undo always be just the last command, and then rather
//! "revert" being a command that takes an/several hids». The real binary; the
//! database is read back with SQLite itself.

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
fn undo_and_redo_refuse_a_name_and_undo_says_revert() {
    let env = Env::new();
    env.ok("moment a");
    let hid = env.hid("a");
    for line in [
        "undo a".to_string(),
        format!("undo {hid}"),
        "undo _LAST".into(),
    ] {
        let err = env.refused(&line);
        assert!(err.contains("datum revert"), "{line}: {err}");
    }
    assert!(env.refused("undo --to-version 1").contains("datum revert"));
    env.ok("undo");
    for line in ["redo a".to_string(), format!("redo {hid}")] {
        assert!(env.refused(&line).contains("bare `datum redo`"), "{line}");
    }
    assert!(env.refused("revert").contains("datum undo"));
}

#[test]
fn a_revert_of_one_field_of_a_batch_is_a_step_of_its_own() {
    let env = Env::new();
    env.ok("moment a,b");

    env.ok("revert a");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (1, true)));

    // A bare undo takes the revert back, and only that.
    env.ok("undo");
    assert_eq!((env.live("a"), env.live("b")), ((1, true), (1, true)));
    // The next takes the whole of the command.
    env.ok("undo");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (0, false)));
    assert!(env.refused("undo").contains("Nothing to undo"));

    // Redo walks them forward again, in order.
    env.ok("redo");
    assert_eq!((env.live("a"), env.live("b")), ((1, true), (1, true)));
    env.ok("redo");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (1, true)));
    env.refused("redo");
}

#[test]
fn a_list_of_hids_is_one_step() {
    let env = Env::new();
    env.ok("moment a x=1");
    env.ok("moment b x=1");
    env.ok("update a:_,b:_ x=2");
    let value = |field: &str| env.ok(&format!("get {field}:_::x")).trim().to_string();
    let (a, b) = (env.hid("a"), env.hid("b"));

    env.ok(&format!("revert {a},{b}"));
    assert_eq!((value("a"), value("b")), ("1".into(), "1".into()));

    env.ok("undo");
    assert_eq!((value("a"), value("b")), ("2".into(), "2".into()));
    env.ok("redo");
    assert_eq!((value("a"), value("b")), ("1".into(), "1".into()));
    env.refused("redo");
}

#[test]
fn a_revert_is_a_command_so_it_drops_the_redo() {
    let env = Env::new();
    env.ok("moment a");
    env.ok("moment b");
    env.ok("undo");
    env.ok("revert a");
    assert!(env.refused("redo").contains("Nothing to redo"));
}

#[test]
fn the_action_word_says_what_happened() {
    let env = Env::new();
    let word = |out: String| out.split_whitespace().next().unwrap().to_string();
    env.ok("moment other");

    // A first write taken back is deleted outright, under either command.
    env.ok("moment a x=1");
    assert_eq!(word(env.ok("revert a")), "UNDONE");
    // Restoring it is any other change.
    assert_eq!(word(env.ok("undo")), "REVERT");
    env.ok("moment b x=1");
    assert_eq!(word(env.ok("undo")), "UNDONE");

    // An update's, under either.
    env.ok("update a:_ x=2");
    assert_eq!(word(env.ok("undo")), "REVERT");
    env.ok("update a:_ x=2");
    assert_eq!(word(env.ok("revert a")), "REVERT");

    // A delete's, restored, under either.
    env.ok("delete a:_");
    assert_eq!(word(env.ok("undo")), "REVERT");
    let hid = env.hid("a");
    env.ok("delete a:_");
    assert_eq!(word(env.ok(&format!("revert {hid}"))), "REVERT");
    // Stepped back again it is deleted outright.
    assert_eq!(word(env.ok(&format!("revert {hid}"))), "UNDONE");
}

#[test]
fn a_hid_steps_to_its_previous_state_and_a_second_revert_puts_it_back() {
    let env = Env::new();
    env.ok("moment a x=1");
    env.ok("update a:_ x=2");
    let hid = env.hid("a");
    let value = || env.ok("get a:_::x").trim().to_string();

    env.ok(&format!("revert {hid}"));
    assert_eq!(value(), "1");
    env.ok(&format!("revert {hid}"));
    assert_eq!(value(), "2");
    env.ok(&format!("revert {hid} --to-version 1"));
    assert_eq!(value(), "1");
    env.ok("undo");
    assert_eq!(value(), "2");
}

#[test]
fn a_field_with_several_entries_walks_back_one_command_at_a_time() {
    let env = Env::new();
    env.ok("moment a -y -t 9:00");
    env.ok("moment a -y -t 9:10");
    env.ok("moment a -y -t 9:20");
    env.ok("moment other");

    env.ok("revert a");
    assert_eq!(env.live("a"), (2, true));
    env.ok("revert a");
    assert_eq!(env.live("a"), (1, true));
    assert_eq!(env.live("other"), (1, true), "another field stood");

    // Each revert is a command of its own: the newest comes back first.
    env.ok("undo");
    assert_eq!(env.live("a"), (2, true));
    env.ok("undo");
    assert_eq!(env.live("a"), (3, true));
}

#[test]
fn a_field_walks_back_through_updates_of_one_entry() {
    let env = Env::new();
    env.ok("moment a x=1");
    env.ok("update a:_ x=2");
    env.ok("update a:_ x=3");
    let value = || env.ok("get a:_::x").trim().to_string();

    env.ok("revert a");
    assert_eq!(value(), "2");
    env.ok("revert a");
    assert_eq!(value(), "1");
    env.ok("revert a");
    assert_eq!(env.live("a"), (0, false));
    assert!(env.refused("revert a").contains("field 'a'"));

    for want in ["1", "2", "3"] {
        env.ok("undo");
        assert_eq!(value(), want);
    }
}

#[test]
fn a_field_with_one_entry_goes_by_the_same_rule_and_its_first_spec_with_it() {
    let env = Env::new();
    env.ok("moment a");
    env.ok("revert a");
    assert_eq!(
        env.live("a"),
        (0, false),
        "the spec its first log wrote went"
    );

    // Nothing left: named, and the entry does not come back by slug prefix.
    assert!(env.refused("revert a").contains("field 'a'"));
    assert_eq!(env.live("a"), (0, false));

    env.ok("undo");
    assert_eq!(env.live("a"), (1, true), "the spec came back with it");
}

#[test]
fn a_spec_written_on_its_own_is_the_fields_action_too() {
    let env = Env::new();
    env.ok("setup a --verb occur");
    env.ok("moment a");

    env.ok("revert a");
    assert_eq!(
        env.live("a"),
        (0, true),
        "the entry, and a declared spec stays"
    );
    env.ok("revert a");
    assert_eq!(env.live("a"), (0, false), "then the setup");
    assert!(env.refused("revert a").contains("field 'a'"));
}

#[test]
fn a_field_that_was_only_a_spec_is_still_named_once_it_is_gone() {
    let env = Env::new();
    env.ok("moment other");
    let hid = env.hid("other");
    // The name starts a live entry's hid, which must not be taken back for it.
    let name = &hid[..3];
    env.ok(&format!("setup {name}x --verb occur"));
    env.ok(&format!("revert {name}x"));
    assert_eq!(env.live(&format!("{name}x")), (0, false));

    let err = env.refused(&format!("revert {name}"));
    assert!(err.contains(&format!("field '{name}x'")), "{err}");
    assert_eq!(env.live("other"), (1, true));
}

#[test]
fn two_fields_are_one_step_across_two_commands() {
    let env = Env::new();
    env.ok("moment a");
    env.ok("moment b");
    let hids = (env.hid("a"), env.hid("b"));
    env.ok("revert a,b");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (0, false)));

    env.ok("undo");
    assert_eq!((env.live("a"), env.live("b")), ((1, true), (1, true)));
    assert_eq!((env.hid("a"), env.hid("b")), hids, "under the same hids");
}

#[test]
fn a_list_with_a_field_with_nothing_to_revert_writes_nothing_and_names_it() {
    let env = Env::new();
    env.ok("moment a");
    env.ok("moment c");
    env.ok("revert c");

    // `c` is all tombstones; `a` has a command to take back.
    let err = env.refused("revert a,c");
    assert!(
        err.contains("field 'c'") && err.contains("nothing was reverted"),
        "{err}"
    );
    assert_eq!(env.live("a"), (1, true));
}

#[test]
fn a_whole_hid_is_its_entry_and_a_field_wins_over_a_hid_it_starts() {
    let env = Env::new();
    env.ok("moment coffee");
    let hid = env.hid("coffee");

    // A field whose name starts the hid: the name is the field's.
    let field = format!("{}x", &hid[..3]);
    env.ok(&format!("moment {field}"));
    env.ok(&format!("revert {}", &hid[..3]));
    assert_eq!(env.live(&field), (0, false));
    assert_eq!(env.live("coffee"), (1, true));
    env.ok("undo");

    // A field named as a whole hid, which only a row from elsewhere can be:
    // the hid wins.
    let conn = env.sql();
    conn.execute(
        "UPDATE entries SET field = ?1, slug = ?1 || ':x' WHERE field = ?2",
        [&hid, &field],
    )
    .unwrap();
    drop(conn);
    env.ok(&format!("revert {hid}"));
    assert_eq!(env.live("coffee").0, 0, "the hid's entry went");
    assert_eq!(env.live(&hid).0, 1, "the field's stood");
}

#[test]
fn a_field_with_no_entries_and_a_name_nothing_starts_are_refused() {
    let env = Env::new();
    env.ok("moment a");
    env.refused("revert nosuchfield");
    let err = env.refused("revert a,nosuchfield");
    assert!(
        err.contains("'nosuchfield'") && err.contains("nothing was reverted"),
        "{err}"
    );
    let hid = env.hid("a");
    env.ok("moment other");
    let err = env.refused(&format!("revert other,{hid}"));
    assert!(err.contains("give one or the other"), "{err}");
    assert!(env
        .refused("revert a --to-version 1")
        .contains("--to-version"));
}

#[test]
fn an_entry_already_reverted_by_id_is_no_action_left_on_the_field() {
    let env = Env::new();
    env.ok("moment a -y -t 9:00");
    env.ok("moment a -y -t 9:10");
    let newest = env.hid("a");
    env.ok(&format!("revert {newest}"));
    assert_eq!(env.live("a"), (1, true));

    // The field's last standing change is the 9:00, not the one already gone.
    env.ok("revert a");
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
    let err = env.refused("revert a");
    assert!(
        err.contains("a sync brought in") && !err.contains("--skip-changed"),
        "{err}"
    );
}

/// A database from schema 77, an action part taken back: its undo moves the rest
/// and leaves what was taken, and its redo too.
#[test]
fn a_part_taken_back_before_the_migration_stays_taken() {
    let env = Env::new();
    env.ok("moment a,b");
    env.ok("revert a");
    // As schema 77 left it: `a`'s rows back, by no action of their own, and held
    // out of their action. The migration writes this `skipped`.
    let conn = env.sql();
    conn.execute_batch(
        "DELETE FROM undo_actions WHERE id = (SELECT MAX(id) FROM undo_actions);
         UPDATE undo_actions SET skipped = (
             SELECT json_group_array(hid) FROM entries
              WHERE field = 'a' OR slug = 'spec:a');",
    )
    .unwrap();
    drop(conn);

    let out = env.ok("undo");
    assert!(out.contains('b') && !out.contains(" a "), "{out}");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (0, false)));
    env.ok("redo");
    assert_eq!((env.live("a"), env.live("b")), ((0, false), (1, true)));
}

/// A value that comes back is no reason to take a command twice: each `revert`
/// goes on past what the last took, and undoing one lets it be taken again.
#[test]
fn a_field_walks_back_past_a_value_it_held_before() {
    let env = Env::new();
    env.ok("moment a x=1");
    env.ok("update a:_ x=2");
    env.ok("update a:_ x=1");
    let value = || env.ok("get a:_::x").trim().to_string();

    env.ok("revert a");
    assert_eq!(value(), "2");
    env.ok("revert a");
    assert_eq!(value(), "1");
    env.ok("undo");
    assert_eq!(value(), "2");
    env.ok("revert a");
    assert_eq!(value(), "1");
    env.ok("revert a");
    assert_eq!(env.live("a"), (0, false));
}

/// A revert that moved nothing leaves `_LAST` where it was.
#[test]
fn a_revert_that_fails_keeps_last() {
    let env = Env::new();
    env.ok("moment a");
    let hid = env.hid("a");
    env.refused("revert nosuchfield");
    assert_eq!(env.ok("get _LAST::hid").trim(), hid);
}
