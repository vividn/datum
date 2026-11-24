//! da-6dv66: `datum check` asks SQLite of the file too. A page no checker's
//! table holds, spoiled, is found by `PRAGMA quick_check` and is an error
//! naming it; the same file before the damage is clean.

use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Command, Output};

fn datum(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn datum")
}

#[test]
fn check_names_a_malformed_page_outside_datums_tables() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("malformed.db");
    assert!(datum(&db, &["occur", "coffee", "-y"]).status.success());

    // A table of no datum's, its root page found and checkpointed into the file.
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE filler (x); WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 \
         FROM n WHERE i < 40) INSERT INTO filler SELECT i FROM n; \
         PRAGMA wal_checkpoint(TRUNCATE);",
    )
    .unwrap();
    let sql = "SELECT rootpage FROM sqlite_schema WHERE name = 'filler'";
    let root: u64 = conn.query_row(sql, [], |r| r.get(0)).unwrap();
    let size: u64 = conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap();
    drop(conn);

    let clean = datum(&db, &["check"]);
    let said = String::from_utf8_lossy(&clean.stdout);
    assert!(clean.status.success(), "clean file: {said}");
    assert!(!said.contains("quick_check"), "{said}");

    let mut file = std::fs::OpenOptions::new().write(true).open(&db).unwrap();
    file.seek(SeekFrom::Start((root - 1) * size)).unwrap();
    file.write_all(&vec![0xff; size as usize]).unwrap();
    drop(file);

    let spoiled = datum(&db, &["check", "integrity"]);
    let said = String::from_utf8_lossy(&spoiled.stdout);
    assert_eq!(spoiled.status.code(), Some(1), "{said}");
    assert!(said.contains("[ERROR] quick_check"), "{said}");
    assert!(said.contains("malformed"), "{said}");
    assert!(said.contains(&format!("page {root}")), "{said}");

    // The default battery says it too.
    let all = datum(&db, &["check"]);
    let said = String::from_utf8_lossy(&all.stdout);
    assert_eq!(all.status.code(), Some(1), "{said}");
    assert!(said.contains("quick_check"), "{said}");
}
