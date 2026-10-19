//! A kit command takes arguments in #263's key kinds, bound into its view as
//! SQLite named parameters; `--sort` sorts by a shown column; `--show json`
//! prints the shown rows (da-7ramf). The real binary, so the words reach
//! the command the way a person types them.

use std::process::{Command, Output};

use tempfile::TempDir;

const KIT: &str = r#"
[kit]
name = "argkit"

[[view]]
name = "echo"
sql = "SELECT :a AS a, :b AS b, :c AS c, (SELECT COUNT(*) FROM sqlite_master WHERE name = 'entries') AS entries"

[[command]]
name = "echo"
view = "echo"
args = ["a", "b=", "c=3"]

[[view]]
name = "nums"
sql = "SELECT column1 AS n, column2 AS s FROM (VALUES (2, 'b'), (1, 'a'), (3, 'c')) WHERE :none = 0"

[[command]]
name = "nums"
view = "nums"
switches = ["none"]
columns = ["n"]
empty = "nothing"

[[command.color_rules]]
when = "n = 2"
color = "yellow"
"#;

struct Env(TempDir);

impl Env {
    fn new() -> Self {
        let env = Self(tempfile::tempdir().unwrap());
        let kit = env.0.path().join("argkit.toml");
        std::fs::write(&kit, KIT).unwrap();
        let out = env.run(&["kit", "install", "--yes", kit.to_str().unwrap()]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        env
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with(args, &[("NO_COLOR", "1")])
    }

    fn run_with(&self, args: &[&str], vars: &[(&str, &str)]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.0.path().join("data.db"))
            .env("DATUM_HOME", self.0.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.0.path().join("config"))
            .env_remove("NO_COLOR")
            .env_remove("FORCE_COLOR")
            .env_remove("CLICOLOR_FORCE")
            .envs(vars.iter().copied())
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let mut args = args.to_vec();
        args.extend(["--show", "json"]);
        serde_json::from_str(&self.ok(&args)).unwrap()
    }

    fn err(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            !out.status.success(),
            "`datum {}` succeeded",
            args.join(" ")
        );
        String::from_utf8(out.stderr).unwrap()
    }
}

/// `echo`'s one row as `[a, b, c]`.
fn abc(env: &Env, args: &[&str]) -> serde_json::Value {
    let mut argv = vec!["echo"];
    argv.extend(args);
    let row = &env.json(&argv)[0];
    serde_json::json!([row["a"], row["b"], row["c"]])
}

#[test]
fn arguments_fill_like_an_alias_s_slots() {
    use serde_json::json;
    let env = Env::new();
    assert_eq!(abc(&env, &["x", "y", "z"]), json!(["x", "y", "z"]));
    // An optional left empty is NULL; a default fills in.
    assert_eq!(abc(&env, &["x"]), json!(["x", null, 3]));
    // `.` skips a slot, to its default.
    assert_eq!(abc(&env, &["x", ".", "5"]), json!(["x", null, 5]));
    assert_eq!(abc(&env, &["x", "y", "."]), json!(["x", "y", 3]));
    // `key=value` fills by name, in any order.
    assert_eq!(abc(&env, &["c=9", "a=x"]), json!(["x", null, 9]));

    assert!(env.err(&["echo"]).contains("Missing required key: a"));
    assert!(env
        .err(&["echo", "x", "d=1"])
        .contains("takes no argument 'd'"));
    assert!(env
        .err(&["echo", "x", "y", "z", "w"])
        .contains("Extra data"));
    assert!(env.err(&["echo", "x", "-A"]).contains("unknown flag -A"));
}

#[test]
fn an_argument_is_bound_never_spliced() {
    let env = Env::new();
    let evil = "'); DROP TABLE entries; --";
    let row = &env.json(&["echo", evil])[0];
    assert_eq!(row["a"], evil, "the value arrives byte for byte");
    assert_eq!(row["entries"], 1, "entries still stands");
    // Control: the same row, read again, still sees the table.
    assert_eq!(env.json(&["echo", "x"])[0]["entries"], 1);
}

