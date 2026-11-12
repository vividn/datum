use super::*;

/// da-wgf4a: `attachment` is reserved as `setting` was; nothing moves
/// migrating up or down.
#[test]
fn roundtrip_v82_to_v83_attachment_is_reserved() {
    let db = fresh_at(82);
    apply_one_up(db.conn(), 82);
    apply_one_down(db.conn(), 83);
}

/// A life field named `attachment` is refused, and nothing changes; a
/// deleted one, or an attachment itself (a v83 peer's), is no clash.
#[test]
fn v82_to_v83_refuses_a_field_already_named_attachment() {
    let db = fresh_at(82);
    let conn = db.conn();
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, data, at, create_time, modify_time,
             client_id)
         VALUES ('a1', 'attachment:ab', 'a1', 'attachment',
                 '{\"name\":\"ab\",\"type\":\"image/png\",\"bytes\":\"\"}',
                 '1970-01-01T00:00:00+00:00', '2026-11-12T00:00:00Z', '2026-11-12T00:00:00Z',
                 'dev-a'),
                ('h1', 'attachment:h1', 'h1', 'attachment', NULL, '2026-11-01T00:00:00+00:00',
                 '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z', 'dev-a')",
        [],
    )
    .unwrap();
    let err = apply_migrations(conn, MIGRATIONS, 82, 83)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("1 entry") && err.contains("`attachment`"),
        "{err}"
    );
    assert_eq!(read_schema_version(conn).unwrap(), Some(82));
    conn.execute(
        "UPDATE entries SET deleted_at = '2026-11-02T00:00:00Z' WHERE hid = 'h1'",
        [],
    )
    .unwrap();
    apply_migrations(conn, MIGRATIONS, 82, 83).unwrap();
}
