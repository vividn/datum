//! `datum rename FIELD --state OLD NEW` (da-zbkqb): a state renamed
//! everywhere, its entries and the field's setup, in one write. vividn,
//! 2026-11-13: it «renames the state everywhere». The real binary.

use std::process::{Command, Output};

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    /// `mood` with states a and b: an entry of each, one of both, an end
    /// leaving a, and a color, description, symbol and key on a.
    fn new() -> Self {
        let env = Self {
            tmp: tempfile::tempdir().unwrap(),
        };
        env.ok(&[
            "setup",
            "mood",
            "--kind",
            "change",
            "--states",
            "a,b",
            "--state-color",
            "a=red",
            "--state-description",
            "a=first",
            "--state-symbol",
            "a=A",
            "--state-key",
            "a=why=",
        ]);
        env.ok(&["change", "mood", "a", "-yy", "-t", "9:00"]);
        env.ok(&["change", "mood", "b", "-yy", "-t", "10:00"]);
        env.ok(&["change", "mood", "a,b", "-yy", "-t", "11:00"]);
        env.ok(&["end", "mood", "-yy", "-t", "12:00", "-l", "a"]);
        env
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("NO_COLOR", "1")
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .arg("--db")
            .arg(self.tmp.path().join("x.db"))
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "`datum {}`: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// The error a refused command prints.
    fn refused(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "`datum {}` ran", args.join(" "));
        String::from_utf8(out.stderr).unwrap()
    }

    fn query(&self, sql: &str) -> Vec<String> {
        rusqlite::Connection::open(self.tmp.path().join("x.db"))
            .unwrap()
            .prepare(sql)
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    /// Every live row of mood and its spec, as stored.
    fn rows(&self) -> Vec<String> {
        self.query(
            "SELECT hid || ' ' || slug || ' ' || coalesce(state, '') || ' '
                    || coalesce(following_state, '') || ' '
                    || coalesce(user_asserted_last_state, '') || ' ' || coalesce(data, '')
               FROM entries WHERE (field = 'mood' OR slug = 'spec:mood')
                AND deleted_at IS NULL ORDER BY hid",
        )
    }

    /// The live rows that say `"<state>"` anywhere.
    fn saying(&self, state: &str) -> Vec<String> {
        self.rows()
            .into_iter()
            .filter(|r| r.contains(&format!("\"{state}\"")))
            .collect()
    }
}

#[test]
fn a_state_renames_in_every_entry_and_spec_key_and_one_undo_takes_it_back() {
    let env = Env::new();
    let before = env.rows();
    let out = env.ok(&["rename", "mood", "--state", "a", "c"]);
    assert!(out.contains("RENAME mood:a → mood:c: 3 entries"), "{out}");
    for key in [
        "allowed_states",
        "state_colors",
        "state_descriptions",
        "state_keys",
        "state_symbols",
    ] {
        assert!(out.contains(key), "{key}: {out}");
    }
    assert!(out.contains("alias a is now c"), "{out}");
    assert_eq!(env.saying("a"), Vec::<String>::new());
    // The three entries and the spec, and b where it was.
    assert_eq!(env.saying("c").len(), 4, "{:#?}", env.rows());
    let multi = env.query("SELECT state FROM entries WHERE state LIKE '[%' AND field = 'mood'");
    assert_eq!(multi, ["[\"c\",\"b\"]"]);
    let spec = env.query("SELECT data FROM entries WHERE slug = 'spec:mood'");
    let spec: serde_json::Value = serde_json::from_str(&spec[0]).unwrap();
    assert_eq!(spec["allowed_states"], serde_json::json!(["c", "b"]));
    for key in [
        "state_colors",
        "state_descriptions",
        "state_symbols",
        "state_keys",
    ] {
        assert!(spec[key].get("c").is_some(), "{key}: {spec}");
    }
    // Each row an edit in place: the same hids.
    let hids = |rows: &[String]| -> Vec<String> {
        rows.iter()
            .map(|r| r.split(' ').next().unwrap().to_string())
            .collect()
    };
    assert_eq!(hids(&env.rows()), hids(&before));
    // The derived alias moved: `c` logs the state.
    let logged = env.ok(&["c"]);
    assert!(logged.contains("mood"), "{logged}");
    env.ok(&["undo"]);
    env.ok(&["undo"]);
    assert_eq!(env.rows(), before);
}

#[test]
fn an_alias_written_by_hand_is_listed_and_kept() {
    let env = Env::new();
    env.ok(&["alias", "set", "chill", "change mood a"]);
    let out = env.ok(&["rename", "mood", "--state", "a", "c"]);
    assert!(
        out.contains("alias chill still names a: «change mood a»"),
        "{out}"
    );
    let alias = env.ok(&["alias", "list"]);
    assert!(alias.contains("change mood a"), "{alias}");
}

