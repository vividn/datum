use super::*;

/// da-7z5i9: v81 lays `dayview_kept`, its dirty marks and their triggers;
/// v80 takes them away.
#[test]
fn roundtrip_v80_to_v81_dayview_kept() {
    let db = Db::open_in_memory().unwrap();
    let conn = db.conn();
    apply_migrations_down(conn, MIGRATIONS, CURRENT_SCHEMA_VERSION, 80).unwrap();
    let objects = || {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
              WHERE name LIKE 'dayview\\_kept%' ESCAPE '\\' AND type IN ('table', 'trigger')",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    assert_eq!(objects(), 0);
    apply_one_up(conn, 80);
    // The two tables and six triggers.
    assert_eq!(objects(), 8);
    apply_one_down(conn, 81);
    assert_eq!(objects(), 0);
}
