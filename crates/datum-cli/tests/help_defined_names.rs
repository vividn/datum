//! `datum help <NAME>` for a name the database defines (da-3odno). An alias
//! got the general page, byte for byte what a typo and a bare `datum help`
//! got, so datum.day's Help button explained no kit command. Now an alias
//! or a kit command answers with its own page, and a miss says it is one.
//! The real binary, as `help` is what a person's shell and the page run.

use std::process::Command;

use tempfile::TempDir;

const KIT: &str = r#"
[kit]
name = "helpkit"

[[alias]]
name = "ache"
expands_to = "note symptom"
description = "ache <name> [severity 0-10]"

[[view]]
name = "ones"
sql = "SELECT 1 AS n"

[[command]]
name = "aches"
view = "ones"
help = "every ache this week"
args = ["who", "since=", "n=3"]
switches = ["all"]
flags = ["watch"]

[[command]]
name = "ow"
expands_to = "occur ow"
help = "log an ow"
"#;

struct Env(TempDir);

impl Env {
    fn new() -> Self {
        let env = Self(tempfile::tempdir().unwrap());
        let kit = env.0.path().join("helpkit.toml");
        std::fs::write(&kit, KIT).unwrap();
        env.ok(&["kit", "install", "--yes", kit.to_str().unwrap()]);
        env.ok(&["alias", "set", "tea", "occur tea"]);
        env
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.0.path().join("data.db"))
            .env("DATUM_HOME", self.0.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.0.path().join("config"))
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "`datum {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

#[test]
fn an_alias_a_kit_command_and_a_miss_each_answer_for_themselves() {
    let env = Env::new();
    assert_eq!(
        env.ok(&["help", "ache"]),
        "datum ache — ache <name> [severity 0-10]\n\nexpands to: datum note symptom\n\
         options: datum help note\n\
         from the kit 'helpkit': datum kit show helpkit\n"
    );
    // `ALIAS --help` is that page too, not the page of what it expands to
    // (da-wxan9): `note`'s says nothing of the alias or its description.
    assert_eq!(env.ok(&["ache", "--help"]), env.ok(&["help", "ache"]));
    assert_eq!(env.ok(&["tea", "-h"]), env.ok(&["help", "tea"]));
    assert_eq!(
        env.ok(&["help", "tea"]),
        "datum tea — an alias\n\nexpands to: datum occur tea\noptions: datum help occur\n"
    );
    assert_eq!(
        env.ok(&["help", "aches"]),
        "datum aches — every ache this week\n\n\
         usage: datum aches <who> [since] [n=3] [--all] [--watch] [--sort [-]COLUMN]\n\
         shows the view 'ones'\n\
         from the kit 'helpkit': datum kit show helpkit\n"
    );
    assert_eq!(
        env.ok(&["help", "ow"]),
        "datum ow — log an ow\n\nexpands to: datum occur ow\n\
         from the kit 'helpkit': datum kit show helpkit\n"
    );
    // The bead's measurement: a defined name, a typo and no name at all
    // printed the same bytes. Each is its own now.
    let general = env.ok(&["help"]);
    let miss = env.ok(&["help", "zzznosuchcommand"]);
    assert_eq!(
        miss,
        format!("datum help: no command, topic or alias named 'zzznosuchcommand'\n\n{general}")
    );
    // A built-in command is still the table's page, the db not asked.
    assert!(env.ok(&["help", "occur"]).starts_with("datum occur — "));
}

#[test]
fn the_db_named_by_db_is_the_one_asked() {
    let env = Env::new();
    let db = env.0.path().join("data.db");
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", env.0.path().join("other.db"))
        .env("DATUM_HOME", env.0.path().join("home"))
        .env("DATUM_CONFIG_HOME", env.0.path().join("config"))
        .args(["--db", db.to_str().unwrap(), "help", "tea"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "datum tea — an alias\n\nexpands to: datum occur tea\noptions: datum help occur\n"
    );
}

/// The commonest alias is a field's own word, and `FIELD --help` is asked
/// for what may follow it: the page names the page that says (da-wxan9).
#[test]
fn a_fields_own_word_names_the_page_of_its_verb() {
    let env = Env::new();
    env.ok(&["spec", "coffee", "--verb", "occur"]);
    assert_eq!(
        env.ok(&["coffee", "--help"]),
        "datum coffee — coffee's own word\n\n\
         expands to: datum occur coffee\n\
         options: datum help occur\n"
    );
}
