//! A flag before the name of `alias show/remove`, `kit show/uninstall` or
//! `spec remove` is refused as a flag, not taken as the name (da-dnvgk):
//! `alias remove --force tea` said «unexpected argument: 'tea'», blaming
//! the one word that was right.

use std::process::{Command, Output};

fn run(db: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "0")
        .args(args)
        .output()
        .expect("run datum")
}

#[test]
fn a_leading_flag_is_named_and_nothing_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    assert!(run(&db, &["alias", "set", "tea", "occur coffee"])
        .status
        .success());
    for args in [
        ["alias", "show", "--force", "tea"],
        ["alias", "remove", "--force", "tea"],
        ["kit", "show", "--force", "tea"],
        ["kit", "uninstall", "--force", "tea"],
        ["spec", "remove", "--force", "tea"],
    ] {
        let out = run(&db, &args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?} must fail");
        assert!(
            stderr.contains("unexpected flag: --force"),
            "{args:?} must name the flag: {stderr}"
        );
    }
    let shown =
        |db| String::from_utf8_lossy(&run(db, &["alias", "show", "tea"]).stdout).into_owned();
    assert!(shown(&db).contains("tea → "), "the alias must survive");
    assert!(run(&db, &["alias", "remove", "tea"]).status.success());
    assert!(!shown(&db).contains("tea → "), "a bare name still removes");
}

/// A name that starts with a dash — `alias set` makes one — is reached
/// after `--`, which ends the flags.
#[test]
fn a_dash_name_is_removed_after_double_dash() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    assert!(run(&db, &["alias", "set", "--force", "occur coffee"])
        .status
        .success());
    let shown = |db| {
        String::from_utf8_lossy(&run(db, &["alias", "show", "--", "--force"]).stdout).into_owned()
    };
    assert!(shown(&db).contains("--force → "), "{}", shown(&db));
    assert!(run(&db, &["alias", "remove", "--", "--force"])
        .status
        .success());
    assert!(!shown(&db).contains("--force → "), "{}", shown(&db));
}
