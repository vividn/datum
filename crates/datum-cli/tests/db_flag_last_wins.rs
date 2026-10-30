//! `--db` may be given any number of times, before and after the command,
//! and the last on the line wins: the entry lands there, and no database is
//! opened or created for one that lost (da-qicfx).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

struct Env(TempDir);

impl Env {
    fn new() -> Self {
        Env(TempDir::new().unwrap())
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env_remove("DATUM_DB")
            .env("HOME", self.0.path())
            .env("DATUM_HOME", self.home())
            .env("DATUM_CONFIG_HOME", self.0.path().join("config"))
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{args:?}: {stderr}");
        String::from_utf8(out.stdout).unwrap()
    }

    fn home(&self) -> PathBuf {
        self.0.path().join("home")
    }

    fn has_db(&self, name: &str) -> bool {
        Path::new(&self.home()).join(format!("{name}.db")).exists()
    }
}

#[test]
fn the_entry_lands_in_the_last_and_the_first_is_not_created() {
    for line in [
        &["--db", "first", "occur", "f", "--db", "last"][..],
        &["occur", "f", "--db", "first", "--db", "last"],
        &["--db=first", "occur", "f", "--db=last"],
    ] {
        let env = Env::new();
        env.ok(line);
        assert!(!env.has_db("first"), "{line:?}");
        let tail = env.ok(&["--db", "last", "tail", "--json"]);
        assert!(tail.contains("\"f\""), "{line:?}: {tail}");
        assert!(!env.has_db("first"), "{line:?}");
    }
}

/// The alias pass opens the database that wins, and no other: `al` exists
/// only in the last.
#[test]
fn an_alias_is_read_from_the_last() {
    let env = Env::new();
    env.ok(&["--db", "last", "alias", "set", "al", "occur f"]);
    let out = env.ok(&["--db", "first", "--db", "last", "al"]);
    assert!(out.contains("CREATE"), "{out}");
    assert!(!env.has_db("first"));
}

/// No database comes of a line the parser refuses, its `--db` with no value.
#[test]
fn a_db_with_no_value_refuses_and_writes_nothing() {
    let env = Env::new();
    assert!(!env
        .run(&["--db", "first", "occur", "f", "--db"])
        .status
        .success());
    assert!(!env.has_db("first"));
}

#[test]
fn a_line_of_commands_all_lands_in_the_last() {
    let env = Env::new();
    env.ok(&[
        "occur", "a", "--db", "first", ",", "occur", "b", "--db", "last",
    ]);
    assert!(!env.has_db("first"));
    let out = env.ok(&["--db", "last", "tail", "--json"]);
    assert!(out.contains("\"a\"") && out.contains("\"b\""), "{out}");
}

/// A kit's command, read after the parse fails, goes by the same flags.
#[test]
fn a_kit_command_runs_against_the_last() {
    let env = Env::new();
    let kit = env.0.path().join("k.toml");
    let body = "[kit]\nname = \"k\"\n\n[[view]]\nname = \"v\"\nsql = \"SELECT 'seen' AS word\"\n\n[[command]]\nname = \"kc\"\nview = \"v\"\n";
    std::fs::write(&kit, body).unwrap();
    env.ok(&[
        "--db",
        "last",
        "kit",
        "install",
        "--yes",
        kit.to_str().unwrap(),
    ]);
    let out = env.ok(&["--db", "first", "kc", "--db", "last"]);
    assert!(out.contains("seen"), "{out}");
    assert!(!env.has_db("first"));
}
