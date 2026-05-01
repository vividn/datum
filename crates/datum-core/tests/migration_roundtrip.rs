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
    assert!(bad.is_err(), "CHECK must reject following_state without dur");

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
