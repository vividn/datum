use super::*;

/// da-zpc98: v80 gives `entry_history` and `entries` an `author`, the rows
/// before NULL, and the trigger that keeps an entry's its latest change's;
/// v79 takes all three away and keeps the rows.
#[test]
fn roundtrip_v79_to_v80_author() {
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
    apply_migrations_down(conn, MIGRATIONS, CURRENT_SCHEMA_VERSION, 79).unwrap();
    let trigger = || {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'entry_history_author'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    assert_eq!(trigger(), 0);
    assert!(!column_exists(conn, "entries", "author"));
    assert!(!column_exists(conn, "entry_history", "author"));
    apply_one_up(conn, 79);
    assert_eq!(trigger(), 1);
    let authors: (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT e.author, h.author FROM entries e JOIN entry_history h ON h.entry_hid = e.hid
              WHERE e.hid = ?1",
            [&e.hid.0],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(authors, (None, None), "the rows before name nobody");
    apply_one_down(conn, 80);
    assert_eq!(trigger(), 0);
    assert!(!column_exists(conn, "entries", "author"));
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entry_history WHERE entry_hid = ?1",
            [&e.hid.0],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 1);
}
