use super::*;

/// da-52vdj: v81 gives `sync_peers` a `floor`, the rows before NULL; v80
/// takes it away and keeps the rows.
#[test]
fn roundtrip_v80_to_v81_peer_floor() {
    let db = Db::open_in_memory().unwrap();
    let conn = db.conn();
    apply_migrations_down(conn, MIGRATIONS, CURRENT_SCHEMA_VERSION, 80).unwrap();
    assert!(!column_exists(conn, "sync_peers", "floor"));
    conn.execute(
        "INSERT INTO sync_peers (endpoint_id, label, addrs, added_at, schema)
         VALUES ('e', 'phone', '{}', '2026-11-03T00:00:00Z', 80)",
        [],
    )
    .unwrap();
    apply_one_up(conn, 80);
    let floor: Option<u32> = conn
        .query_row(
            "SELECT floor FROM sync_peers WHERE endpoint_id = 'e'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(floor, None, "a row before says no floor");
    apply_one_down(conn, 81);
    assert!(!column_exists(conn, "sync_peers", "floor"));
    let schema: u32 = conn
        .query_row(
            "SELECT schema FROM sync_peers WHERE endpoint_id = 'e'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(schema, 80);
}
