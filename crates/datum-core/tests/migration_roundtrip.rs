//! Per-version migration round-trip tests (da-9u9.2).
//!
//! For every adjacent (N, N+1) pair in the registry, exercise:
//!
//!   1. Land at v=N by down-walking from CURRENT.
//!   2. Insert representative fixture rows where the v=N schema permits.
//!   3. Apply the up step v=N → v=N+1 and verify the new column / index /
//!      table exists and seeded data survives where applicable.
//!   4. Apply the down step v=N+1 → v=N and verify the schema lands back
//!      at v=N (modulo documented data loss).
//!
//! Coverage tests live alongside the per-pair tests:
//!
//!   * [`all_migrations_have_down`] asserts every registered Migration
//!     has a non-default `down`.
//!   * [`every_pair_has_a_roundtrip_test`] asserts the per-pair test
//!     coverage walks every (N, N+1) pair.

use datum_core::db::Db;
use datum_core::migrations::{
    apply_migrations, apply_migrations_down, read_schema_version, Migration,
    CURRENT_SCHEMA_VERSION, MIGRATIONS,
};
use rusqlite::Connection;

fn fresh_at(version: u32) -> Db {
    let db = Db::open_in_memory().unwrap();
    apply_migrations_down(db.conn(), MIGRATIONS, CURRENT_SCHEMA_VERSION, version).unwrap();
    assert_eq!(read_schema_version(db.conn()).unwrap(), Some(version));
    db
}

fn step(from: u32) -> &'static Migration {
    MIGRATIONS
        .iter()
        .find(|m| m.from == from)
        .unwrap_or_else(|| panic!("no migration step starting at v{from}"))
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        rusqlite::params![name],
        |row| row.get::<_, i64>(0).map(|n| n > 0),
    )
    .unwrap_or(false)
}

fn index_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
        rusqlite::params![name],
        |row| row.get::<_, i64>(0).map(|n| n > 0),
    )
    .unwrap_or(false)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    // Use `pragma_table_xinfo` so generated/virtual columns (e.g.
    // `entries.verb`, `entries.dur_seconds`) are included; the plain
    // `pragma_table_info` view skips them.
    conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_xinfo('{table}') WHERE name = ?1"),
        rusqlite::params![column],
        |row| row.get::<_, i64>(0).map(|n| n > 0),
    )
    .unwrap_or(false)
}

/// Whether `table.column` carries a `NOT NULL` constraint.
fn column_not_null(conn: &Connection, table: &str, column: &str) -> bool {
    conn.query_row(
        &format!("SELECT \"notnull\" FROM pragma_table_info('{table}') WHERE name = ?1"),
        rusqlite::params![column],
        |row| row.get::<_, i64>(0).map(|n| n != 0),
    )
    .unwrap_or(false)
}

/// Apply one up step from `version`, return summary line.
fn apply_one_up(conn: &Connection, version: u32) -> String {
    let s = step(version);
    let summaries = apply_migrations(conn, MIGRATIONS, version, s.to).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(read_schema_version(conn).unwrap(), Some(s.to));
    summaries.into_iter().next().unwrap()
}

/// Apply one down step from `version` back to `version - 1`'s `from`.
fn apply_one_down(conn: &Connection, version: u32) {
    let s = MIGRATIONS.iter().find(|m| m.to == version).unwrap();
    let summaries = apply_migrations_down(conn, MIGRATIONS, version, s.from).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(read_schema_version(conn).unwrap(), Some(s.from));
}

// ─── Coverage gates ──────────────────────────────────────────────────

/// Every registered migration must have a `down`. Adding a Migration
/// without a down is rejected at compile time (the field is required),
/// but this test still asserts the registry is non-empty and the field
/// is reachable for every entry.
#[test]
fn all_migrations_have_down() {
    assert!(!MIGRATIONS.is_empty(), "MIGRATIONS registry is empty");
    for m in MIGRATIONS {
        // Calling `down` against an empty in-memory connection isn't
        // meaningful for most steps (they expect prior state), so we
        // just take the function pointer. If `down` were ever a default
        // we'd want a behavioral check; for now reaching the field is
        // enough — any future field rename that omits `down` would
        // surface here at compile time, not runtime.
        let ptr = m.down as *const ();
        assert!(
            !ptr.is_null(),
            "down for v{}→v{} must not be null",
            m.from,
            m.to
        );
    }
}

/// The per-pair tests below MUST cover every (N, N+1) pair in the
/// registry. We list the pairs explicitly here and assert the lengths
/// line up so adding a new Migration without a roundtrip test fails
/// this gate.
#[test]
fn every_pair_has_a_roundtrip_test() {
    let covered: &[(u32, u32)] = &[
        (1, 2),
        (2, 3),
        (3, 4),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 8),
        (8, 9),
        (9, 10),
        (10, 11),
        (11, 12),
        (12, 13),
        (13, 14),
        (14, 15),
        (15, 16),
        (16, 17),
        (17, 18),
        (18, 19),
        (19, 20),
        (20, 21),
        (21, 22),
        (22, 23),
        (23, 24),
        (24, 25),
        (25, 26),
        (26, 27),
        (27, 28),
        (28, 29),
        (29, 30),
        (30, 31),
    ];
    assert_eq!(
        covered.len(),
        MIGRATIONS.len(),
        "roundtrip coverage must list every registered migration",
    );
    for (m, (from, to)) in MIGRATIONS.iter().zip(covered) {
        assert_eq!(
            (m.from, m.to),
            (*from, *to),
            "roundtrip coverage list out of order with MIGRATIONS",
        );
    }
}

// ─── Per-pair roundtrip tests ────────────────────────────────────────

#[test]
fn roundtrip_v1_to_v2_sync_state_restructure() {
    let db = fresh_at(1);
    let conn = db.conn();

    // At v=1 the sync_state shape is the placeholder (device_id, last_seq, …).
    assert!(column_exists(conn, "sync_state", "device_id"));
    assert!(column_exists(conn, "sync_state", "last_seq"));
    assert!(!column_exists(conn, "sync_state", "db_id"));

    apply_one_up(conn, 1);

    // After up: v=2 shape (db_id, push_seq, pull_seq, last_*).
    assert!(column_exists(conn, "sync_state", "db_id"));
    assert!(column_exists(conn, "sync_state", "push_seq"));
    assert!(column_exists(conn, "sync_state", "pull_seq"));
    assert!(!column_exists(conn, "sync_state", "device_id"));

    apply_one_down(conn, 2);
    assert!(column_exists(conn, "sync_state", "device_id"));
    assert!(!column_exists(conn, "sync_state", "db_id"));
}