#[test]
fn a_dry_run_writes_nothing_and_says_what_would_change() {
    let env = Env::new();
    let before = env.rows();
    let ops = env.query("SELECT count(*) || '' FROM entry_history");
    let out = env.ok(&["rename", "mood", "--state", "a", "c", "--dry-run"]);
    assert!(
        out.contains("would rename mood:a → mood:c: 3 entries"),
        "{out}"
    );
    assert!(out.contains("state_colors"), "{out}");
    assert_eq!(env.rows(), before);
    assert_eq!(env.query("SELECT count(*) || '' FROM entry_history"), ops);
}

#[test]
fn a_new_name_the_field_knows_is_refused_unless_merge() {
    let env = Env::new();
    let before = env.rows();
    let err = env.refused(&["rename", "mood", "--state", "a", "b"]);
    assert!(
        err.contains("already a state of `mood`") && err.contains("--merge"),
        "{err}"
    );
    assert_eq!(env.rows(), before);

    // A state only a deleted entry held counts too.
    let hid = env.query("SELECT hid FROM entries WHERE field = 'mood' AND state = '\"b\"'");
    env.ok(&["delete", &hid[0]]);
    env.ok(&["setup", "mood", "--states", "a"]);
    let err = env.refused(&["rename", "mood", "--state", "a", "b"]);
    assert!(err.contains("deleted entries count"), "{err}");
}

#[test]
fn merge_makes_old_entries_new_and_new_spec_keys_win() {
    let env = Env::new();
    env.ok(&["setup", "mood", "--state-color", "b=blue"]);
    let color = |state: &str| {
        let spec = env.query("SELECT data FROM entries WHERE slug = 'spec:mood'");
        let spec: serde_json::Value = serde_json::from_str(&spec[0]).unwrap();
        spec["state_colors"][state].clone()
    };
    let blue = color("b");
    let lists = env.query("SELECT hid FROM entries WHERE field = 'mood' AND state LIKE '[%'");
    assert!(!lists.is_empty());
    let out = env.ok(&["rename", "mood", "--state", "a", "b", "--merge"]);
    assert!(out.contains("RENAME mood:a → mood:b: 3 entries"), "{out}");
    assert!(
        out.contains("state_colors: a said \"#") && out.contains("which stays"),
        "{out}"
    );
    assert_eq!(env.saying("a"), Vec::<String>::new());
    let spec = env.query("SELECT data FROM entries WHERE slug = 'spec:mood'");
    let spec: serde_json::Value = serde_json::from_str(&spec[0]).unwrap();
    assert_eq!(spec["allowed_states"], serde_json::json!(["b"]));
    assert_eq!(color("b"), blue);
    assert_eq!(spec["state_descriptions"]["b"], "first");
    // `[a, b]` is b as a log stores it: the state, not a list of one.
    for hid in &lists {
        let state = env.query(&format!("SELECT state FROM entries WHERE hid = '{hid}'"));
        assert_eq!(state, ["\"b\""], "{hid}");
    }
    let rows = env.rows();
    let entries = rows.iter().filter(|r| !r.contains(" spec:mood "));
    assert!(
        !entries.into_iter().any(|r| r.contains("[\"b\"")),
        "{rows:#?}"
    );
}

#[test]
fn on_and_off_are_no_names() {
    let env = Env::new();
    for args in [["a", "true"], ["false", "c"], ["a", "FALSE"]] {
        let err = env.refused(&["rename", "mood", "--state", args[0], args[1]]);
        assert!(err.contains("is on or off, not a name"), "{err}");
    }
}

#[test]
fn an_unknown_state_is_refused() {
    let env = Env::new();
    let err = env.refused(&["rename", "mood", "--state", "zzz", "c"]);
    assert!(err.contains("`mood` has no state `zzz`"), "{err}");
    let err = env.refused(&["rename", "nofield", "--state", "a", "c"]);
    assert!(err.contains("no field `nofield`"), "{err}");
}

#[test]
fn a_new_name_a_log_would_not_read_back_is_refused() {
    let env = Env::new();
    let before = env.rows();
    for (new, says) in [
        ("5", "a number"),
        ("x,y", "a list of states"),
        ("end", "the command `end`"),
        ("k=v", "a key and its value"),
    ] {
        let err = env.refused(&["rename", "mood", "--state", "a", new]);
        assert!(
            err.contains("would not read back as a state"),
            "{new}: {err}"
        );
        assert!(err.contains(says), "{new}: {err}");
    }
    assert_eq!(env.rows(), before);
}

#[test]
fn a_state_naming_a_document_says_the_document_keeps_its_name() {
    let env = Env::new();
    env.ok(&["setup", "mood", "--state-lookup-field", "mood_kind"]);
    let out = env.ok(&["rename", "mood", "--state", "a", "c"]);
    assert!(out.contains("still refers to mood_kind:a"), "{out}");
}
