use super::*;

/// da-il29m: v77 numbers the actions already undone, the oldest highest so it
/// is still redone first, and lays `undo_taken`; v76 puts `undone` back to 1.
#[test]
fn roundtrip_v76_to_v77_undo_taken() {
    let db = fresh_at(76);
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO undo_actions (id, time, undone) VALUES
             (1, 't', 0), (2, 't', 1), (3, 't', 1), (4, 't', 1);",
    )
    .unwrap();
    let undone = || -> Vec<i64> {
        conn.prepare("SELECT undone FROM undo_actions ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let index = || -> String {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'undo_actions_undone'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert!(!table_exists(conn, "undo_taken"));
    apply_one_up(conn, 76);
    assert!(table_exists(conn, "undo_taken"));
    assert_eq!(undone(), [0, 3, 2, 1]);
    assert!(index().contains("undone <> 0"));
    apply_one_down(conn, 77);
    assert!(!table_exists(conn, "undo_taken"));
    assert_eq!(undone(), [0, 1, 1, 1]);
    assert!(index().contains("undone = 1"));
}