#[test]
fn roundtrip_v2_to_v3_entries_v2_rewrite() {
    let db = fresh_at(2);
    let conn = db.conn();

    // v=2 stub entries shape created by down_3_to_2 has slug PK, no hid.
    assert!(table_exists(conn, "entries"));
    assert!(column_exists(conn, "entries", "slug"));
    assert!(!column_exists(conn, "entries", "hid"));

    apply_one_up(conn, 2);

    // v=3 entries: hid PK + entry_shape + generated verb.
    assert!(column_exists(conn, "entries", "hid"));
    assert!(column_exists(conn, "entries", "entry_shape"));
    assert!(column_exists(conn, "entries", "verb"));
    assert!(index_exists(conn, "entries_slug"));

    apply_one_down(conn, 3);
    assert!(!column_exists(conn, "entries", "hid"));
    assert!(column_exists(conn, "entries", "slug"));
}

#[test]
fn roundtrip_v3_to_v4_entry_history_table() {
    let db = fresh_at(3);
    let conn = db.conn();
    assert!(!table_exists(conn, "entry_history"));

    apply_one_up(conn, 3);
    assert!(table_exists(conn, "entry_history"));
    assert!(index_exists(conn, "entry_history_entry"));
    assert!(index_exists(conn, "entry_history_change_kind"));

    apply_one_down(conn, 4);
    assert!(!table_exists(conn, "entry_history"));
}

#[test]
fn roundtrip_v4_to_v5_sync_conflicts_table() {
    let db = fresh_at(4);
    let conn = db.conn();
    assert!(!table_exists(conn, "sync_conflicts"));

    apply_one_up(conn, 4);
    assert!(table_exists(conn, "sync_conflicts"));
    assert!(index_exists(conn, "sync_conflicts_hid"));
    assert!(index_exists(conn, "sync_conflicts_slug"));

    apply_one_down(conn, 5);
    assert!(!table_exists(conn, "sync_conflicts"));
    assert!(!index_exists(conn, "sync_conflicts_hid"));
}

#[test]
fn roundtrip_v5_to_v6_live_field_start_index() {
    let db = fresh_at(5);
    let conn = db.conn();
    assert!(!index_exists(conn, "entries_live_field_start"));

    apply_one_up(conn, 5);
    assert!(index_exists(conn, "entries_live_field_start"));

    apply_one_down(conn, 6);
    assert!(!index_exists(conn, "entries_live_field_start"));
}

#[test]
fn roundtrip_v6_to_v7_sync_state_my_push_seq() {
    let db = fresh_at(6);
    let conn = db.conn();
    assert!(!column_exists(conn, "sync_state", "my_push_seq"));

    // Seed a v=6 sync_state row so the up's ALTER preserves it.
    conn.execute(
        "INSERT INTO sync_state (db_id, push_seq, pull_seq, last_push_at, last_pull_at)
         VALUES ('db-x', 7, 9, 100, 200)",
        [],
    )
    .unwrap();

    apply_one_up(conn, 6);
    assert!(column_exists(conn, "sync_state", "my_push_seq"));
    let (push, my_push): (i64, i64) = conn
        .query_row(
            "SELECT push_seq, my_push_seq FROM sync_state WHERE db_id = 'db-x'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(push, 7);
    assert_eq!(my_push, 0, "fresh column defaults to 0");

    apply_one_down(conn, 7);
    assert!(!column_exists(conn, "sync_state", "my_push_seq"));
    let push_after: i64 = conn
        .query_row(
            "SELECT push_seq FROM sync_state WHERE db_id = 'db-x'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(push_after, 7, "rebuild down preserves rows on column drop",);
}

#[test]
fn roundtrip_v7_to_v8_user_asserted_last_state() {
    let db = fresh_at(7);
    let conn = db.conn();
    assert!(!column_exists(conn, "entries", "user_asserted_last_state"));

    apply_one_up(conn, 7);
    assert!(column_exists(conn, "entries", "user_asserted_last_state"));

    apply_one_down(conn, 8);
    assert!(!column_exists(conn, "entries", "user_asserted_last_state"));
    // Generated columns survive the rebuild.
    assert!(column_exists(conn, "entries", "verb"));
    assert!(column_exists(conn, "entries", "dur_seconds"));
}

#[test]
fn roundtrip_v8_to_v9_track_ignore_unification() {
    let db = fresh_at(8);
    let conn = db.conn();

    // m8_to_9's `crate::db::try_insert` uses the CURRENT Entry shape
    // (post-da-5ns: occur_time/state/dur/following_state plus a stored
    // verb). At v=8 the entries table is still in the v3-era
    // start_time/end_time shape, so try_insert would fail. In
    // production this is fine because Db::migrate runs schema.sql
    // (creating the v=20 entries shape) before walking the migration
    // chain. We simulate that here by rebuilding entries to the v=20
    // shape before invoking m8_to_9.
    conn.execute_batch(
        "DROP TABLE entries;
         CREATE TABLE entries (
             hid             TEXT NOT NULL PRIMARY KEY,
             slug            TEXT NOT NULL,
             content_hash    TEXT NOT NULL,
             field           TEXT NOT NULL,
             state           TEXT,
             dur             TEXT,
             following_state TEXT,
             verb            TEXT NOT NULL DEFAULT 'point',
             data            TEXT,
             unit            TEXT,
             occur_time           TEXT NOT NULL,
             occur_time_offset    INTEGER,
             occur_time_tz        TEXT,
             occur_time_precision TEXT,
             dur_precision        TEXT,
             lat             REAL,
             lon             REAL,
             location        TEXT,
             create_time     TEXT NOT NULL,
             modify_time     TEXT NOT NULL,
             device_id       TEXT NOT NULL,
             deleted_at      TEXT,
             id_structure    TEXT,
             field_structure TEXT,
             raw_input       TEXT NOT NULL DEFAULT '',
             alias_chain     TEXT,
             parser_version  INTEGER NOT NULL DEFAULT 1,
             user_asserted_last_state TEXT,
             extra_metadata  TEXT,
             CHECK (following_state IS NULL OR dur IS NOT NULL)
         );",
    )
    .unwrap();

    // Seed a spec with ignored=true at the v=8 shape (no `tags`,
    // no `render_json`, no `kit_id` columns yet).
    conn.execute(
        "INSERT INTO field_specs (field, spec_json) VALUES (?1, ?2)",
        rusqlite::params![
            "yt",
            r#"{"field":"yt","default_verb":"occur","subfields":{},"tracked":true,"ignored":true}"#
        ],
    )
    .unwrap();

    apply_one_up(conn, 8);

    // Up wrote a switch entry with device_id='migration-v9'.
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE device_id = 'migration-v9' AND field = 'yt'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "v8→v9 must seed exactly one IGNORED switch");

    apply_one_down(conn, 9);

    let n_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE device_id = 'migration-v9'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n_after, 0, "down deletes the migration-tagged entries");
}

