//! Help at the width it is shown in (da-nt53d): `COLUMNS=40 datum help
//! occur` has no line over 40, and piped with no width given a page is
//! printed as it was written.
//!
//! Through the real binary, because what is asserted is stdout.

use std::process::Command;

fn help(columns: Option<&str>, args: &[&str]) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
    cmd.env("DATUM_HOME", tmp.path())
        .env("DATUM_CONFIG_HOME", tmp.path())
        .env("NO_COLOR", "1")
        .env_remove("COLUMNS")
        .args(args);
    if let Some(c) = columns {
        cmd.env("COLUMNS", c);
    }
    let out = cmd.output().expect("spawn datum");
    assert!(out.status.success(), "datum {args:?}");
    String::from_utf8(out.stdout).unwrap()
}

fn widest(text: &str) -> usize {
    text.lines().map(|l| l.chars().count()).max().unwrap()
}

#[test]
fn help_fits_columns() {
    for args in [&["help", "occur"][..], &["help"], &["occur", "--help"]] {
        let written = help(None, args);
        assert!(widest(&written) > 40, "{args:?} as written");
        assert!(widest(&help(Some("40"), args)) <= 40, "{args:?} at 40");
        assert_eq!(help(Some("200"), args), written, "{args:?} at 200");
    }
}
