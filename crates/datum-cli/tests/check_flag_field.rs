//! `datum check` does not report a field `datum flag` made as unregistered
//! (da-9p08z). `flag <hid>` takes the hid as its field, and `unknown_field`
//! named it beside the `flagged_entry` warning that is the real finding.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn datum(db: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn datum");
    assert!(
        out.status.success(),
        "datum {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn a_flag_is_reported_as_flagged_and_not_as_an_unknown_field() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    let created = datum(&db, &["occur", "coffee"]);
    let hid = created.split_whitespace().nth(1).unwrap();
    datum(&db, &["flag", hid]);

    let check = datum(&db, &["check"]);
    assert!(check.contains("flagged_entry (x1)"), "{check}");
    assert!(!check.contains("unknown_field"), "{check}");
}

/// The control: a field logged with no spec is still reported.
#[test]
fn a_logged_field_with_no_spec_is_still_an_unknown_field() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("t.db");
    datum(&db, &["occur", "tea"]);
    datum(&db, &["spec", "tea", "--delete"]);

    let check = datum(&db, &["check"]);
    assert!(check.contains("unknown_field (x1)"), "{check}");
    assert!(check.contains("example field: tea"), "{check}");
}