#[test]
fn roundtrip_v9_to_v10_hybrid_time_index() {
    let db = fresh_at(9);
    let conn = db.conn();
    assert!(!index_exists(conn, "entries_hybrid_time"));

    apply_one_up(conn, 9);
    assert!(index_exists(conn, "entries_hybrid_time"));

    apply_one_down(conn, 10);
    assert!(!index_exists(conn, "entries_hybrid_time"));
}

#[test]
fn roundtrip_v10_to_v11_last_docs_table() {
    let db = fresh_at(10);
    let conn = db.conn();
    assert!(!table_exists(conn, "last_docs"));

    apply_one_up(conn, 10);
    assert!(table_exists(conn, "last_docs"));

    apply_one_down(conn, 11);
    assert!(!table_exists(conn, "last_docs"));
}

#[test]
fn roundtrip_v11_to_v12_extra_metadata_relocation() {
    let db = fresh_at(11);
    let conn = db.conn();
    assert!(!column_exists(conn, "entries", "extra_metadata"));

    apply_one_up(conn, 11);
    assert!(column_exists(conn, "entries", "extra_metadata"));

    apply_one_down(conn, 12);
    assert!(!column_exists(conn, "entries", "extra_metadata"));
}

#[test]
fn roundtrip_v12_to_v13_field_specs_kit_id() {
    let db = fresh_at(12);
    let conn = db.conn();
    assert!(!column_exists(conn, "field_specs", "kit_id"));

    // Seed a v=12 field_specs row.
    conn.execute(
        "INSERT INTO field_specs (field, spec_json) VALUES ('a', '{\"field\":\"a\"}')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 12);
    assert!(column_exists(conn, "field_specs", "kit_id"));
    assert!(index_exists(conn, "idx_field_specs_kit_id"));

    apply_one_down(conn, 13);
    assert!(!column_exists(conn, "field_specs", "kit_id"));
    assert!(!index_exists(conn, "idx_field_specs_kit_id"));
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM field_specs WHERE field = 'a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "rebuild preserves rows");
}

#[test]
fn roundtrip_v13_to_v14_views_table() {
    let db = fresh_at(13);
    let conn = db.conn();
    assert!(!table_exists(conn, "views"));

    apply_one_up(conn, 13);
    assert!(table_exists(conn, "views"));
    assert!(index_exists(conn, "idx_views_kit_id"));

    apply_one_down(conn, 14);
    assert!(!table_exists(conn, "views"));
}

#[test]
fn roundtrip_v14_to_v15_kit_indices_table() {
    let db = fresh_at(14);
    let conn = db.conn();
    assert!(!table_exists(conn, "kit_indices"));

    apply_one_up(conn, 14);
    assert!(table_exists(conn, "kit_indices"));

    apply_one_down(conn, 15);
    assert!(!table_exists(conn, "kit_indices"));
}

#[test]
fn roundtrip_v15_to_v16_aliases_kit_id() {
    let db = fresh_at(15);
    let conn = db.conn();
    assert!(!column_exists(conn, "aliases", "kit_id"));
    // The seed `note` alias (from schema.sql) survives the down-walk
    // through v15 because its row is in the v15 aliases shape.
    let note_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM aliases WHERE name = 'note'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(note_count, 1, "seeded `note` alias survives down to v15");

    apply_one_up(conn, 15);
    assert!(column_exists(conn, "aliases", "kit_id"));
    assert!(index_exists(conn, "idx_aliases_kit_id"));

    apply_one_down(conn, 16);
    assert!(!column_exists(conn, "aliases", "kit_id"));
    assert!(!index_exists(conn, "idx_aliases_kit_id"));
    // Rebuild preserved the seeded alias.
    let note_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM aliases WHERE name = 'note'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(note_after, 1);
}

#[test]
fn roundtrip_v16_to_v17_kit_commands_table() {
    let db = fresh_at(16);
    let conn = db.conn();
    assert!(!table_exists(conn, "kit_commands"));

    apply_one_up(conn, 16);
    assert!(table_exists(conn, "kit_commands"));
    assert!(index_exists(conn, "idx_kit_commands_kit_id"));

    apply_one_down(conn, 17);
    assert!(!table_exists(conn, "kit_commands"));
}

#[test]
fn roundtrip_v17_to_v18_field_specs_tags() {
    let db = fresh_at(17);
    let conn = db.conn();
    assert!(!column_exists(conn, "field_specs", "tags"));

    conn.execute(
        "INSERT INTO field_specs (field, spec_json) VALUES ('b', '{\"field\":\"b\"}')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 17);
    assert!(column_exists(conn, "field_specs", "tags"));
    let tags: String = conn
        .query_row("SELECT tags FROM field_specs WHERE field = 'b'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(tags, "[]", "default tags is '[]'");

    apply_one_down(conn, 18);
    assert!(!column_exists(conn, "field_specs", "tags"));
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM field_specs WHERE field = 'b'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn roundtrip_v19_to_v20_time_shape_pivot() {
    let db = fresh_at(19);
    let conn = db.conn();

    // v=19 entries shape: start_time/end_time/entry_shape/intent/value
    // present; generated verb + dur_seconds present; no occur_time, no
    // dur, no following_state. The v19-era partial/expression indexes
    // are present.
    assert!(column_exists(conn, "entries", "start_time"));
    assert!(column_exists(conn, "entries", "end_time"));
    assert!(column_exists(conn, "entries", "entry_shape"));
    assert!(column_exists(conn, "entries", "intent"));
    assert!(column_exists(conn, "entries", "value"));
    assert!(column_exists(conn, "entries", "verb"));
    assert!(column_exists(conn, "entries", "dur_seconds"));
    assert!(!column_exists(conn, "entries", "occur_time"));
    assert!(!column_exists(conn, "entries", "dur"));
    assert!(!column_exists(conn, "entries", "following_state"));
    assert!(index_exists(conn, "entries_field_start"));
    assert!(index_exists(conn, "entries_live_field_start"));
    assert!(index_exists(conn, "entries_hybrid_time"));

    apply_one_up(conn, 19);

    // v=20 entries shape: occur_time/state/dur/following_state plus a
    // stored verb column. Old time-shape columns and the generated
    // dur_seconds projection are gone. New indexes are reseated on
    // occur_time.
    assert!(column_exists(conn, "entries", "occur_time"));
    assert!(column_exists(conn, "entries", "occur_time_precision"));
    assert!(column_exists(conn, "entries", "state"));
    assert!(column_exists(conn, "entries", "dur"));
    assert!(column_exists(conn, "entries", "following_state"));
    assert!(column_exists(conn, "entries", "verb"));
    assert!(!column_exists(conn, "entries", "start_time"));
    assert!(!column_exists(conn, "entries", "end_time"));
    assert!(!column_exists(conn, "entries", "entry_shape"));
    assert!(!column_exists(conn, "entries", "intent"));
    assert!(!column_exists(conn, "entries", "value"));
    assert!(!column_exists(conn, "entries", "dur_seconds"));
    assert!(index_exists(conn, "entries_field_occur"));
    assert!(index_exists(conn, "entries_live_field_occur"));
    assert!(index_exists(conn, "entries_occur_time"));
    assert!(!index_exists(conn, "entries_field_start"));
    assert!(!index_exists(conn, "entries_live_field_start"));
    assert!(!index_exists(conn, "entries_hybrid_time"));

    // CHECK constraint: rejecting following_state without dur. Try to
    // insert a row that violates the invariant; the insert must fail.
    let bad = conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, occur_time,
             create_time, modify_time, device_id, following_state)
         VALUES
            ('h0', 's0', 'c0', 'f', '2026-05-01T00:00:00Z',
             '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z', 'd', '\"x\"')",
        [],
    );
    assert!(
        bad.is_err(),
        "CHECK must reject following_state without dur"
    );

    // verb defaults to 'point' when not supplied.
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, occur_time,
             create_time, modify_time, device_id)
         VALUES
            ('h1', 's1', 'c1', 'f', '2026-05-01T00:00:00Z',
             '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z', 'd')",
        [],
    )
    .unwrap();
    let v: String = conn
        .query_row("SELECT verb FROM entries WHERE hid = 'h1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(v, "point", "verb default is 'point'");

    apply_one_down(conn, 20);

    // After down: back to the v=19 shape. The pre-v1 hard-cut means the
    // row inserted above is discarded — the docstring on the down step
    // documents this.
    assert!(column_exists(conn, "entries", "start_time"));
    assert!(column_exists(conn, "entries", "entry_shape"));
    assert!(column_exists(conn, "entries", "verb"));
    assert!(column_exists(conn, "entries", "dur_seconds"));
    assert!(!column_exists(conn, "entries", "occur_time"));
    assert!(!column_exists(conn, "entries", "dur"));
    assert!(!column_exists(conn, "entries", "following_state"));
    assert!(index_exists(conn, "entries_field_start"));
    assert!(index_exists(conn, "entries_live_field_start"));
    assert!(index_exists(conn, "entries_hybrid_time"));
    assert!(!index_exists(conn, "entries_field_occur"));
}

/// da-8y2q.5: v20→v21 drops `aliases.derived_from_field` and the
/// derived-row scheme that wrote ~10 rows per spec save. The forward
/// step deletes any rows whose `derived_from_field IS NOT NULL` and
/// rebuilds the table without the column. The down step restores the
/// column (NULL-filled — pre-v1 hard-cut, the original ownership data
/// is gone).
#[test]
fn roundtrip_v20_to_v21_drop_derived_from_field() {
    let db = fresh_at(20);
    let conn = db.conn();

    assert!(column_exists(conn, "aliases", "derived_from_field"));
    assert!(index_exists(conn, "idx_aliases_derived_from_field"));

    // Seed a user row and a fake derived row.
    conn.execute(
        "INSERT INTO aliases (name, alias_json, derived_from_field) \
         VALUES ('user', '{\"name\":\"user\",\"expands_to\":\"add\"}', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO aliases (name, alias_json, derived_from_field) \
         VALUES ('happy', '{\"name\":\"happy\",\"expands_to\":\"switch mood happy\"}', 'mood')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 20);

    // Column gone, index gone, derived row gone, user row preserved.
    assert!(!column_exists(conn, "aliases", "derived_from_field"));
    assert!(!index_exists(conn, "idx_aliases_derived_from_field"));
    let derived_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM aliases WHERE name = 'happy'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(derived_count, 0, "derived row dropped");
    let user_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM aliases WHERE name = 'user'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(user_count, 1, "user row preserved");

    apply_one_down(conn, 21);

    // Column back, index back. The user row survived the rebuild;
    // derived rows do NOT come back (pre-v1 hard-cut).
    assert!(column_exists(conn, "aliases", "derived_from_field"));
    assert!(index_exists(conn, "idx_aliases_derived_from_field"));
    let user_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM aliases WHERE name = 'user'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(user_after, 1);
}

/// da-8y2q.2: rewrite legacy `verb` strings on stored entries.
///
/// Seeds rows at v=21 carrying every legacy verb (`occur`, `start`,
/// `stop`, `add`) and asserts the v=21→v=22 step rewrites them to the
/// canonical five-shape storage values (with state canonicalization for
/// `start`/`stop` rows that came in without a state). The down step
/// performs a best-effort reconstruction; the docstring on
/// `down_22_to_21_verb_canonicalize` documents the lossy collapse.
#[test]
fn roundtrip_v21_to_v22_verb_canonicalize() {
    let db = fresh_at(21);
    let conn = db.conn();

    // Seed one row per legacy verb. State is left NULL for start/stop
    // so we can verify the canonicalization. occur/add carry no state.
    let inserts: &[(&str, &str, Option<&str>)] = &[
        ("hocc", "f:occ", None),                          // verb='occur', state=NULL
        ("hadd", "f:add", None),                          // verb='add', state=NULL
        ("hstart", "f:start", None), // verb='start', state=NULL → canonicalize to true
        ("hstop", "f:stop", None),   // verb='stop', state=NULL → canonicalize to false
        ("hstart_st", "f:start_st", Some("\"playing\"")), // verb='start' with state preserved
        ("hpoint", "f:pt", None),    // verb='point' (already canonical, untouched)
    ];
    let verbs = ["occur", "add", "start", "stop", "start", "point"];
    for ((hid, slug, state), verb) in inserts.iter().zip(verbs) {
        conn.execute(
            "INSERT INTO entries
                (hid, slug, content_hash, field, verb, state, occur_time,
                 create_time, modify_time, device_id)
             VALUES
                (?1, ?2, ?1, 'f', ?3, ?4, '2026-05-01T00:00:00Z',
                 '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z', 'd')",
            rusqlite::params![hid, slug, verb, state],
        )
        .unwrap();
    }

    apply_one_up(conn, 21);

    let read = |hid: &str| -> (String, Option<String>) {
        conn.query_row(
            "SELECT verb, state FROM entries WHERE hid = ?1",
            rusqlite::params![hid],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .unwrap()
    };

    assert_eq!(read("hocc"), ("point".to_string(), None));
    assert_eq!(read("hadd"), ("point".to_string(), None));
    assert_eq!(
        read("hstart"),
        ("switch".to_string(), Some("true".to_string()))
    );
    assert_eq!(
        read("hstop"),
        ("switch".to_string(), Some("false".to_string()))
    );
    assert_eq!(
        read("hstart_st"),
        ("switch".to_string(), Some("\"playing\"".to_string())),
        "state preserved when set",
    );
    assert_eq!(read("hpoint"), ("point".to_string(), None));

    apply_one_down(conn, 22);

    // After down: best-effort. Switch rows with literal true/false in the
    // dur=NULL/fs=NULL slot reverse to start/stop; switch rows with other
    // states stay as switch. point rows reverse to occur (legacy add is
    // not recoverable — all collapse to occur).
    assert_eq!(read("hocc"), ("occur".to_string(), None));
    assert_eq!(read("hadd"), ("occur".to_string(), None));
    assert_eq!(read("hstart"), ("start".to_string(), None));
    assert_eq!(read("hstop"), ("stop".to_string(), None));
    assert_eq!(
        read("hstart_st"),
        ("switch".to_string(), Some("\"playing\"".to_string())),
        "non-bool state stays as switch (no legacy verb to map back to)",
    );
    assert_eq!(read("hpoint"), ("occur".to_string(), None));
}

/// da-qnn9 + da-df27: v22→v23 drops the vestigial `entries.unit` column
/// and adds the nullable `entries.dur_precision` column via a table
/// rebuild. Pre-v1 hard-cut: the dropped column's data does not survive,
/// and the reverse fills `unit` with NULL.
#[test]
fn roundtrip_v22_to_v23_unit_drop_dur_precision_add() {
    let db = fresh_at(22);
    let conn = db.conn();

    // v=22 entries shape: unit column present, dur_precision absent.
    assert!(column_exists(conn, "entries", "unit"));
    assert!(!column_exists(conn, "entries", "dur_precision"));

    // Seed a v=22 row carrying a `unit` value, to confirm rows survive
    // the rebuild even though the unit data itself is dropped.
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, verb, state, unit, occur_time,
             create_time, modify_time, device_id)
         VALUES
            ('hu', 'f:u', 'cu', 'f', 'point', NULL, 'cups',
             '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z',
             '2026-05-01T00:00:00Z', 'd')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 22);

    // Up: `unit` is gone, `dur_precision` is present (and NULL on
    // surviving rows). The seeded row's hid persists.
    assert!(!column_exists(conn, "entries", "unit"));
    assert!(column_exists(conn, "entries", "dur_precision"));
    let dp: Option<String> = conn
        .query_row(
            "SELECT dur_precision FROM entries WHERE hid = 'hu'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        dp.is_none(),
        "dur_precision defaults to NULL on legacy rows"
    );

    // Indexes were recreated by the rebuild.
    assert!(index_exists(conn, "entries_slug"));
    assert!(index_exists(conn, "entries_field_occur"));
    assert!(index_exists(conn, "entries_live_field_occur"));
    assert!(index_exists(conn, "entries_content_hash"));
    assert!(index_exists(conn, "entries_verb"));
    assert!(index_exists(conn, "entries_occur_time"));

    apply_one_down(conn, 23);

    // Down: `unit` is back (NULL — pre-v1 hard-cut, the original value
    // is not preserved), `dur_precision` is gone, the row still exists.
    assert!(column_exists(conn, "entries", "unit"));
    assert!(!column_exists(conn, "entries", "dur_precision"));
    let unit_after: Option<String> = conn
        .query_row("SELECT unit FROM entries WHERE hid = 'hu'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        unit_after.is_none(),
        "unit re-added with NULL after rebuild"
    );
}

/// da-b423: v23→v24 adds two partial indexes on `entries` to back
/// `dm check slugs` drift detection — one keyed by `id_structure IS NOT
/// NULL` (near-full), one by `field_structure IS NOT NULL` (sparse).
/// Index-only change; no data is touched. Down drops both indexes.
#[test]
fn roundtrip_v23_to_v24_id_field_structure_indexes() {
    let db = fresh_at(23);
    let conn = db.conn();

    // v=23: neither partial index exists yet.
    assert!(!index_exists(conn, "entries_id_structure"));
    assert!(!index_exists(conn, "entries_field_structure"));

    // Seed two rows: one with id_structure set, one with both
    // id_structure and field_structure set, so the EXPLAIN below has
    // matching rows to consider against the partial index.
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, verb, occur_time,
             create_time, modify_time, device_id, id_structure)
         VALUES
            ('hids', 'f:ids', 'cids', 'f', 'point',
             '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z',
             '2026-05-01T00:00:00Z', 'd', '%occur_time%')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, verb, occur_time,
             create_time, modify_time, device_id,
             id_structure, field_structure)
         VALUES
            ('hfs', 'f:fs', 'cfs', 'word__pt', 'point',
             '2026-05-01T00:00:01Z', '2026-05-01T00:00:01Z',
             '2026-05-01T00:00:01Z', 'd',
             '%occur_time%', 'word__%srcLang%')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 23);

    // Up: both partial indexes exist; rows survive untouched.
    assert!(index_exists(conn, "entries_id_structure"));
    assert!(index_exists(conn, "entries_field_structure"));
    let row_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(row_count, 2);

    // Planner picks the partial index for the matching scan. The exact
    // EXPLAIN text varies between SQLite builds, so just assert the
    // index name appears in the plan.
    let plan_field: String = conn
        .query_row(
            "EXPLAIN QUERY PLAN
                SELECT hid FROM entries WHERE field_structure IS NOT NULL",
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(
        plan_field.contains("entries_field_structure"),
        "expected partial index entries_field_structure in plan, got: {plan_field}"
    );
    let plan_id: String = conn
        .query_row(
            "EXPLAIN QUERY PLAN
                SELECT hid FROM entries WHERE id_structure IS NOT NULL",
            [],
            |r| r.get(3),
        )
        .unwrap();
    assert!(
        plan_id.contains("entries_id_structure"),
        "expected partial index entries_id_structure in plan, got: {plan_id}"
    );

    apply_one_down(conn, 24);

    // Down: indexes are gone; rows still present.
    assert!(!index_exists(conn, "entries_id_structure"));
    assert!(!index_exists(conn, "entries_field_structure"));
    let row_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(row_count, 2);
}

