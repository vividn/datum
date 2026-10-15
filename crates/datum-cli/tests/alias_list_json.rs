//! `datum alias list --json` (da-x6yj): every alias in the namespace as
//! `{ name, expansion, source }`, a derived one with the `field` and
//! `kind` it came from. The datum.day aliases widget reads it.
//!
//! Through the real binary, because what is asserted is stdout.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};

fn datum(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn datum")
}

fn ok(db: &Path, args: &[&str]) -> String {
    let out = datum(db, args);
    assert!(
        out.status.success(),
        "datum {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn alias_list_json_has_user_and_derived() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    ok(&db, &["spec", "coffee", "--verb", "occur"]);
    ok(&db, &["alias", "set", "cof", "occur coffee cups=1"]);

    let listed: Value = serde_json::from_str(&ok(&db, &["alias", "list", "--json"])).unwrap();
    assert_eq!(
        listed,
        json!([
            { "name": "cof", "expansion": "occur coffee cups=1", "source": "user" },
            { "name": "coffee", "expansion": "occur coffee", "source": "derived",
              "field": "coffee", "kind": "bare_field" },
        ])
    );

    // The text form is unchanged.
    assert_eq!(
        ok(&db, &["alias", "list"]),
        "  cof → datum occur coffee cups=1\n  coffee → datum occur coffee\n"
    );
}

#[test]
fn alias_list_json_on_an_empty_db_is_an_empty_array() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    assert_eq!(ok(&db, &["alias", "list", "--json"]).trim(), "[]");
}

#[test]
fn alias_list_refuses_what_it_does_not_take() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("t.db");
    for extra in ["--bogus", "coffee"] {
        let out = datum(&db, &["alias", "list", extra]);
        assert!(!out.status.success(), "`alias list {extra}` must fail");
    }
}
