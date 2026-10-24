//! A field name names the one field it starts (da-tldkb): a read always, a
//! write only where it could not create the name anyway, and in an entry's
//! place a field wins over a hid.

use std::path::Path;
use std::process::Command;

use crate::common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::fresh_db;

/// The binary's stdout and stderr, and whether it succeeded.
fn datum(db: &Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn datum");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn seeded(dir: &Path) -> std::path::PathBuf {
    let db = dir.join("t.db");
    for line in [
        &["switch", "environment", "home"][..],
        &["switch", "envoy", "sent"],
        &["occur", "coffee"],
    ] {
        assert!(datum(&db, line).0, "{line:?}");
    }
    db
}

#[test]
fn a_read_takes_the_one_field_a_prefix_starts() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    let (ok, out, err) = datum(&db, &["tail", "envi"]);
    assert!(ok, "{err}");
    assert!(
        out.contains("environment") && !out.contains("envoy"),
        "{out}"
    );
    let (ok, out, err) = datum(&db, &["states", "envi"]);
    assert!(ok && out.contains("home"), "{out}{err}");
    let (ok, out, err) = datum(&db, &["get", "envi", "state=home"]);
    assert!(ok && out.contains("environment"), "{out}{err}");
}

#[test]
fn a_prefix_starting_several_fields_is_refused_naming_them() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    let (ok, out, err) = datum(&db, &["tail", "env"]);
    assert!(!ok, "{out}");
    assert!(
        err.contains("environment, envoy"),
        "names the candidates: {err}"
    );
}

#[test]
fn an_open_db_write_to_a_new_name_creates_it() {
    let db = fresh_db();
    run_cli(&["switch", "environment", "home"], &db).unwrap();
    run_cli(&["occur", "envi"], &db).unwrap();
    assert!(db.field_has_entries("envi").unwrap());
    assert_eq!(db.entries_for_field("environment", None).unwrap().len(), 1);
}

/// The tracker's case: `issue__%hid:8%` creates nothing unless asked, and
/// its kit's alias builds the name with `-F`.
fn strict_board(ids: &[&str]) -> Db {
    let db = fresh_db();
    run_cli(
        &[
            "setup",
            "issue__%hid:8%",
            "--verb",
            "switch",
            "--auto-setup",
            "false",
        ],
        &db,
    )
    .unwrap();
    for id in ids {
        run_cli(&["switch", &format!("issue__{id}"), "open", "--setup"], &db).unwrap();
    }
    run_cli(
        &["alias", "set", "idone", "switch issue -F issue_id done"],
        &db,
    )
    .unwrap();
    db
}

#[test]
fn a_write_where_creation_is_off_takes_the_one_field_it_starts() {
    let db = strict_board(&["k7f2a9b1", "a1b2c3d4", "a1b9c3d4"]);
    let state = |db: &Db| db.state_at("issue__k7f2a9b1", chrono::Utc::now()).unwrap();
    run_cli(&["switch", "issue__k7f", "done"], &db).unwrap();
    assert_eq!(state(&db), Some(serde_json::json!("done")));
    assert!(!db.field_exists("issue__k7f").unwrap());

    // The kit's alias: the name `-F` builds is the one read.
    run_cli(&["switch", "issue__k7f2a9b1", "open"], &db).unwrap();
    run_cli(&["idone", "k7f"], &db).unwrap();
    assert_eq!(state(&db), Some(serde_json::json!("done")));
    run_cli(&["idone", "a1b2c3d4"], &db).unwrap();
    assert!(!db.field_exists("issue__k7f2a9b1__k7f2a9b1").unwrap());

    // Under the closed head, a name starting no field is created by nothing.
    let err = run_cli(&["idone", "zz"], &db).unwrap_err();
    assert!(err.to_string().contains("--setup"), "{err}");
    assert!(!db.field_exists("issue__zz").unwrap());

    let err = run_cli(&["switch", "issue__a1b", "done"], &db).unwrap_err();
    assert!(
        err.to_string().contains("issue__a1b2c3d4, issue__a1b9c3d4"),
        "{err}"
    );
    assert!(!db.field_exists("issue__a1b").unwrap());

    // `--setup` asks for the name itself.
    run_cli(&["switch", "issue__k7f", "done", "--setup"], &db).unwrap();
    assert!(db.field_exists("issue__k7f").unwrap());
}

#[test]
fn a_name_outside_a_closed_head_is_created() {
    let db = strict_board(&["k7f2a9b1"]);
    run_cli(&["occur", "iss"], &db).unwrap();
    assert!(db.field_has_entries("iss").unwrap());
    assert_eq!(
        db.entries_for_field("issue__k7f2a9b1", None).unwrap().len(),
        1
    );
    run_cli(&["switch", "issue__a1b2c3d4", "open", "--setup"], &db).unwrap();
    run_cli(&["occur", "issue"], &db).unwrap();
    assert!(db.field_has_entries("issue").unwrap());
}

#[test]
fn in_an_entrys_place_a_field_wins_over_a_hid() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    let (_, hid, _) = datum(&db, &["get", "coffee::hid"]);
    let hid = hid.trim();
    let field = format!("{}x", &hid[..3]);
    assert!(datum(&db, &["occur", &field]).0);

    // The hid's first three characters start the field and the hid.
    let (ok, out, err) = datum(&db, &["get", &hid[..3]]);
    assert!(ok, "{err}");
    assert!(out.contains(&field) && !out.contains("coffee"), "{out}");
    let (ok, out, err) = datum(&db, &["get", &format!("{}::field", &hid[..3])]);
    assert!(ok, "{err}");
    assert_eq!(out.trim(), field);

    // A field with no entry to show, a spec alone, leaves the name to the
    // hid: here the only field it starts.
    let (_, other, _) = datum(&db, &["get", &format!("{field}::hid")]);
    assert!(datum(&db, &["delete", other.trim()]).0);
    assert!(datum(&db, &["setup", &field, "--verb", "occur"]).0);
    let (ok, out, err) = datum(&db, &["get", &hid[..3]]);
    assert!(ok, "{err}");
    assert!(out.contains("coffee"), "{out}");
    let (ok, out, err) = datum(&db, &["get", &format!("{}::field", &hid[..3])]);
    assert!(ok, "{err}");
    assert_eq!(out.trim(), "coffee");

    // No field may be named as a hid.
    for line in [
        &["occur", hid][..],
        &["setup", hid, "--verb", "occur"],
        &["rename", "coffee", hid],
    ] {
        let (ok, _, err) = datum(&db, line);
        assert!(!ok, "{line:?}");
        assert!(err.contains("hid"), "{line:?}: {err}");
    }
}
