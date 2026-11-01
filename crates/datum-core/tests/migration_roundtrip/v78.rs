use super::*;

/// da-k5vg1: v78 drops `undo_taken` and puts `undone` back to 0 or 1, keeping an
/// entry already taken back out of what its action's undo and redo move; v77
/// numbers the undone actions again and lays the table, empty.
#[test]
fn roundtrip_v77_to_v78_no_undo_taken() {
    let db = fresh_at(77);
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO undo_actions (id, time, undone, skipped) VALUES
             (1, 't', 0, NULL),  -- part taken, the rest stands
             (2, 't', 2, NULL),  -- undone, a part's redo still held
             (3, 't', 1, '[\"k\"]'),  -- undone, a part's redo dropped
             (4, 't', 0, NULL);
         INSERT INTO undo_taken (action_id, hid, step) VALUES
             (1, 'a', 3), (2, 'b', 2), (3, 'c', -1);",
    )
    .unwrap();
    let rows = || -> Vec<(i64, Option<String>)> {
        conn.prepare("SELECT undone, skipped FROM undo_actions ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    apply_one_up(conn, 77);
    assert!(!table_exists(conn, "undo_taken"));
    assert_eq!(
        rows(),
        [
            (0, Some(r#"["a"]"#.to_string())),
            (1, None),
            (1, Some(r#"["c","k"]"#.to_string())),
            (0, None),
        ]
    );
    assert!(has_column(conn, "undo_actions", "took"));
    apply_one_down(conn, 78);
    assert!(!has_column(conn, "undo_actions", "took"));
    assert!(table_exists(conn, "undo_taken"));
    assert_eq!(rows()[1].0, 2, "the oldest undone is redone first");
}
