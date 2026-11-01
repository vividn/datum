//! An array of fields on a write command is one atomic command (da-p2ro6):
//! vividn, 2026-10-31: «you should be able to occur an array of fields or
//! start an array of fields», «undo should always undo one whole atomic
//! command». The real binary; the database is read back with SQLite itself.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_datum"));
        command
            .env("DATUM_DB", self.tmp.path().join("data.db"))
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "`{}`: {err}", args.join(" "));
        String::from_utf8(out.stdout).unwrap()
    }

    /// What `args` says as it refuses, having written nothing at all.
    fn refused(&self, args: &[&str]) -> String {
        let before = self.rows();
        let out = self.run(args);
        assert!(
            !out.status.success(),
            "`{}` was not refused",
            args.join(" ")
        );
        assert_eq!(self.rows(), before, "`{}` wrote", args.join(" "));
        String::from_utf8(out.stderr).unwrap()
    }

    /// Every live row that is no spec or alias: field, at, state, data.
    fn rows(&self) -> Vec<[Option<String>; 4]> {
        let path = self.tmp.path().join("data.db");
        if !path.exists() {
            return Vec::new();
        }
        let conn = rusqlite::Connection::open(path).unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT field, at, state, data FROM entries
                 WHERE deleted_at IS NULL AND superseded_by IS NULL AND field NOT IN ('spec', 'alias')
                 ORDER BY create_time, field",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]))
            .unwrap();
        rows.map(Result::unwrap).collect()
    }

    fn fields(&self) -> Vec<String> {
        let rows = self.rows().into_iter();
        rows.map(|[field, ..]| field.unwrap()).collect()
    }
}

#[test]
fn each_command_takes_an_array_at_one_instant_and_one_undo_takes_it_back() {
    for (setup, line) in [
        (&[][..], &["moment", "a,b", "-t", "-8"][..]),
        (&[], &["occur", "a,b", "-q"]),
        (&[], &["start", "a,b"]),
        (&[], &["on", "a,b"]),
        (&["start", "a,b"], &["stop", "a,b"]),
        (&["start", "a,b"], &["end", "a,b"]),
        (&["start", "a,b"], &["off", "a,b"]),
        (&[], &["switch", "a,b", "x"]),
        (&[], &["flag", "a,b", "look", "-t", "-8"]),
    ] {
        let env = Env::new();
        if !setup.is_empty() {
            env.ok(setup);
        }
        let before = env.rows();
        env.ok(line);
        let after = env.rows();
        let new: Vec<_> = after[before.len()..].to_vec();
        assert_eq!(new.len(), 2, "{line:?}");
        assert_eq!(new[0][0].as_deref(), Some("a"), "{line:?}");
        assert_eq!(new[1][0].as_deref(), Some("b"), "{line:?}");
        assert_eq!(new[0][1], new[1][1], "{line:?}: the same instant");
        assert!(new[0][1].is_some(), "{line:?}");

        env.ok(&["undo"]);
        assert_eq!(env.rows(), before, "{line:?}: one undo");
        env.ok(&["redo"]);
        assert_eq!(env.rows(), after, "{line:?}: one redo");
    }
}

#[test]
fn the_rest_of_the_line_is_each_names() {
    let env = Env::new();
    env.ok(&["moment", "a,b", "-t", "9", "x=1"]);
    let rows = env.rows();
    assert_eq!(rows[0][1], rows[1][1]);
    assert_eq!(rows[0][3], rows[1][3]);
    assert!(
        rows[0][3].as_deref().unwrap().contains("\"x\":1"),
        "{rows:?}"
    );
}

