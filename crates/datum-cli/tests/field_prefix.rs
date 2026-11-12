//! A field name names the one field it starts (da-tldkb): a read always, a
//! write only where it could not create the name anyway, and in an entry's
//! place a field wins over a hid.

use std::path::Path;
use std::process::Command;

use crate::common::run_cli;
use datum_core::db::Db;
use datum_core::test_util::fresh_db;

/// The shortest start of `hid`, three characters or more, that a log keeps
/// as a field's name. A word read as a number or a literal is not kept, as
/// in v2: `occur 0e3` logs on `0.0`, `occur inf` on `Infinity` (da-34e01).
/// So a test logging on a field that only starts a hid names it from here.
pub(crate) fn a_field_starting(hid: &str) -> &str {
    (3..hid.len())
        .map(|n| &hid[..n])
        .find(|p| {
            let kept = datum_parse::input::infer_type(p).map(|v| match v {
                serde_json::Value::String(s) => s,
                v => v.to_string(),
            });
            kept.as_deref() == Some(p)
        })
        .unwrap_or_else(|| panic!("no start of {hid} is kept as a field's name"))
}

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

/// da-zai4h: a write names an entry as `get` does. `retime tea` read the
/// field `tea`'s one entry and a row whose hid starts `tea` as two matches,
/// a flake once in 36³ rows. da-sr76g: `retime` and `delete` take the
/// field's newest entry, as `edit` and `update` do.
#[test]
fn a_write_takes_the_field_over_a_hid_it_starts() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded(dir.path());
    let (_, hid, _) = datum(&db, &["get", "coffee::hid"]);
    let hid = hid.trim();
    let field = a_field_starting(hid);
    assert!(datum(&db, &["occur", field, "-y", "-t", "9:00"]).0);

    let (ok, out, err) = datum(&db, &["retime", field, "-y", "-t", "9:30"]);
    assert!(ok, "{err}");
    assert!(out.contains(field) && !out.contains("coffee"), "{out}");

    // Two entries of the field: the newest, and nothing else.
    assert!(datum(&db, &["occur", field, "-y", "-t", "11:00"]).0);
    // (hid, at, deleted) of the field's rows, newest first.
    let rows = || -> Vec<(String, Option<String>, bool)> {
        let conn = Db::open(db.to_str().unwrap()).unwrap();
        let sql = format!(
            "SELECT hid, CAST(at AS TEXT), deleted_at IS NOT NULL FROM entries \
             WHERE field = '{field}' ORDER BY at DESC"
        );
        let mut stmt = conn.conn().prepare(&sql).unwrap();
        let got = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        got
    };
    let before = rows();
    let (newest, older) = (&before[0], &before[1]);
    let (ok, out, err) = datum(&db, &["retime", field, "-y", "-t", "11:45"]);
    assert!(ok, "{err}");
    assert!(
        out.contains(&newest.0[..5]) && !out.contains("coffee"),
        "{out}"
    );
    let after = rows();
    assert_eq!(after[0].0, newest.0);
    assert_ne!(after[0].1, newest.1, "the newest moved");
    assert_eq!(&after[1], older, "the older did not");

    let (ok, out, err) = datum(&db, &["delete", field]);
    assert!(ok, "{err}");
    assert!(out.contains(&newest.0[..5]), "{out}");
    let after = rows();
    assert!(after.iter().any(|r| r.0 == newest.0 && r.2), "{after:?}");
    assert!(after.contains(older), "{after:?}");

    // A word that is no field, hid or slug is refused, and changes nothing.
    for cmd in ["retime", "delete"] {
        let (ok, _, err) = datum(&db, &[cmd, "zzzq"]);
        assert!(!ok && err.contains("not found"), "{cmd}: {err}");
    }
    assert_eq!(rows(), after);

    // A whole hid is its entry.
    let (ok, out, err) = datum(&db, &["retime", hid, "-y", "-t", "8:00"]);
    assert!(ok, "{err}");
    assert!(out.contains("coffee"), "{out}");
}

/// da-34e01: a hid whose first three characters a log reads as a number or
/// a literal (`0e3` logs on `0.0`) gives a test the first longer start a
/// log keeps, so a test naming a field by a hid's start does not flake.
#[test]
fn a_field_starting_a_hid_is_a_name_a_log_keeps() {
    let db = fresh_db();
    for (hid, field) in [
        ("0e3m68hotnz6", "0e3m"),
        ("1e23abcdefgh", "1e23a"),
        ("infq0aaaaaaa", "infq"),
        ("nanq0aaaaaaa", "nanq"),
        ("123abcdefghi", "123"),
        ("coffee000000", "cof"),
    ] {
        assert_eq!(a_field_starting(hid), field, "{hid}");
        run_cli(&["occur", field, "-y", "-t", "9:00"], &db).unwrap();
        assert!(db.field_has_entries(field).unwrap(), "{field}");
    }
}
