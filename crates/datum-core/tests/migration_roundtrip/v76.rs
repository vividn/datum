use super::*;

/// da-95avu: v76 calls the time `at`, its old names virtual columns reading the
/// same value, and its four named indexes after it; v75 puts all of it back, a
/// time and a conflict on it as they were.
#[test]
fn roundtrip_v75_to_v76_at() {
    let db = fresh_at(75);
    let conn = db.conn();
    conn.execute_batch(
        r#"INSERT INTO entries (hid, slug, content_hash, field, verb, create_time, modify_time,
               client_id, occur_time, occur_time_offset, occur_time_tz, occur_time_uncertainty,
               conflicts)
           VALUES ('h', 'f:t', 'c', 'f', 'point', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
               'd', '2026-01-01T00:00:00+00:00', 60, 'Europe/Berlin', 'PT5M',
               '{"occur_time":[],"state":[]}')"#,
    )
    .unwrap();
    let time = |name: &str| -> String {
        conn.query_row(
            &format!("SELECT {name} || '/' || {name}_offset || '/' || {name}_tz || '/' || {name}_uncertainty FROM entries"),
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    let conflicts = || -> String {
        conn.query_row("SELECT conflicts FROM entries", [], |r| r.get(0))
            .unwrap()
    };
    let want = "2026-01-01T00:00:00+00:00/60/Europe/Berlin/PT5M";
    assert_eq!(time("occur_time"), want);

    apply_one_up(conn, 75);
    assert_eq!(time("at"), want);
    assert_eq!(time("occur_time"), want, "the old names read the same");
    assert_eq!(conflicts(), r#"{"at":[],"state":[]}"#);
    for index in [
        "entries_at",
        "entries_field_at",
        "entries_field_at_unique",
        "entries_live_field_at",
    ] {
        assert!(index_exists(conn, index), "{index}");
    }
    assert!(!index_exists(conn, "entries_occur_time"));
    assert!(column_exists(conn, "entries", "at"));
    // A virtual column is written by nothing.
    assert!(conn
        .execute("UPDATE entries SET occur_time = 'x'", [])
        .is_err());

    apply_one_down(conn, 76);
    assert_eq!(time("occur_time"), want);
    assert!(!column_exists(conn, "entries", "at"));
    assert_eq!(conflicts(), r#"{"occur_time":[],"state":[]}"#);
    assert!(index_exists(conn, "entries_occur_time"));
    assert!(!index_exists(conn, "entries_at"));
}