#[test]
fn a_flag_stays_a_flag_and_says_so() {
    let env = Env::new();
    let said = env.ok(&["flag", "a,b", "look"]);
    assert_eq!(said.matches("FLAG ").count(), 2, "{said}");
    let conn = rusqlite::Connection::open(env.tmp.path().join("data.db")).unwrap();
    let flagged: i64 = conn
        .query_row(
            "SELECT count(*) FROM entries
             WHERE json_extract(extra_metadata, '$.flag') = 1 AND state = '\"look\"'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(flagged, 2);
}

#[test]
fn one_failure_writes_nothing_and_names_the_field() {
    let env = Env::new();
    env.ok(&["start", "a"]);
    let said = env.refused(&["stop", "a,b"]);
    assert!(
        said.contains("b:") && said.contains("not running"),
        "{said}"
    );
    // The same through the words that stand for a stop.
    let said = env.refused(&["switch", "a,b", "off"]);
    assert!(
        said.contains("b:") && said.contains("not running"),
        "{said}"
    );
    let said = env.refused(&["flag", "c,x:y"]);
    assert!(said.contains("x:y"), "{said}");

    env.ok(&["setup", "dose", "--verb", "occur", "-k", "amount"]);
    let said = env.refused(&["moment", "c,dose"]);
    assert!(said.contains("dose:") && said.contains("amount"), "{said}");
}

/// Alone, a stop of a field not running writes, and a duration says the
/// whole block in an array too, so neither is that failure.
#[test]
fn a_stop_that_needs_nothing_running_still_writes() {
    let env = Env::new();
    env.ok(&["stop", "b"]);
    env.ok(&["stop", "c,d", "30m"]);
    assert_eq!(env.fields(), ["b", "c", "d"]);
}

#[test]
fn a_bare_word_no_two_fields_read_alike_is_refused_naming_them() {
    let env = Env::new();
    env.ok(&["setup", "dose", "--verb", "occur", "-k", "amount"]);
    env.ok(&["setup", "weigh", "--verb", "occur", "-k", "grams"]);
    env.ok(&["setup", "more", "--verb", "occur", "-k", "amount"]);
    for (line, names) in [
        (&["moment", "dose,weigh", "5"][..], &["dose", "weigh"][..]),
        (&["moment", "dose,plain", "5"], &["dose", "plain"]),
        (&["moment", "plain,other", "5"], &["plain"]),
        (&["start", "dose,other", "5"], &["dose", "other"]),
    ] {
        let said = env.refused(line);
        assert!(said.contains("`5`"), "{line:?}: {said}");
        for name in names {
            assert!(said.contains(name), "{line:?}: {said}");
        }
    }
    // Named, or read alike, it is no guess.
    env.ok(&["moment", "dose,weigh", "amount=5", "grams=5"]);
    env.ok(&["moment", "dose,more", "5"]);
    env.ok(&["start", "p,q", "30m"]);
}

#[test]
fn a_switch_gives_each_the_whole_state() {
    let env = Env::new();
    env.ok(&["switch", "a,b", "x,y"]);
    let rows = env.rows();
    assert_eq!(rows.len(), 2);
    for row in rows {
        let state: serde_json::Value = serde_json::from_str(row[2].as_deref().unwrap()).unwrap();
        assert_eq!(state, serde_json::json!(["x", "y"]));
    }
}

#[test]
fn a_name_twice_is_one_entry() {
    let env = Env::new();
    env.ok(&["moment", "a,a"]);
    assert_eq!(env.fields(), ["a"]);
}

/// `moment a, b` is an array with an empty name, and never `b` as anything.
#[test]
fn a_comma_with_nothing_after_it_is_refused() {
    let env = Env::new();
    for line in [
        &["moment", "a,", "b"][..],
        &["moment", "a,"],
        &["moment", ",a"],
        &["moment", "a,,b"],
        &["start", "a,", "b"],
        &["flag", "a,", "b"],
    ] {
        let said = env.refused(line);
        assert!(
            said.contains("empty name") && said.contains("no spaces"),
            "{said}"
        );
    }
}

#[test]
fn a_typo_behaves_as_it_would_alone() {
    let env = Env::new();
    env.ok(&["moment", "coffe,tea"]);
    assert_eq!(env.fields(), ["coffe", "tea"]);

    // Where a name alone could not be made, neither is the array.
    let env = Env::new();
    env.ok(&["moment", "tea"]);
    env.ok(&["db", "config", "auto_setup", "false"]);
    let before = env.fields();
    let said = env.refused(&["moment", "coffe,tea"]);
    assert!(said.contains("coffe"), "{said}");
    assert_eq!(env.fields(), before);
}

#[test]
fn each_name_expands_its_alias_alone() {
    let env = Env::new();
    env.ok(&["setup", "coffee", "--verb", "occur"]);
    env.ok(&["alias", "set", "caf", "coffee"]);
    // A field's own word, an alias of it by a bare word, and no alias.
    env.ok(&["moment", "coffee,caf,tea"]);
    assert_eq!(env.fields(), ["coffee", "tea"]);

    env.ok(&["alias", "set", "outside", "switch environment outside"]);
    let said = env.refused(&["moment", "tea,outside"]);
    assert!(
        said.contains("outside") && said.contains("whole command"),
        "{said}"
    );
}

#[test]
fn an_alias_may_hold_an_array_and_the_call_site_fills_it() {
    let env = Env::new();
    env.ok(&["alias", "set", "ab", "moment a,b"]);
    env.ok(&["ab", "x=1"]);
    let rows = env.rows();
    assert_eq!(env.fields(), ["a", "b"]);
    assert_eq!(rows[0][3], rows[1][3]);

    env.ok(&["moment", "c,d", "--alias", "cd"]);
    assert!(env.ok(&["alias", "show", "cd"]).contains("moment c,d"));
}

#[test]
fn one_echo_says_every_entry() {
    let env = Env::new();
    let said = env.ok(&["moment", "a,b", "--show", "json"]);
    let echo: serde_json::Value = serde_json::from_str(said.trim()).unwrap();
    let docs = echo["docs"].as_array().unwrap();
    let fields: Vec<_> = docs.iter().map(|d| d["field"].as_str().unwrap()).collect();
    assert_eq!(fields, ["a", "b"], "{said}");
}

/// A fall-back night's question is the line's, not each name's, so the two
/// cannot land an hour apart: one answer on the pipe is enough.
#[test]
fn a_repeated_hour_is_asked_once() {
    let env = Env::new();
    let mut child = env
        .command()
        .env("TZ", "Europe/Berlin")
        .env("DATUM_NO_DEFAULT_ANSWER", "1")
        .args(["moment", "a,b", "-d", "2026-10-25", "-t", "02:30"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"2\n").unwrap();
    let out = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    let rows = env.rows();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0][1], rows[1][1]);
    assert!(
        rows[0][1]
            .as_deref()
            .unwrap()
            .starts_with("2026-10-25T01:30"),
        "{rows:?}"
    );
}

