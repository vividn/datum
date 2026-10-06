//! A keep-both pair shares one slug. `check --fix` repairs the drifted row
//! of the pair and leaves the other alone: its verb and content-hash
//! rewrites address the row by hid, never by the slug (da-3qvnq).

use std::path::Path;
use std::process::Command;

/// Runs `datum`; `check --fix` exits 1 when it found drift, even drift it
/// repaired, so only `ok` callers assert success.
fn datum(db: &Path, args: &[&str], ok: bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.with_extension("home"))
        .env("DATUM_CONFIG_HOME", db.with_extension("config"))
        .args(args)
        .output()
        .expect("spawn datum");
    assert!(
        !ok || out.status.success(),
        "`datum {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// `(hid, verb, content_hash)` of every habit row, oldest first.
fn habits(db: &Path) -> Vec<(String, String, String)> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare("SELECT hid, verb, content_hash FROM entries WHERE field = 'habit' ORDER BY rowid")
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    rows
}

const AT: &str = "2026-10-04 10:00";

#[test]
fn fix_of_one_row_of_a_keep_both_pair_leaves_the_other_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("data.db");
    // x and y are two creates of one slug; only x has a duration, so their
    // verbs differ and a write of y's onto x shows. x steps off the slug
    // while y is logged, the way two copies each log one before a sync.
    datum(
        &db,
        &["occur", "habit", "-t", AT, "dur=30m", "note=x"],
        true,
    );
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE entries SET slug = 'habit:aside' WHERE field = 'habit'",
        [],
    )
    .unwrap();
    datum(&db, &["occur", "habit", "-t", AT, "note=y"], true);
    let [x, y] = <[_; 2]>::try_from(habits(&db)).unwrap();
    assert_ne!(x.1, y.1, "the pair needs two verbs to tell the rows apart");
    // The keep-both shape: y wins, x is set aside under the same slug.
    conn.execute(
        "UPDATE entries SET slug = (SELECT slug FROM entries WHERE hid = ?2), superseded_by = ?2
         WHERE hid = ?1",
        rusqlite::params![x.0, y.0],
    )
    .unwrap();
    // Then y drifts, in both derived columns.
    conn.execute(
        "UPDATE entries SET verb = ?2, content_hash = 'stale' WHERE hid = ?1",
        rusqlite::params![y.0, x.1],
    )
    .unwrap();

    datum(&db, &["check", "--fix"], false);
    assert_eq!(habits(&db), vec![x, y], "check --fix of y touched x");
}
