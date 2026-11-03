//! An open lays the every-open DDL only where `_meta` `laid` does not say
//! this build laid it, on this schema, with as many schema objects as there
//! are now (da-4qq8v): so a dropped trigger is laid again by the next open.

use datum_core::db::Db;

const TRIGGER: &str = "sync_uncovered_insert";

fn open(path: &std::path::Path) -> Db {
    Db::open(path.to_str().unwrap()).unwrap()
}

fn has_trigger(db: &Db) -> bool {
    db.conn()
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'trigger' AND name = ?1")
        .unwrap()
        .exists([TRIGGER])
        .unwrap()
}

fn laid(db: &Db) -> String {
    db.conn()
        .query_row("SELECT value FROM _meta WHERE key = 'laid'", [], |r| {
            r.get(0)
        })
        .unwrap()
}

/// Drop the trigger and write the stamp the count now says, as if laid so.
fn drop_trigger_keeping_the_stamp(db: &Db) {
    let fingerprint = laid(db).split_once(':').unwrap().0.to_string();
    db.conn()
        .execute_batch(&format!("DROP TRIGGER {TRIGGER}"))
        .unwrap();
    db.conn()
        .execute(
            "UPDATE _meta SET value = ?1 || ':' ||
               (SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'kit!_%' ESCAPE '!')
             WHERE key = 'laid'",
            [fingerprint],
        )
        .unwrap();
}

#[test]
fn a_dropped_trigger_is_laid_by_the_next_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let db = open(&path);
    assert!(has_trigger(&db));
    let stamp = laid(&db);
    db.conn()
        .execute_batch(&format!("DROP TRIGGER {TRIGGER}"))
        .unwrap();
    drop(db);

    let db = open(&path);
    assert!(
        has_trigger(&db),
        "the count read one object fewer: laid again"
    );
    assert_eq!(laid(&db), stamp);
}

#[test]
fn a_stamp_that_holds_skips_the_ddl_and_another_builds_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let db = open(&path);
    drop_trigger_keeping_the_stamp(&db);
    drop(db);

    // The stamp holds, so this open lays nothing: the trigger stays dropped,
    // which only the skip explains.
    let db = open(&path);
    assert!(!has_trigger(&db));
    let count = laid(&db).split_once(':').unwrap().1.to_string();
    db.conn()
        .execute(
            "UPDATE _meta SET value = ?1 WHERE key = 'laid'",
            [format!("0000000000000000:{count}")],
        )
        .unwrap();
    drop(db);

    let db = open(&path);
    assert!(has_trigger(&db), "another build's stamp: laid again");
}

/// A file migrated down keeps the stamp it was laid with, and the next open
/// migrates it up again: the stamp holds only at the version it was laid on.
#[test]
fn a_file_migrated_down_is_migrated_up_by_the_next_open() {
    use datum_core::migrations::{migrate_down_to, CURRENT_SCHEMA_VERSION};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let db = open(&path);
    migrate_down_to(db.conn(), CURRENT_SCHEMA_VERSION - 1).unwrap();
    drop(db);

    let db = open(&path);
    assert_eq!(db.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    assert!(db.last_migration().is_some());
}
