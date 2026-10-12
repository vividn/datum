//! `Db::open_read_only` beside a hot rollback journal (da-yajgu): a write
//! cut mid-transaction leaves its pages in the file and the originals in
//! `<db>-journal`. That is every write on WASI, which has no WAL. A reader
//! must see the rolled-back pages or refuse, never the cut write's.

use datum_core::db::Db;
use rusqlite::Connection;

#[test]
fn a_read_only_open_never_reads_a_cut_write() {
    let dir = tempfile::tempdir().unwrap();
    let live = dir.path().join("live.db");
    let conn = Connection::open(&live).unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode=DELETE; PRAGMA cache_size=1; CREATE TABLE t(x);
         WITH RECURSIVE s(v) AS (SELECT 1 UNION ALL SELECT v + 1 FROM s WHERE v < 5000)
         INSERT INTO t SELECT v FROM s;",
    )
    .unwrap();

    // A write over every page, too big for the cache, spills its pages into
    // the file before it commits; then the file and its journal are copied
    // out as a crash leaves them.
    conn.execute_batch("BEGIN; UPDATE t SET x = -x;").unwrap();
    let cut = dir.path().join("cut.db");
    std::fs::copy(&live, &cut).unwrap();
    let journal = std::fs::copy(
        dir.path().join("live.db-journal"),
        dir.path().join("cut.db-journal"),
    )
    .unwrap();
    assert!(journal > 0, "the cut write left no journal");

    let cut_rows = Db::open_read_only(cut.to_str().unwrap()).and_then(|db| {
        Ok(db
            .conn()
            .query_row("SELECT count(*) FROM t WHERE x < 0", [], |r| {
                r.get::<_, i64>(0)
            })?)
    });
    assert!(
        !matches!(cut_rows, Ok(n) if n > 0),
        "read the cut write: {cut_rows:?}"
    );
}
