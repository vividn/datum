//! Concurrency configuration tests (da-11qq).
//!
//! File-backed opens must configure SQLite for datum's
//! daemon-reads-while-CLI-writes design: an explicit busy timeout
//! (writer-vs-writer overlap waits instead of erroring), WAL journaling
//! (readers never block the writer — in rollback-journal mode a reader
//! holding a transaction BUSYs any writer once the timeout expires), and
//! `synchronous = NORMAL` (the standard WAL pairing). The pragma tests
//! pin the configuration; the behavioral tests drive two independent
//! connections to the same file and require every operation to succeed.

use std::sync::{Arc, Barrier};
use std::thread;

use datum_core::db::Db;
use datum_core::test_util::entry_builder;

fn assert_concurrency_pragmas(db: &Db) {
    let journal: String = db
        .conn()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(journal.to_lowercase(), "wal");

    let timeout: i64 = db
        .conn()
        .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
        .unwrap();
    assert_eq!(timeout, 5000);

    // synchronous: 1 == NORMAL
    let synchronous: i64 = db
        .conn()
        .query_row("PRAGMA synchronous", [], |r| r.get(0))
        .unwrap();
    assert_eq!(synchronous, 1);
}

#[test]
fn open_configures_concurrency_pragmas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Db::open(path.to_str().unwrap()).unwrap();
    assert_concurrency_pragmas(&db);
}

#[test]
fn open_unmigrated_configures_concurrency_pragmas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = Db::open_unmigrated(path.to_str().unwrap()).unwrap();
    assert_concurrency_pragmas(&db);
}

/// Deterministic WAL regression test: in rollback-journal mode a reader
/// holding an open transaction (SHARED lock) blocks any writer — the
/// insert below would stall for the 5s busy timeout and then return
/// SQLITE_BUSY. In WAL the reader has a snapshot and the writer proceeds
/// immediately. This is exactly the sync-agent-pulls-while-CLI-writes
/// shape.
#[test]
fn writer_proceeds_while_reader_holds_open_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let path = path.to_str().unwrap();

    let reader = Db::open(path).unwrap();
    let writer = Db::open(path).unwrap();

    // Open a read transaction and actually read so the snapshot/lock is
    // taken, then hold it across the write.
    reader.conn().execute_batch("BEGIN").unwrap();
    let _: i64 = reader
        .conn()
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .unwrap();

    entry_builder("coffee").insert(&writer);

    reader.conn().execute_batch("COMMIT").unwrap();
}

#[test]
fn concurrent_writer_and_reader_no_busy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let path = path.to_str().unwrap().to_string();

    // Create + migrate up front so the threads race on entry traffic,
    // not on first-time schema creation.
    Db::open(&path).unwrap();

    let barrier = Arc::new(Barrier::new(2));

    let writer = {
        let path = path.clone();
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            let db = Db::open(&path).unwrap();
            barrier.wait();
            for _ in 0..200 {
                // EntryBuilder::insert panics on any error — a spurious
                // SQLITE_BUSY fails the test here.
                entry_builder("coffee").insert(&db);
            }
        })
    };

    let reader = {
        let path = path.clone();
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            let db = Db::open(&path).unwrap();
            barrier.wait();
            for _ in 0..200 {
                let _count: i64 = db
                    .conn()
                    .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
                    .unwrap();
            }
        })
    };

    writer.join().unwrap();
    reader.join().unwrap();
}
