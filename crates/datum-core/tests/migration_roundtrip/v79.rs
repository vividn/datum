use super::*;

/// da-8hfta: v79 lays `sync_uncovered` and its triggers, filled with the
/// entries no op covers; v78 takes them away.
#[test]
fn roundtrip_v78_to_v79_sync_uncovered() {
    let db = Db::open_in_memory().unwrap();
    let conn = db.conn();
    let e = datum_core::entry::Entry::new(
        "coffee",
        datum_core::entry::Verb::Point,
        None,
        None,
        None,
        None,
        "test",
    );
    db.insert_entry(&e).unwrap();
    conn.execute_batch("DELETE FROM ops").unwrap();
    apply_migrations_down(conn, MIGRATIONS, CURRENT_SCHEMA_VERSION, 78).unwrap();
    let objects = || {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'sync\\_uncovered%' ESCAPE '\\'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    assert_eq!(objects(), 0);
    apply_one_up(conn, 78);
    // The table and six triggers.
    assert_eq!(objects(), 7);
    let kept: String = conn
        .query_row("SELECT hid FROM sync_uncovered", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kept, e.hid.0);
    apply_one_down(conn, 79);
    assert_eq!(objects(), 0);
}