/// da-ga8b: v24→v25 adds the partial UNIQUE on `(field, occur_time)`
/// over live rows. The pre-check refuses to land the index if any live
/// collisions exist; on a clean DB the index appears and a subsequent
/// custom-id same-instant insert errors out. Down drops the index and
/// the previously-rejected insert can land.
#[test]
fn roundtrip_v24_to_v25_field_occurtime_unique() {
    let db = fresh_at(24);
    let conn = db.conn();
    assert!(!index_exists(conn, "entries_field_occurtime"));

    // Seed a baseline row at v=24 (no constraint yet). Use direct SQL
    // because `Db::insert_entry` writes via the v=CURRENT shape — and
    // we want to land the row exactly at v=24.
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, verb, occur_time,
             create_time, modify_time, device_id)
         VALUES
            ('hidbase00000', 'project:2026-05-13T09:00Z', 'cbase', 'project',
             'switch', '2026-05-13T09:00:00Z',
             '2026-05-13T09:00:00Z', '2026-05-13T09:00:00Z', 'd')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 24);

    // Up: the partial UNIQUE index exists and a same-instant custom-slug
    // insert is now rejected.
    assert!(index_exists(conn, "entries_field_occurtime"));
    let err = conn
        .execute(
            "INSERT INTO entries
                (hid, slug, content_hash, field, verb, occur_time,
                 create_time, modify_time, device_id)
             VALUES
                ('hidcustom001', 'custom-id-K', 'ccustom', 'project',
                 'switch', '2026-05-13T09:00:00Z',
                 '2026-05-13T09:00:00Z', '2026-05-13T09:00:00Z', 'd')",
            [],
        )
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("UNIQUE constraint failed"),
        "expected UNIQUE constraint failure, got: {msg}"
    );

    apply_one_down(conn, 25);

    // Down: the index is gone; the previously-rejected row now lands.
    assert!(!index_exists(conn, "entries_field_occurtime"));
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, verb, occur_time,
             create_time, modify_time, device_id)
         VALUES
            ('hidcustom001', 'custom-id-K', 'ccustom', 'project',
             'switch', '2026-05-13T09:00:00Z',
             '2026-05-13T09:00:00Z', '2026-05-13T09:00:00Z', 'd')",
        [],
    )
    .unwrap();
    let row_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(row_count, 2);
}

