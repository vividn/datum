use super::*;

/// da-l6roh: v75 lays `entries_field_deciding` and takes the moments out of
/// `entries_field_blocks`; v74 puts both back.
#[test]
fn roundtrip_v74_to_v75_field_deciding_index() {
    let db = fresh_at(74);
    let conn = db.conn();
    let blocks = || -> String {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'entries_field_blocks'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert!(!index_exists(conn, "entries_field_deciding"));
    assert!(blocks().contains("NOT (verb = 'point'"));
    apply_one_up(conn, 74);
    assert!(index_exists(conn, "entries_field_deciding"));
    assert!(blocks().contains("verb <> 'point'"));
    apply_one_down(conn, 75);
    assert!(!index_exists(conn, "entries_field_deciding"));
    assert!(blocks().contains("NOT (verb = 'point'"));
}
