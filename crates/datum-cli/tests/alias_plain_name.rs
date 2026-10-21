//! An alias name is a plain word (da-uqv4r): `alias set` and `kit install`
//! refuse any other, saying which characters are allowed, and store nothing.

use std::process::{Command, Output};

use tempfile::TempDir;

fn datum(dir: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("a.db"))
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .unwrap()
}

fn refused(out: &Output) -> String {
    assert!(!out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn no_aliases(dir: &TempDir) {
    let out = datum(dir, &["alias", "list"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "No aliases defined.\n"
    );
}

#[test]
fn alias_set_refuses_a_name_that_is_not_a_plain_word() {
    let dir = TempDir::new().unwrap();
    for name in ["x;rm", "x;touch pwned", "two words", "é/"] {
        let stderr = refused(&datum(&dir, &["alias", "set", name, "occur coffee"]));
        assert!(
            stderr.contains(&format!("alias name '{name}' is not a plain word"))
                && stderr.contains("letters, digits and _ - . + @ :"),
            "{name}: {stderr}"
        );
    }
    no_aliases(&dir);
    let out = datum(&dir, &["alias", "set", "x.rm+1@a:b_c-d", "occur coffee"]);
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn kit_install_refuses_an_alias_that_is_not_a_plain_word_naming_kit_and_alias() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("kit.toml");
    std::fs::write(
        &path,
        "[kit]\nname = \"bad\"\n\n[[field]]\nname = \"coffee\"\nverb = \"occur\"\n\n\
         [[alias]]\nname = \"x;rm\"\nexpands_to = \"occur coffee\"\n",
    )
    .unwrap();
    let stderr = refused(&datum(&dir, &["kit", "install", &path.to_string_lossy()]));
    assert!(
        stderr.contains("kit 'bad'") && stderr.contains("alias name 'x;rm' is not a plain word"),
        "{stderr}"
    );
    no_aliases(&dir);
    let fields = datum(&dir, &["fields", "--json"]);
    assert!(
        !String::from_utf8_lossy(&fields.stdout).contains("coffee"),
        "nothing of the kit is installed: {fields:?}"
    );
}
