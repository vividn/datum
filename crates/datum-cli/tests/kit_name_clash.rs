//! A kit's alias and another kit's command of one name (da-jp6ji): the alias
//! takes the word whichever came first, so the second is skipped and listed,
//! and `--force` installs it and says what it hides.
//!
//! The real binary, because what is asserted is what install PRINTS.

use std::process::{Command, Output};

use tempfile::TempDir;

const ALIAS_KIT: &str =
    "[kit]\nname = \"xa\"\n\n[[alias]]\nname = \"spent\"\nexpands_to = \"occur money\"\n";
const COMMAND_KIT: &str = "[kit]\nname = \"xc\"\n\n[[view]]\nname = \"spent_v\"\nsql = \"SELECT field FROM entries\"\n\n[[command]]\nname = \"spent\"\nview = \"spent_v\"\n";

fn datum(dir: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("k.db"))
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .unwrap()
}

fn ok(dir: &TempDir, args: &[&str]) -> String {
    let out = datum(dir, args);
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

fn install(dir: &TempDir, toml: &str, extra: &[&str]) -> String {
    let path = dir.path().join("kit.toml");
    std::fs::write(&path, toml).unwrap();
    ok(
        dir,
        &[&["kit", "install", &path.to_string_lossy()][..], extra].concat(),
    )
}

/// Whether typing `spent` logged `money`: the alias ran, not the command.
fn spent_logs_money(dir: &TempDir) -> bool {
    ok(dir, &["spent"]);
    let tail: serde_json::Value =
        serde_json::from_str(&ok(dir, &["tail", "money", "--json"])).unwrap();
    !tail["rows"].as_array().unwrap().is_empty()
}

#[test]
fn an_alias_and_a_command_of_one_name_skip_the_second_in_either_order() {
    let cases = [
        (
            ALIAS_KIT,
            COMMAND_KIT,
            "Skipped 1 command(s) (existing rows kept; use --force to overwrite):\n  - spent: the alias of kit xa\n",
            "Installed 1 command(s):\n  + spent: hidden by the alias of kit xa\n",
        ),
        (
            COMMAND_KIT,
            ALIAS_KIT,
            "Skipped 1 alias(es) (existing rows kept; use --force to overwrite):\n  - spent: the command of kit xc\n",
            "Installed 1 alias(es):\n  + spent: hides the command of kit xc\n",
        ),
    ];
    for (first, second, skipped, forced) in cases {
        let dir = tempfile::tempdir().unwrap();
        install(&dir, first, &[]);
        let alias_first = first == ALIAS_KIT;
        let printed = install(&dir, second, &[]);
        assert!(printed.contains(skipped), "{printed}");
        assert_eq!(spent_logs_money(&dir), alias_first, "the first one kept");

        let printed = install(&dir, second, &["--force"]);
        assert!(printed.contains(forced), "{printed}");
        assert!(spent_logs_money(&dir), "the alias wins, as it says");
    }
}