#[test]
fn sort_takes_a_shown_column_and_a_minus_reverses_it() {
    let env = Env::new();
    let ns = |args: &[&str]| -> Vec<i64> {
        let mut argv = vec!["nums"];
        argv.extend(args);
        let rows = env.json(&argv);
        rows.as_array()
            .unwrap()
            .iter()
            .map(|r| r["n"].as_i64().unwrap())
            .collect()
    };
    assert_eq!(ns(&[]), [2, 1, 3], "control: the view's own order");
    assert_eq!(ns(&["--sort", "n"]), [1, 2, 3]);
    assert_eq!(ns(&["--sort", "-n"]), [3, 2, 1]);
    assert_eq!(ns(&["--sort=-n"]), [3, 2, 1]);

    let err = env.err(&["nums", "--sort", "s"]);
    assert!(
        err.contains("--sort takes a shown column (n), not 's'"),
        "{err}"
    );
}

/// Each row is the shown columns, its cells as the table prints them, and
/// the colour its rule paints, as hex (da-gh37k); `--json` is `--show json`.
#[test]
fn show_json_is_the_shown_rows_and_no_rows_is_an_empty_array() {
    let env = Env::new();
    let rows = env.json(&["nums"]);
    assert_eq!(
        rows,
        serde_json::json!([
            { "n": 2, "row_cells": ["2"], "row_color": "#e5a50a" },
            { "n": 1, "row_cells": ["1"] },
            { "n": 3, "row_cells": ["3"] },
        ]),
        "hidden `s` stays out"
    );
    assert_eq!(
        env.ok(&["nums", "--json"]),
        env.ok(&["nums", "--show", "json"])
    );
    let table: Vec<String> = env
        .ok(&["nums"])
        .lines()
        .skip(2)
        .take(3)
        .map(|l| l.trim_end().into())
        .collect();
    assert_eq!(table, ["2", "1", "3"], "control: the table's rows");
    assert_eq!(env.ok(&["nums", "--none", "--show", "json"]), "[]\n");
    // Control: a person gets the command's `empty` words.
    assert_eq!(env.ok(&["nums", "--none"]), "nothing\n");
}

/// A switch is a flag: `--none` binds 1, its absence 0 (never NULL), and it
/// takes no value — not `none=1`, not `--none=1`.
#[test]
fn a_switch_binds_one_when_given_and_zero_when_not() {
    let env = Env::new();
    assert_eq!(
        env.json(&["nums"]).as_array().unwrap().len(),
        3,
        "absent is 0"
    );
    assert_eq!(env.json(&["nums", "--none"]), serde_json::json!([]));
    assert!(env
        .err(&["nums", "none=1"])
        .contains("takes no argument 'none'"));
    assert!(env
        .err(&["nums", "--none=1"])
        .contains("unknown flag --none=1"));
}

/// A colour rule goes through the canonical gate (da-cjh89): piped with
/// `FORCE_COLOR=1` — how datum.day runs the CLI — the matching row is
/// painted, and `FORCE_COLOR=0` or `NO_COLOR` keeps it plain.
#[test]
fn a_color_rule_follows_the_color_gate() {
    let env = Env::new();
    let nums = |vars: &[(&str, &str)]| {
        let out = env.run_with(&["nums"], vars);
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    };
    let yellow = nums(&[("FORCE_COLOR", "1")]);
    let row = yellow.lines().find(|l| l.contains('2')).unwrap();
    assert!(row.starts_with("\x1b[33m"), "{yellow:?}");
    assert_eq!(yellow.matches("\x1b[33m").count(), 1, "{yellow:?}");
    for vars in [&[("FORCE_COLOR", "0")][..], &[("NO_COLOR", "1")], &[]] {
        let plain = nums(vars);
        assert!(!plain.contains('\x1b'), "{vars:?}: {plain:?}");
    }
    // `--color` in front of a kit command, which no parse knows (da-r15mq).
    let flagged = |args: &[&str], vars: &[(&str, &str)]| {
        let out = env.run_with(args, vars);
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(
        flagged(&["--color", "always", "nums"], &[("NO_COLOR", "1")]),
        yellow
    );
    let plain = flagged(&["--color=never", "nums"], &[("FORCE_COLOR", "1")]);
    assert!(!plain.contains('\x1b'), "{plain:?}");
}