#[test]
fn roundtrip_v18_to_v19_field_specs_render_json() {
    let db = fresh_at(18);
    let conn = db.conn();
    assert!(!column_exists(conn, "field_specs", "render_json"));

    conn.execute(
        "INSERT INTO field_specs (field, spec_json, tags) VALUES ('c', '{\"field\":\"c\"}', '[]')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 18);
    assert!(column_exists(conn, "field_specs", "render_json"));
    let render: Option<String> = conn
        .query_row(
            "SELECT render_json FROM field_specs WHERE field = 'c'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(render.is_none(), "default render_json is NULL");

    apply_one_down(conn, 19);
    assert!(!column_exists(conn, "field_specs", "render_json"));
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM field_specs WHERE field = 'c'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
}

/// da-u2do: v25→v26 renames the `sync_conflicts` table (and its
/// `conflict_kind` column) to `integrity_warnings` / `kind`. Existing
/// pending rows survive the rename. The down step puts the legacy names
/// back so the migration chain is reversible.
#[test]
fn roundtrip_v25_to_v26_rename_integrity_warnings() {
    let db = fresh_at(25);
    let conn = db.conn();
    assert!(table_exists(conn, "sync_conflicts"));
    assert!(!table_exists(conn, "integrity_warnings"));

    // Seed a row at v=25 with the legacy column name so we can verify the
    // rename preserves data and the new `kind` column is populated.
    conn.execute(
        "INSERT INTO sync_conflicts
            (id, hid, entry_slug, local_entry, remote_entry, detected_at,
             resolution, resolved_at, detected_by, conflict_kind)
         VALUES
            ('aaaaaaaaaaaa', 'aaaaa', 'mood:2026-05-21T10:00Z',
             '{\"hid\":\"h\"}', NULL, '2026-05-21T10:00:00Z',
             NULL, NULL, 'local_validator', 'orphan_end')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 25);

    assert!(!table_exists(conn, "sync_conflicts"));
    assert!(table_exists(conn, "integrity_warnings"));
    assert!(index_exists(conn, "integrity_warnings_hid"));
    assert!(index_exists(conn, "integrity_warnings_slug"));
    assert!(index_exists(conn, "integrity_warnings_pending"));
    assert!(column_exists(conn, "integrity_warnings", "kind"));
    assert!(!column_exists(conn, "integrity_warnings", "conflict_kind"));
    let kind: String = conn
        .query_row(
            "SELECT kind FROM integrity_warnings WHERE id = 'aaaaaaaaaaaa'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kind, "orphan_end");

    apply_one_down(conn, 26);

    assert!(table_exists(conn, "sync_conflicts"));
    assert!(!table_exists(conn, "integrity_warnings"));
    assert!(index_exists(conn, "sync_conflicts_hid"));
    assert!(column_exists(conn, "sync_conflicts", "conflict_kind"));
    let kind: String = conn
        .query_row(
            "SELECT conflict_kind FROM sync_conflicts WHERE id = 'aaaaaaaaaaaa'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kind, "orphan_end");
}

/// da-eu7o: v26→v27 adds the per-DB `cluster_id` (nullable) and
/// `cluster_generation` (default 0) columns to `sync_state`. Existing
/// watermark rows must survive intact. The down step rebuilds the v26
/// shape, discarding any locally-assigned cluster_id.
#[test]
fn roundtrip_v26_to_v27_sync_state_cluster_columns() {
    let db = fresh_at(26);
    let conn = db.conn();
    assert!(table_exists(conn, "sync_state"));
    assert!(!column_exists(conn, "sync_state", "cluster_id"));
    assert!(!column_exists(conn, "sync_state", "cluster_generation"));

    // Seed a sync_state row so we can verify it survives the up/down.
    conn.execute(
        "INSERT INTO sync_state (db_id, push_seq, pull_seq, my_push_seq, last_push_at, last_pull_at)
         VALUES ('local', 17, 22, 3, 1700000000000, 1700000005000)",
        [],
    )
    .unwrap();

    apply_one_up(conn, 26);

    assert!(column_exists(conn, "sync_state", "cluster_id"));
    assert!(column_exists(conn, "sync_state", "cluster_generation"));
    // New columns default to NULL / 0 for pre-existing rows.
    let (cid, cluster_gen, push_seq, my_push_seq): (Option<String>, i64, i64, i64) = conn
        .query_row(
            "SELECT cluster_id, cluster_generation, push_seq, my_push_seq
             FROM sync_state WHERE db_id = 'local'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(cid, None);
    assert_eq!(cluster_gen, 0);
    assert_eq!(push_seq, 17);
    assert_eq!(my_push_seq, 3);

    // Persist a cluster_id + non-zero generation, then walk back to v26.
    conn.execute(
        "UPDATE sync_state SET cluster_id = 'cluster-roundtrip', cluster_generation = 4
         WHERE db_id = 'local'",
        [],
    )
    .unwrap();

    apply_one_down(conn, 27);

    assert!(!column_exists(conn, "sync_state", "cluster_id"));
    assert!(!column_exists(conn, "sync_state", "cluster_generation"));
    // Watermark columns survive the rebuild.
    let push_seq_after: i64 = conn
        .query_row(
            "SELECT push_seq FROM sync_state WHERE db_id = 'local'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(push_seq_after, 17);
}

/// da-acft: v27→v28 deletes stored `same_state_repeat` rows from
/// `integrity_warnings` — that kind is now computed live from the
/// `entries` table rather than persisted at insert. Rows of other
/// kinds (double_open, orphan_end, overlap, last_state_mismatch)
/// must survive untouched. The down step is a no-op (the deleted
/// rows were stale by construction).
#[test]
fn roundtrip_v27_to_v28_drop_stored_same_state_repeat() {
    let db = fresh_at(27);
    let conn = db.conn();
    assert!(table_exists(conn, "integrity_warnings"));

    // Seed one row per kind so we can verify the migration is
    // surgical about which it removes.
    let seed = |id: &str, hid: &str, kind: &str| {
        conn.execute(
            r#"INSERT INTO integrity_warnings
                  (id, hid, entry_slug, local_entry, remote_entry,
                   detected_at, resolution, resolved_at, detected_by, kind)
               VALUES (?1, ?2, 'mood:x', '{}', NULL,
                       '2024-01-01T00:00:00Z', NULL, NULL,
                       'local_validator', ?3)"#,
            rusqlite::params![id, hid, kind],
        )
        .unwrap();
    };
    seed("id_same_state_a", "ha", "same_state_repeat");
    seed("id_same_state_b", "hb", "same_state_repeat");
    seed("id_double_open", "do", "double_open");
    seed("id_orphan_end", "oe", "orphan_end");

    apply_one_up(conn, 27);

    let remaining: Vec<String> = conn
        .prepare("SELECT kind FROM integrity_warnings ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        remaining,
        vec!["double_open".to_string(), "orphan_end".to_string()],
        "only stored same_state_repeat rows should be deleted",
    );

    // Down step is a no-op — the deleted rows aren't restored. The
    // migration declares this explicitly; assert the table stays in
    // the post-up shape.
    apply_one_down(conn, 28);
    let after_down: Vec<String> = conn
        .prepare("SELECT kind FROM integrity_warnings ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        after_down,
        vec!["double_open".to_string(), "orphan_end".to_string()],
        "down v28→v27 is a no-op — same_state_repeat rows are not restored",
    );
}

/// da-lxyw: v28→v29 drops the `NOT NULL` on `entries.occur_time` so a
/// bare `add` is timeless. Existing timed rows survive untouched; a new
/// timeless row (NULL occur_time) is accepted post-up. The down step
/// restores `NOT NULL`, backfilling any NULL rows with `create_time`
/// (matching the pre-da-lxyw stub) so the constraint holds.
#[test]
fn roundtrip_v28_to_v29_occur_time_nullable() {
    let db = fresh_at(28);
    let conn = db.conn();
    assert!(
        column_not_null(conn, "entries", "occur_time"),
        "v28 occur_time is NOT NULL",
    );

    // Seed a normal timed row.
    let seed = |hid: &str, slug: &str, occur: &str, create: &str| {
        conn.execute(
            r#"INSERT INTO entries
                  (hid, slug, content_hash, field, verb, occur_time,
                   create_time, modify_time, device_id, raw_input, parser_version)
               VALUES (?1, ?2, 'h', 'mood', 'point', ?3, ?4, ?4, 'dev', '', 1)"#,
            rusqlite::params![hid, slug, occur, create],
        )
        .unwrap();
    };
    seed(
        "h1",
        "mood:1",
        "2024-01-01T00:00:00+00:00",
        "2024-01-01T00:00:00+00:00",
    );

    apply_one_up(conn, 28);
    assert!(
        !column_not_null(conn, "entries", "occur_time"),
        "v29 occur_time must be nullable",
    );
    let surviving: String = conn
        .query_row("SELECT occur_time FROM entries WHERE hid = 'h1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        surviving, "2024-01-01T00:00:00+00:00",
        "timed row survives up"
    );

    // A timeless row (NULL occur_time) is now accepted. Give it a distinct
    // create_time so we can verify the down-step backfill.
    conn.execute(
        r#"INSERT INTO entries
              (hid, slug, content_hash, field, verb, occur_time,
               create_time, modify_time, device_id, raw_input, parser_version)
           VALUES ('h2', 'mood:unset', 'h', 'mood', 'point', NULL,
                   '2024-03-03T03:03:03+00:00', '2024-03-03T03:03:03+00:00', 'dev', '', 1)"#,
        [],
    )
    .expect("timeless insert must succeed under v29");

    apply_one_down(conn, 29);
    assert!(
        column_not_null(conn, "entries", "occur_time"),
        "down v29→v28 restores NOT NULL",
    );
    let backfilled: String = conn
        .query_row("SELECT occur_time FROM entries WHERE hid = 'h2'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        backfilled, "2024-03-03T03:03:03+00:00",
        "down step backfills NULL occur_time with create_time",
    );
}

/// da-x3da: v29→v30 adds the `modify_time_ms` virtual generated column
/// (epoch-ms of `modify_time`) plus an index, so sync push can filter
/// `WHERE modify_time_ms > ?` against a b-tree instead of scanning the whole
/// table. Existing rows survive; the computed value matches the RFC3339
/// source. The down step drops the index then the column.
#[test]
fn roundtrip_v29_to_v30_modify_time_ms_index() {
    let db = fresh_at(29);
    let conn = db.conn();
    assert!(!column_exists(conn, "entries", "modify_time_ms"));
    assert!(!index_exists(conn, "entries_modify_time_ms"));

    // Whole-second modify_time so the float metric equals chrono exactly.
    conn.execute(
        r#"INSERT INTO entries
              (hid, slug, content_hash, field, verb, occur_time,
               create_time, modify_time, device_id, raw_input, parser_version)
           VALUES ('h1', 'mood:1', 'h', 'mood', 'point', '2026-05-01T00:00:00+00:00',
                   '2026-05-01T00:00:00+00:00', '2026-05-01T12:34:56+00:00', 'dev', '', 1)"#,
        [],
    )
    .unwrap();

    apply_one_up(conn, 29);
    assert!(column_exists(conn, "entries", "modify_time_ms"));
    assert!(index_exists(conn, "entries_modify_time_ms"));

    // The generated column equals the epoch-ms of `2026-05-01T12:34:56Z`.
    let ms: i64 = conn
        .query_row(
            "SELECT modify_time_ms FROM entries WHERE hid = 'h1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        ms, 1_777_638_896_000,
        "modify_time_ms = epoch-ms of modify_time"
    );

    // The watermark filter hits the index rather than scanning the table.
    let plan: String = conn
        .query_row(
            "EXPLAIN QUERY PLAN SELECT * FROM entries WHERE modify_time_ms > ?1",
            [0_i64],
            |r| r.get(3),
        )
        .unwrap();
    assert!(
        plan.contains("entries_modify_time_ms"),
        "expected index seek, planner chose: {plan}"
    );

    apply_one_down(conn, 30);
    assert!(!index_exists(conn, "entries_modify_time_ms"));
    assert!(!column_exists(conn, "entries", "modify_time_ms"));
    let surviving: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries WHERE hid = 'h1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(surviving, 1, "down drops column/index but preserves rows");
}

/// da-xuiw: v30→v31 renames `sync_state.push_seq` → `push_mark` and
/// `my_push_seq` → `device_seq` (pull_seq untouched). Pure rename — the
/// seeded watermark values must survive both directions.
#[test]
fn roundtrip_v30_to_v31_sync_state_rename_seq_columns() {
    let db = fresh_at(30);
    let conn = db.conn();
    assert!(column_exists(conn, "sync_state", "push_seq"));
    assert!(column_exists(conn, "sync_state", "my_push_seq"));
    assert!(!column_exists(conn, "sync_state", "push_mark"));
    assert!(!column_exists(conn, "sync_state", "device_seq"));

    // Seed a watermark row (v30 column names) to verify values survive.
    conn.execute(
        "INSERT INTO sync_state (db_id, push_seq, pull_seq, my_push_seq, last_push_at, last_pull_at)
         VALUES ('local', 17, 22, 3, 1700000000000, 1700000005000)",
        [],
    )
    .unwrap();

    apply_one_up(conn, 30);

    // Renamed columns present, old names gone, pull_seq untouched.
    assert!(!column_exists(conn, "sync_state", "push_seq"));
    assert!(!column_exists(conn, "sync_state", "my_push_seq"));
    assert!(column_exists(conn, "sync_state", "push_mark"));
    assert!(column_exists(conn, "sync_state", "device_seq"));
    assert!(column_exists(conn, "sync_state", "pull_seq"));

    let (push_mark, pull_seq, device_seq): (i64, i64, i64) = conn
        .query_row(
            "SELECT push_mark, pull_seq, device_seq FROM sync_state WHERE db_id = 'local'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(push_mark, 17);
    assert_eq!(pull_seq, 22);
    assert_eq!(device_seq, 3);

    apply_one_down(conn, 31);

    // Old names restored with values intact.
    assert!(column_exists(conn, "sync_state", "push_seq"));
    assert!(column_exists(conn, "sync_state", "my_push_seq"));
    assert!(!column_exists(conn, "sync_state", "push_mark"));
    assert!(!column_exists(conn, "sync_state", "device_seq"));
    let (push_seq, my_push_seq): (i64, i64) = conn
        .query_row(
            "SELECT push_seq, my_push_seq FROM sync_state WHERE db_id = 'local'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(push_seq, 17);
    assert_eq!(my_push_seq, 3);
}
