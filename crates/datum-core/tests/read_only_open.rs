//! `Db::open_read_only` beside a writer (da-uxfj3): a reader reads through
//! the `-wal` and `-shm` of a writer that has the file open, refuses when it
//! cannot, and an `immutable` read knows when the file changed under it.

use datum_core::db::Db;

fn writer(path: &std::path::Path) -> Db {
    let db = Db::open(path.to_str().unwrap()).unwrap();
    db.conn()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS t(x);
             WITH RECURSIVE s(v) AS (SELECT 1 UNION ALL SELECT v + 1 FROM s WHERE v < 5000)
             INSERT INTO t SELECT 1 FROM s;",
        )
        .unwrap();
    db
}

fn sum(db: &Db) -> i64 {
    db.conn()
        .query_row("SELECT sum(x) FROM t", [], |r| r.get(0))
        .unwrap()
}

/// A writer mid-transaction, its uncommitted pages spilled into the `-wal`:
/// the reader reads what was committed.
#[test]
fn a_reader_reads_the_committed_state_through_a_writers_open_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("town.db");
    let w = writer(&path);
    w.conn()
        .execute_batch("PRAGMA cache_size = 1; BEGIN IMMEDIATE; UPDATE t SET x = 2;")
        .unwrap();
    for f in ["town.db-wal", "town.db-shm"] {
        assert!(dir.path().join(f).exists(), "no {f} beside a writer");
    }
    let r = Db::open_read_only(path.to_str().unwrap()).unwrap();
    assert_eq!(sum(&r), 5000, "read the open transaction's pages");
    assert!(!r.changed_while_read());
    w.conn().execute_batch("ROLLBACK").unwrap();
}

/// A checkpointed file is read `immutable`, which SQLite trusts not to
/// change: a write that lands on it meanwhile is noticed, never trusted.
#[test]
fn a_write_landing_during_an_immutable_read_is_noticed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("town.db");
    drop(writer(&path));
    assert!(!dir.path().join("town.db-wal").exists());
    let r = Db::open_read_only(path.to_str().unwrap()).unwrap();
    assert_eq!(sum(&r), 5000);
    assert!(!r.changed_while_read(), "a read no write met");
    // Past a coarse clock's tick, so the write's mtime is another.
    std::thread::sleep(std::time::Duration::from_millis(20));
    drop(writer(&path));
    assert!(!dir.path().join("town.db-wal").exists());
    assert!(
        r.changed_while_read(),
        "a write during the read went unnoticed"
    );
    let again = Db::open_read_only(path.to_str().unwrap()).unwrap();
    assert_eq!(sum(&again), 10000);
    assert!(!again.changed_while_read());
}

/// A `-shm` with no `-wal` is a writer opening or closing the file: the
/// reader refuses, saying so, and creates nothing.
#[test]
fn a_reader_refuses_a_shm_with_no_wal_as_its_writers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("town.db");
    drop(writer(&path));
    std::fs::write(dir.path().join("town.db-shm"), b"").unwrap();
    let err = Db::open_read_only(path.to_str().unwrap())
        .err()
        .expect("read beside a lone -shm");
    assert!(
        err.to_string().contains("open in its writer's datum"),
        "{err}"
    );
    assert!(!dir.path().join("town.db-wal").exists());
}