/// Each array on a line is asked of its own hour: an answer, or a time with
/// nothing to ask, is no answer for another.
#[test]
fn another_array_on_the_line_asks_its_own_hour() {
    let env = Env::new();
    let mut child = env
        .command()
        .env("TZ", "Europe/Berlin")
        .env("DATUM_NO_DEFAULT_ANSWER", "1")
        .args(["moment", "a,b", "-t", "9", ",", "moment", "c,d"])
        .args(["-d", "2026-10-25", "-t", "02:30"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"2\n").unwrap();
    let out = child.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    let rows = env.rows();
    for row in rows
        .iter()
        .filter(|r| matches!(r[0].as_deref(), Some("c" | "d")))
    {
        let at = row[1].as_deref().unwrap();
        assert!(at.starts_with("2026-10-25T01:30"), "{rows:?}");
    }
    assert_eq!(rows.len(), 4, "{rows:?}");
}

/// After `--` a word is as typed, so no array is read there and no field
/// holding a comma is made.
#[test]
fn past_a_double_dash_a_comma_is_no_array_and_no_field() {
    let env = Env::new();
    for line in [&["moment", "--", "x,y"][..], &["flag", "--", "x,y"]] {
        let said = env.refused(line);
        assert!(said.contains("cannot contain ','"), "{line:?}: {said}");
    }
}

/// `-u` takes back one field's line.
#[test]
fn what_takes_one_field_says_so() {
    let env = Env::new();
    let said = env.refused(&["moment", "a,b", "-u"]);
    assert!(said.contains("datum undo"), "{said}");
}

/// What a person typed is what is refused, with what to do, and never the JSON
/// the word was read as (da-0ggbe).
#[test]
fn one_field_commands_quote_the_word_and_say_what_to_do() {
    let env = Env::new();
    for (line, says) in [
        (&["add", "coffee,tea"][..], "`add coffee … , add tea …`"),
        (&["add", "coffee,tea", "x"], "`add coffee … , add tea …`"),
        (&["note", "coffee,tea"], "`note coffee … , note tea …`"),
        (&["add", "a,,b"], "`add a … , add b …`"),
        (&["setup", "a,b"], "`setup a … , setup b …`"),
        (
            &["setup", "a,b", "--states", "on"],
            "`setup a … , setup b …`",
        ),
    ] {
        let said = env.refused(line);
        assert!(said.contains(says), "{line:?}: {said}");
        assert!(said.contains("takes one field"), "{line:?}: {said}");
        assert!(said.contains(line[1]), "{line:?}: {said}");
        assert!(
            !said.contains('\\') && !said.contains('['),
            "{line:?}: {said}"
        );
    }
    // What it offers runs, once each side says what it needs.
    env.ok(&["add", "coffee", "x", ",", "add", "tea", "y"]);
    env.ok(&["note", "coffee", ",", "note", "tea"]);
    let line = [
        "setup", "c", "--verb", "moment", ",", "setup", "d", "--verb", "moment",
    ];
    env.ok(&line);
}

/// A comma word reaching the name check by any other way is quoted as typed:
/// no command here takes the list, and no field is made of its JSON.
#[test]
fn a_comma_word_is_refused_as_typed_and_never_stored_as_json() {
    let env = Env::new();
    for (line, word) in [
        (&["moment", "--", "x,y"][..], "x,y"),
        (&["start", "--", "x,y"], "x,y"),
        (&["moment", "x", "field=a,b"], "a,b"),
        (&["note", ",a"], ",a"),
    ] {
        let said = env.refused(line);
        let quoted = format!("cannot contain ',' (got \"{word}\")");
        assert!(said.contains(&quoted), "{line:?}: {said}");
    }
    env.ok(&["moment", "g"]);
    for word in ["field=a,b", "field=,a"] {
        let said = env.refused(&["update", "_LAST", word]);
        let typed = word.trim_start_matches("field=");
        let quoted = format!("cannot contain ',' (got \"{typed}\")");
        assert!(said.contains(&quoted), "{word}: {said}");
    }
}
