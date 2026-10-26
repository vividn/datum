//! A kit reinstall says where the database's field differs from the kit's,
//! and that it kept yours (da-twvn0).
//!
//! The real binary, because what is asserted is what install PRINTS.

use std::process::Command;

use tempfile::TempDir;

const KIT: &str = "[kit]\nname = \"habits\"\n\n[[field]]\nname = \"did\"\nverb = \"occur\"\nid_template = \"%occur_time%_%habit%\"\n";

fn ok(dir: &TempDir, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("k.db"))
        .env("DATUM_HOME", dir.path())
        .env("DATUM_CONFIG_HOME", dir.path().join("config"))
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

fn install(dir: &TempDir, kit: &str) -> String {
    let path = dir.path().join("kit.toml");
    std::fs::write(&path, kit).unwrap();
    ok(dir, &["kit", "install", &path.to_string_lossy()])
}

#[test]
fn a_reinstall_names_what_setup_changed_and_nothing_else() {
    let dir = TempDir::new().unwrap();
    install(&dir, KIT);
    let same = install(&dir, KIT);
    assert!(same.contains("  - did\n"), "{same}");
    assert!(!same.contains("differs"), "nothing edited: {same}");

    ok(
        &dir,
        &[
            "setup",
            "did",
            "--id-template",
            "%occur_time%_%habit%_edited",
        ],
    );
    let edited = install(&dir, KIT);
    assert!(
        edited.contains(
            "  - did: your id_template differs from the kit's \
             (yours: \"%occur_time%_%habit%_edited\", kit's: \"%occur_time%_%habit%\"); \
             kept yours, --force takes the kit's\n"
        ),
        "{edited}"
    );
    assert_eq!(edited.matches("differs").count(), 1, "{edited}");
}

/// A changed state list is one difference: the aliases it derives follow it
/// and are no second line.
#[test]
fn derived_aliases_follow_the_states_and_are_no_difference_of_their_own() {
    let kit = "[kit]\nname = \"k\"\n\n[[field]]\nname = \"mood\"\nverb = \"switch\"\nstates.a = {}\nstates.b = {}\n";
    let dir = TempDir::new().unwrap();
    install(&dir, kit);
    ok(&dir, &["setup", "mood", "--states", "a,b,c"]);
    let edited = install(&dir, kit);
    assert!(
        edited.contains("  - mood: your allowed_states differs from the kit's (yours: [\"a\",\"b\",\"c\"], kit's: [\"a\",\"b\"])"),
        "{edited}"
    );
    assert_eq!(edited.matches("differs").count(), 1, "{edited}");
}
