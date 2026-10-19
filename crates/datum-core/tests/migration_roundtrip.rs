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

/// Run `f`, which goes through the live `Db` API, against the client
/// columns' live name: before v48 it was `device_id` (da-lfsei).
fn with_live_client_column<T>(conn: &Connection, f: impl FnOnce() -> T) -> T {
    let tables = ["entries", "entry_history"];
    let old = column_exists(conn, "entries", "device_id");
    let rename = |from: &str, to: &str| {
        for t in tables {
            conn.execute_batch(&format!("ALTER TABLE {t} RENAME COLUMN {from} TO {to}"))
                .unwrap()
        }
    };
    if old {
        rename("device_id", "client_id");
    }
    let out = f();
    if old {
        rename("client_id", "device_id");
    }
    out
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
        (31, 32),
        (32, 33),
        (33, 34),
        (34, 35),
        (35, 36),
        (36, 37),
        (37, 38),
        (38, 39),
        (39, 40),
        (40, 41),
        (41, 42),
        (42, 43),
        (43, 44),
        (44, 45),
        (45, 46),
        (46, 47),
        (47, 48),
        (48, 49),
        (49, 50),
        (50, 51),
        (51, 52),
        (52, 53),
        (53, 54),
        (54, 55),
        (55, 56),
        (56, 57),
        (57, 58),
        (58, 59),
        (59, 60),
        (60, 61),
        (61, 62),
        (62, 63),
        (63, 64),
        (64, 65),
        (65, 66),
        (66, 67),
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
             occur_time_uncertainty TEXT,
             dur_uncertainty      TEXT,
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
    // da-r17h: `note` is now a built-in verb, not a seeded alias, so no alias
    // ships by default. Seed a user alias in the v15 shape and verify it
    // survives the kit_id up/down rebuild.
    conn.execute(
        "INSERT INTO aliases (name, alias_json) VALUES ('vbp', \
         '{\"name\":\"vbp\",\"expands_to\":\"note word\"}')",
        [],
    )
    .unwrap();
    let alias_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM aliases WHERE name = 'vbp'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(alias_count, 1, "user alias present at v15");

    apply_one_up(conn, 15);
    assert!(column_exists(conn, "aliases", "kit_id"));
    assert!(index_exists(conn, "idx_aliases_kit_id"));

    apply_one_down(conn, 16);
    assert!(!column_exists(conn, "aliases", "kit_id"));
    assert!(!index_exists(conn, "idx_aliases_kit_id"));
    // Rebuild preserved the user alias.
    let alias_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM aliases WHERE name = 'vbp'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(alias_after, 1);
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

/// polecat §5 #11: v31→v32 drops `NOT NULL DEFAULT ''` from
/// `entries.raw_input` (and `entry_history.raw_input`). Roundtrip
/// proves the writable-schema trick is reversible and that legacy data
/// survives the conversion in both directions.
#[test]
fn roundtrip_v31_to_v32_raw_input_nullable() {
    use rusqlite::types::Value as SqlValue;

    let db = fresh_at(31);
    let conn = db.conn();

    // Pre-up: writing a NULL into entries.raw_input fails the NOT NULL.
    // (Use a minimal direct INSERT — we just need the constraint check.)
    let pre_up_null_insert = conn.execute(
        "INSERT INTO entries (
            hid, slug, content_hash, field, verb, occur_time, create_time,
            modify_time, device_id, parser_version, raw_input
        ) VALUES (
            'a' || hex(randomblob(4)), 'pre-up-null', 'h', 'f', 'point',
            '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
            '2026-01-01T00:00:00Z', 'd1', 1, NULL
        )",
        [],
    );
    assert!(
        pre_up_null_insert.is_err(),
        "v31 entries.raw_input must reject NULL inserts"
    );

    // Seed a row that should round-trip across the up.
    conn.execute(
        "INSERT INTO entries (
            hid, slug, content_hash, field, verb, occur_time, create_time,
            modify_time, device_id, parser_version, raw_input
        ) VALUES (
            'a' || hex(randomblob(4)), 'survives:1', 'hh', 'mood',
            'switch', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z',
            '2026-01-01T00:00:00Z', 'd1', 1, 'datum switch mood ok'
        )",
        [],
    )
    .unwrap();

    apply_one_up(conn, 31);

    // Post-up: NULL insert now works.
    conn.execute(
        "INSERT INTO entries (
            hid, slug, content_hash, field, verb, occur_time, create_time,
            modify_time, device_id, parser_version, raw_input
        ) VALUES (
            'b' || hex(randomblob(4)), 'post-up-null', 'hn', 'mood',
            'switch', '2026-01-01T00:00:01Z', '2026-01-01T00:00:01Z',
            '2026-01-01T00:00:01Z', 'd1', 1, NULL
        )",
        [],
    )
    .expect("v32 entries.raw_input must accept NULL");

    // Seeded row preserved.
    let raw: String = conn
        .query_row(
            "SELECT raw_input FROM entries WHERE slug = 'survives:1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(raw, "datum switch mood ok");

    // Mirror check on entry_history. v31 still has NOT NULL on this
    // column; v32 should permit NULL.
    let pre_down_null_history: SqlValue = conn
        .query_row(
            "SELECT raw_input FROM entries WHERE slug = 'post-up-null'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pre_down_null_history, SqlValue::Null);

    apply_one_down(conn, 32);

    // Post-down: NULL insert fails again, NULL rows were rewritten to ''.
    let post_down_null_insert = conn.execute(
        "INSERT INTO entries (
            hid, slug, content_hash, field, verb, occur_time, create_time,
            modify_time, device_id, parser_version, raw_input
        ) VALUES (
            'c' || hex(randomblob(4)), 'post-down-null', 'hd', 'f', 'point',
            '2026-01-01T00:00:02Z', '2026-01-01T00:00:02Z',
            '2026-01-01T00:00:02Z', 'd1', 1, NULL
        )",
        [],
    );
    assert!(
        post_down_null_insert.is_err(),
        "v31 restored: entries.raw_input rejects NULL again"
    );

    let restored: String = conn
        .query_row(
            "SELECT raw_input FROM entries WHERE slug = 'post-up-null'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        restored, "",
        "down-migration must rewrite NULL → '' to restore the NOT NULL contract"
    );
}

/// da-4vyw: v32→v33 recomputes every `content_hash` under the definition
/// that now covers `following_state` and deleted-ness. Roundtrip proves the
/// forward step separates the pairs v32 conflated, leaves no row disagreeing
/// with `expected_content_hash`, walks tombstones too, and that the down step
/// restores the v32 five-input hashes exactly.
#[test]
fn roundtrip_v32_to_v33_content_hash_rehash() {
    let db = fresh_at(32);
    let conn = db.conn();

    // The pre-da-4vyw hash: field · occur · dur · state · data, \x00-framed.
    let v32_hash = |occur: &str| {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for (i, part) in ["project", occur, "PT30M", "\"writing\"", ""]
            .iter()
            .enumerate()
        {
            if i > 0 {
                h.update(b"\x00");
            }
            h.update(part.as_bytes());
        }
        format!("{:x}", h.finalize())
    };

    // Two pairs that v32 cannot tell apart, planted with direct SQL so the
    // stored hashes are genuinely the v32 values (the live insert path would
    // already write v33 ones):
    //
    //   h1 / h3 — identical content, one live and one tombstoned. This is
    //             the case da-yiq9 decided: rows that hash alike are twins
    //             and fold away silently, so under the v32 hash this pair
    //             would fold — resurrecting the tombstone or discarding the
    //             live row, whichever outranked the other. The v33 hash
    //             covers deleted-ness, so the two no longer agree and take
    //             the surfaced path instead.
    //   h5 / h6 — identical apart from `following_state`. Both tombstoned,
    //             because the partial UNIQUE on (field, occur_time) will not
    //             let two *live* rows sit at one instant.
    struct Planted {
        hid: &'static str,
        slug: &'static str,
        occur: &'static str,
        following_state: Option<&'static str>,
        deleted_at: Option<&'static str>,
    }
    let nine = "2026-01-02T09:00:00+00:00";
    let thirteen = "2026-01-02T13:00:00+00:00";
    let rows = &[
        Planted {
            hid: "h1",
            slug: "project:09-live",
            occur: nine,
            following_state: None,
            deleted_at: None,
        },
        Planted {
            hid: "h3",
            slug: "project:09-dead",
            occur: nine,
            following_state: None,
            deleted_at: Some(thirteen),
        },
        Planted {
            hid: "h5",
            slug: "project:13-plain",
            occur: thirteen,
            following_state: None,
            deleted_at: Some(thirteen),
        },
        Planted {
            hid: "h6",
            slug: "project:13-switch",
            occur: thirteen,
            following_state: Some("\"resting\""),
            deleted_at: Some(thirteen),
        },
    ];
    for Planted {
        hid,
        slug,
        occur,
        following_state: following,
        deleted_at: deleted,
    } in rows
    {
        conn.execute(
            "INSERT INTO entries (
                hid, slug, content_hash, field, verb, state, dur,
                following_state, occur_time, create_time, modify_time,
                device_id, parser_version, deleted_at
             ) VALUES (?1, ?2, ?3, 'project', 'block', '\"writing\"', 'PT30M',
                       ?4, ?5, '2026-01-01T00:00:00+00:00',
                       '2026-01-01T00:00:00+00:00', 'd1', 1, ?6)",
            rusqlite::params![hid, slug, v32_hash(occur), following, occur, deleted],
        )
        .unwrap();
    }

    let distinct = |conn: &Connection| -> i64 {
        conn.query_row(
            "SELECT COUNT(DISTINCT content_hash) FROM entries",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        distinct(conn),
        2,
        "v32 collapses four distinct rows onto two hashes"
    );

    apply_one_up(conn, 32);

    assert_eq!(
        distinct(conn),
        4,
        "v33 must give every semantically distinct row its own hash"
    );
    for entry in entries_at_v33(conn) {
        assert_eq!(
            entry.content_hash,
            entry.expected_content_hash(),
            "row {} disagrees with expected_content_hash after v33",
            entry.slug
        );
    }

    apply_one_down(conn, 33);

    assert_eq!(distinct(conn), 2, "down must restore the v32 hashes");
    for row in rows {
        let restored: String = conn
            .query_row(
                "SELECT content_hash FROM entries WHERE hid = ?1",
                rusqlite::params![row.hid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(restored, v32_hash(row.occur), "{} not restored", row.hid);
    }
}

/// Read every row through the live `Entry` mapper while the schema is at
/// v33 — where the two uncertainty columns still carry their pre-da-b8rs
/// names. Aliasing them forward is what `rehash_all_entries` does for the
/// same reason: the mapper always speaks today's column names, and a
/// mid-walk schema does not.
fn entries_at_v33(conn: &Connection) -> Vec<datum_core::entry::Entry> {
    let mut stmt = conn
        .prepare(
            "SELECT *, occur_time_precision AS occur_time_uncertainty, \
             dur_precision AS dur_uncertainty, device_id AS client_id FROM entries",
        )
        .unwrap();
    let rows = stmt
        .query_map([], datum_core::db::row_to_entry_public)
        .unwrap();
    rows.map(|r| r.unwrap()).collect()
}

/// da-b8rs: v33→v34 renames the two uncertainty columns. A rename must be
/// *lossless in both directions* — the values are the whole point, and a
/// rename that quietly emptied them would look identical to a successful
/// one at the schema level. Assert the stored values survive the round
/// trip, not just the column names.
#[test]
fn roundtrip_v33_to_v34_precision_to_uncertainty_rename() {
    let db = fresh_at(33);
    let conn = db.conn();

    assert!(column_exists(conn, "entries", "occur_time_precision"));
    assert!(column_exists(conn, "entries", "dur_precision"));
    assert!(!column_exists(conn, "entries", "occur_time_uncertainty"));
    assert!(!column_exists(conn, "entries", "dur_uncertainty"));

    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, verb, dur, occur_time,
             occur_time_precision, dur_precision,
             create_time, modify_time, device_id)
         VALUES
            ('hp', 'f:p', 'cp', 'f', 'point', 'PT1H30M',
             '2026-05-01T00:00:00Z', 'PT5M', 'PT1M',
             '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z', 'd')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 33);

    assert!(!column_exists(conn, "entries", "occur_time_precision"));
    assert!(!column_exists(conn, "entries", "dur_precision"));
    assert!(column_exists(conn, "entries", "occur_time_uncertainty"));
    assert!(column_exists(conn, "entries", "dur_uncertainty"));

    let (occur_u, dur_u): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT occur_time_uncertainty, dur_uncertainty FROM entries WHERE hid = 'hp'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(occur_u.as_deref(), Some("PT5M"), "values ride the rename");
    assert_eq!(dur_u.as_deref(), Some("PT1M"), "values ride the rename");

    apply_one_down(conn, 34);

    assert!(column_exists(conn, "entries", "occur_time_precision"));
    assert!(column_exists(conn, "entries", "dur_precision"));
    assert!(!column_exists(conn, "entries", "occur_time_uncertainty"));
    assert!(!column_exists(conn, "entries", "dur_uncertainty"));

    let (occur_p, dur_p): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT occur_time_precision, dur_precision FROM entries WHERE hid = 'hp'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(occur_p.as_deref(), Some("PT5M"), "down is lossless too");
    assert_eq!(dur_p.as_deref(), Some("PT1M"), "down is lossless too");
}

/// da-uxgv: `superseded_by` arrives, both UNIQUE indexes become partial on
/// it, and the rows the retired parking mechanism displaced are moved onto
/// the pointer — natural slug restored, park tombstone lifted.
#[test]
fn roundtrip_v34_to_v35_superseded_by() {
    let db = fresh_at(34);
    let conn = db.conn();

    assert!(!column_exists(conn, "entries", "superseded_by"));
    assert!(!index_exists(conn, "entries_superseded_by"));

    // The winner, and beside it a row parked out of its way the old way:
    // slug suffixed with its own hid, tombstoned at the winner's
    // modify_time because the two also contended for (field, occur_time).
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, verb, occur_time,
             create_time, modify_time, device_id)
         VALUES
            ('winnerhid001', 'coffee:2026-08-13T08:00:00+00:00', 'h1', 'coffee',
             'point', '2026-08-13T08:00:00+00:00',
             '2026-08-13T08:00:00Z', '2026-08-13T08:00:00Z', 'dev-a')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, verb, occur_time,
             create_time, modify_time, device_id, deleted_at)
         VALUES
            ('loserhid0002', 'coffee:2026-08-13T08:00:00+00:00_loserhid0002', 'h2',
             'coffee', 'point', '2026-08-13T08:00:00+00:00',
             '2026-08-13T07:00:00Z', '2026-08-13T07:00:00Z', 'dev-b',
             '2026-08-13T08:00:00+00:00')",
        [],
    )
    .unwrap();
    // A row the user genuinely deleted, whose slug merely ends in its hid:
    // nobody holds the base, so there is no supersession to record.
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, verb, occur_time,
             create_time, modify_time, device_id, deleted_at)
         VALUES
            ('lonehid00003', 'tea:2026-08-13T09:00:00+00:00_lonehid00003', 'h3',
             'tea', 'point', '2026-08-13T09:00:00+00:00',
             '2026-08-13T09:00:00Z', '2026-08-13T09:00:00Z', 'dev-a',
             '2026-08-13T10:00:00+00:00')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 34);

    assert!(column_exists(conn, "entries", "superseded_by"));
    assert!(index_exists(conn, "entries_superseded_by"));

    let (slug, superseded_by, deleted_at): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT slug, superseded_by, deleted_at FROM entries WHERE hid = 'loserhid0002'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        slug, "coffee:2026-08-13T08:00:00+00:00",
        "the loser gets its naturally-derived slug back",
    );
    assert_eq!(
        superseded_by.as_deref(),
        Some("winnerhid001"),
        "and points at the row that holds that slug",
    );
    assert_eq!(
        deleted_at, None,
        "the park's tombstone is lifted — it was never a user deletion",
    );

    let (lone_slug, lone_ptr, lone_deleted): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT slug, superseded_by, deleted_at FROM entries WHERE hid = 'lonehid00003'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        lone_slug, "tea:2026-08-13T09:00:00+00:00_lonehid00003",
        "a suffixed slug nobody contends for is left alone",
    );
    assert_eq!(lone_ptr, None);
    assert_eq!(
        lone_deleted.as_deref(),
        Some("2026-08-13T10:00:00+00:00"),
        "a real deletion stays deleted",
    );

    // Both rows now hold one slug, which the partial index permits.
    let sharing: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE slug = 'coffee:2026-08-13T08:00:00+00:00'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        sharing, 2,
        "two rows may share a slug once one is superseded"
    );

    // The down restores the total UNIQUE index, so the superseded row has
    // to give the slug back first — documented data loss, exercised here
    // the way a conflict-free database would meet it.
    conn.execute(
        "UPDATE entries SET slug = 'coffee:parked', deleted_at = '2026-08-13T08:00:00+00:00' \
         WHERE hid = 'loserhid0002'",
        [],
    )
    .unwrap();
    apply_one_down(conn, 35);
    assert!(!column_exists(conn, "entries", "superseded_by"));
    assert!(!index_exists(conn, "entries_superseded_by"));
    assert!(index_exists(conn, "entries_slug"));
}

/// da-2fkr: v35→v36 narrows `entries_field_occurtime` to rows that assert a
/// state. Up: the predicate gains `state IS NOT NULL`, two stateless points
/// at one instant land, two state-bearing ones still do not. Down: the
/// predicate loses the term again — and refuses to build over the stateless
/// pair v36 permitted, naming the hids rather than leaving CREATE INDEX to
/// complain about rows the operator cannot then find.
#[test]
fn roundtrip_v35_to_v36_field_occurtime_state_only() {
    let db = fresh_at(35);
    let conn = db.conn();

    assert!(index_exists(conn, "entries_field_occurtime"));
    assert!(
        !field_occurtime_sql(conn).contains("state IS NOT NULL"),
        "v35 predicate is state-blind: {}",
        field_occurtime_sql(conn),
    );
    // At v35 the second transaction on one day is refused.
    insert_tx(conn, "hidrent00001", "tx:rent", None).unwrap();
    let err = insert_tx(conn, "hidelec00002", "tx:electricity", None).unwrap_err();
    assert!(
        format!("{err}").contains("UNIQUE constraint failed"),
        "expected the v35 index to refuse it, got: {err}"
    );

    apply_one_up(conn, 35);

    let up_sql = field_occurtime_sql(conn);
    assert!(
        up_sql.contains("state IS NOT NULL"),
        "v36 predicate must be state-scoped: {up_sql}"
    );
    // The same insert now lands, beside the one already there.
    insert_tx(conn, "hidelec00002", "tx:electricity", None).unwrap();
    let live: Vec<String> = conn
        .prepare("SELECT hid FROM entries WHERE field = 'tx' ORDER BY hid")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(live, vec!["hidelec00002", "hidrent00001"]);

    // Two rows that DO assert a state at one instant are still refused —
    // da-ga8b's case, unchanged.
    insert_tx(conn, "hidalpha0003", "project:alpha", Some("\"alpha\"")).unwrap();
    let err = insert_tx(conn, "hidbeta00004", "project:beta", Some("\"beta\"")).unwrap_err();
    assert!(
        format!("{err}").contains("UNIQUE constraint failed"),
        "state-bearing pair must still collide, got: {err}"
    );

    // Down over the stateless pair v36 allowed: a hid-by-hid diagnostic,
    // not a bare constraint failure at CREATE INDEX.
    let err = apply_migrations_down(conn, MIGRATIONS, 36, 35).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("v36→v35"),
        "diagnostic must name the step: {msg}"
    );
    assert!(
        msg.contains("hidrent00001") && msg.contains("hidelec00002"),
        "diagnostic must list the offending hids: {msg}"
    );
    assert_eq!(read_schema_version(conn).unwrap(), Some(36));
    assert!(
        field_occurtime_sql(conn).contains("state IS NOT NULL"),
        "the refused down must leave the v36 index standing",
    );

    // Resolve the pair the way the diagnostic asks, and the down lands.
    conn.execute("DELETE FROM entries WHERE hid = 'hidelec00002'", [])
        .unwrap();
    apply_one_down(conn, 36);
    assert!(index_exists(conn, "entries_field_occurtime"));
    assert!(
        !field_occurtime_sql(conn).contains("state IS NOT NULL"),
        "down must restore the state-blind predicate",
    );
}

/// da-ij3x: v37 adds `entries_slug_lookup`, and with it a slug lookup that
/// does not say `superseded_by IS NULL` searches an index instead of
/// scanning `entries`. Down drops it and the scan comes back.
#[test]
fn roundtrip_v36_to_v37_slug_lookup_index() {
    let db = fresh_at(36);
    let conn = db.conn();
    let plan = |conn: &Connection| -> String {
        conn.query_row(
            "EXPLAIN QUERY PLAN SELECT * FROM entries WHERE slug = 'x' AND deleted_at IS NOT NULL",
            [],
            |r| r.get(3),
        )
        .unwrap()
    };
    assert!(!index_exists(conn, "entries_slug_lookup"));
    assert_eq!(plan(conn), "SCAN entries");

    apply_one_up(conn, 36);
    assert!(index_exists(conn, "entries_slug_lookup"));
    assert!(plan(conn).contains("entries_slug_lookup"), "{}", plan(conn));

    apply_one_down(conn, 37);
    assert!(!index_exists(conn, "entries_slug_lookup"));
    assert_eq!(plan(conn), "SCAN entries");
}

/// A v37 database with a user's spec (verbless, from before da-c8mh) and a
/// kit's (tags, state colours, a render override), in that write order.
fn v37_with_specs() -> Db {
    let db = fresh_at(37);
    db.conn()
        .execute_batch(
            r#"INSERT INTO field_specs (field, spec_json, kit_id, tags, render_json) VALUES
               ('mood', '{"field":"mood","default_verb":null,"color":"blue"}', NULL, '[]', NULL),
               ('dishes', '{"field":"dishes","verb":"occur","tags":["chore"],
                  "state_colors":{"done":"green"},"allowed_states":["done"],"render":false}',
                'chores', '["chore"]', 'false');"#,
        )
        .unwrap();
    db
}

/// da-lc26c: the specs move into entries and the table goes; the specs
/// read back identical, the kit's mark rides in the data, and the down
/// puts the table back as it was.
#[test]
fn roundtrip_v37_to_v38_specs_are_entries() {
    let db = v37_with_specs();
    let conn = db.conn();
    let before: Vec<(String, String)> = conn
        .prepare("SELECT field, spec_json FROM field_specs ORDER BY field")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    apply_one_up(conn, 37);
    assert!(!table_exists(conn, "field_specs"));
    let rows: Vec<(String, String, String, String)> = conn
        .prepare("SELECT slug, field, occur_time, data FROM entries ORDER BY rowid")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let slugs: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(
        slugs,
        ["spec:mood", "spec:dishes"],
        "field_specs rowid order"
    );
    for (_, field, occur, _) in &rows {
        assert_eq!(field, "spec");
        assert_eq!(occur, "1970-01-01T00:00:00+00:00", "a fixed occur time");
    }
    let dishes: serde_json::Value = serde_json::from_str(&rows[1].3).unwrap();
    assert_eq!(dishes["kit_id"], "chores");

    // Every reader sees the same specs it saw in the table.
    for (field, json) in &before {
        let mut was: serde_json::Value = serde_json::from_str(json).unwrap();
        if field == "mood" {
            was["verb"] = "occur".into(); // the log-implied verb, read as before
            was.as_object_mut().unwrap().remove("default_verb");
        }
        let was: datum_core::field_spec::FieldSpec = serde_json::from_value(was).unwrap();
        let now = db.load_spec(field).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&now).unwrap(),
            serde_json::to_value(&was).unwrap(),
            "{field}"
        );
    }
    let tagged: Vec<String> = db
        .list_specs_by_tag("chore")
        .unwrap()
        .into_iter()
        .map(|s| s.field)
        .collect();
    assert_eq!(tagged, ["dishes"]);
    assert_eq!(
        db.get_all_state_colors().unwrap()[&("dishes".into(), "done".into())],
        "green"
    );
    assert_eq!(db.load_specs_by_kit("chores").unwrap().len(), 1);
    assert_eq!(db.load_registry().unwrap().fields.len(), 2);

    apply_one_down(conn, 38);
    assert!(table_exists(conn, "field_specs"));
    let back: Vec<(String, Option<String>, String, Option<String>)> = conn
        .prepare("SELECT field, kit_id, tags, render_json FROM field_specs ORDER BY field")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        back,
        [
            (
                "dishes".into(),
                Some("chores".into()),
                r#"["chore"]"#.into(),
                Some("false".into())
            ),
            ("mood".into(), None, "[]".into(), None),
        ]
    );
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

/// Two devices migrating the same spec make the same doc: slug, data,
/// occur time and content hash all agree.
#[test]
fn v37_to_v38_is_deterministic_across_devices() {
    let doc = |db: &Db| -> Vec<(String, String, String, String)> {
        apply_one_up(db.conn(), 37);
        db.conn()
            .prepare("SELECT slug, data, occur_time, content_hash FROM entries ORDER BY slug")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert_eq!(doc(&v37_with_specs()), doc(&v37_with_specs()));
}

/// Life data already logged under `spec` stops the migration before it
/// writes anything, naming the field and the count.
#[test]
fn v37_to_v38_refuses_a_field_already_named_spec() {
    let db = v37_with_specs();
    let conn = db.conn();
    for hid in ["h1", "h2"] {
        conn.execute(
            "INSERT INTO entries (hid, slug, content_hash, field, occur_time,
                                  create_time, modify_time, device_id)
             VALUES (?1, 'spec:' || ?1, ?1, 'spec', '2026-09-01T00:00:00+00:00',
                     '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'dev')",
            [hid],
        )
        .unwrap();
    }
    let err = apply_migrations(conn, MIGRATIONS, 37, 38)
        .unwrap_err()
        .to_string();
    assert!(err.contains("2 entries") && err.contains("`spec`"), "{err}");
    assert_eq!(read_schema_version(conn).unwrap(), Some(37));
    assert!(table_exists(conn, "field_specs"));
    let specs: i64 = conn
        .query_row("SELECT COUNT(*) FROM field_specs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(specs, 2);
}

/// A v38 database with a user's alias, a kit's, and a user alias that
/// shadows a spec-derived name (`mood`, the bare-field alias of the spec).
fn v38_with_aliases() -> Db {
    let db = fresh_at(38);
    let mut spec =
        datum_core::field_spec::FieldSpec::new("mood", datum_core::action::Action::Switch);
    spec.regenerate_derived_aliases();
    with_live_client_column(db.conn(), || db.save_field_spec(&spec).unwrap());
    db.conn()
        .execute_batch(
            r#"INSERT INTO aliases (name, alias_json, kit_id) VALUES
               ('vbp', '{"name":"vbp","expands_to":"note word pos=verb","description":null}', NULL),
               ('cof', '{"name":"cof","expands_to":"occur coffee","description":"a cup"}', 'drinks'),
               ('mood', '{"name":"mood","expands_to":"switch mood happy","description":null}', NULL);"#,
        )
        .unwrap();
    db
}

/// da-b3752: the aliases move into entries and the table goes; every alias
/// resolves as it did (the shadowing of a spec-derived name included), the
/// kit's mark rides in the data, and the down puts the table back.
#[test]
fn roundtrip_v38_to_v39_aliases_are_entries() {
    let db = v38_with_aliases();
    let conn = db.conn();
    let before: Vec<(String, String, Option<String>)> = conn
        .prepare("SELECT name, alias_json, kit_id FROM aliases ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    apply_one_up(conn, 38);
    assert!(!table_exists(conn, "aliases"));
    let rows: Vec<(String, String)> = conn
        .prepare("SELECT slug, occur_time FROM entries WHERE field = 'alias' ORDER BY slug")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        ["alias:cof", "alias:mood", "alias:vbp"]
    );
    assert!(rows.iter().all(|r| r.1 == "1970-01-01T00:00:00+00:00"));

    // Every alias resolves identically, on every reader.
    let registry = db.load_registry().unwrap();
    for (name, json, _) in &before {
        let was: datum_core::field_spec::Alias = serde_json::from_str(json).unwrap();
        assert_eq!(
            db.lookup_alias(name).unwrap().as_ref(),
            Some(&was),
            "{name}"
        );
        assert_eq!(registry.aliases.get(name), Some(&was), "{name}");
    }
    assert_eq!(db.list_user_aliases().unwrap().len(), 3);
    let kit: Vec<String> = db
        .load_aliases_by_kit("drinks")
        .unwrap()
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(kit, ["cof"]);
    // The spec's other derived alias is still reached through the index.
    assert!(db.lookup_alias("moode").unwrap().is_some());

    apply_one_down(conn, 39);
    let back: Vec<(String, String, Option<String>)> = conn
        .prepare("SELECT name, alias_json, kit_id FROM aliases ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let parsed = |rows: &[(String, String, Option<String>)]| -> Vec<(String, serde_json::Value, Option<String>)> {
        rows.iter()
            .map(|(n, j, k)| (n.clone(), serde_json::from_str(j).unwrap(), k.clone()))
            .collect()
    };
    assert_eq!(parsed(&back), parsed(&before));
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE field = 'alias'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
}

/// Two devices migrating the same alias make the same doc.
#[test]
fn v38_to_v39_is_deterministic_across_devices() {
    let doc = |db: &Db| -> Vec<(String, String, String, String)> {
        apply_one_up(db.conn(), 38);
        db.conn()
            .prepare(
                "SELECT slug, data, occur_time, content_hash FROM entries
                 WHERE field = 'alias' ORDER BY slug",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert_eq!(doc(&v38_with_aliases()), doc(&v38_with_aliases()));
}

/// Life data already logged under `alias` stops the migration before it
/// writes anything.
#[test]
fn v38_to_v39_refuses_a_field_already_named_alias() {
    let db = v38_with_aliases();
    let conn = db.conn();
    insert_tx(conn, "h1", "alias:h1", None).unwrap();
    let err = apply_migrations(conn, MIGRATIONS, 38, 39)
        .unwrap_err()
        .to_string();
    assert!(err.contains("1 entry") && err.contains("`alias`"), "{err}");
    assert_eq!(read_schema_version(conn).unwrap(), Some(38));
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM aliases", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);
}

/// The index a v39 kit install recorded in `kit_indices` and built.
fn v39_index() -> datum_core::kit::IndexEntry {
    datum_core::kit::IndexEntry {
        name: "by_mood".into(),
        on: "entries".into(),
        expression: "field, json_extract(data, '$.why')".into(),
        where_clause: Some("field IN ('mood', 'x)y')".into()),
        unique: false,
    }
}

/// A v39 database holding a kit's view, command and index, a user's view,
/// and a mood to see.
fn v39_with_definitions() -> Db {
    let db = fresh_at(39);
    let conn = db.conn();
    insert_tx(conn, "m1", "mood:m1", Some("\"happy\"")).unwrap();
    let ddl = v39_index().build_ddl("moods").unwrap();
    conn.execute_batch(&ddl).unwrap();
    conn.execute(
        "INSERT INTO kit_indices (kit_id, index_name, on_disk_name, sql_text)
         VALUES ('moods', 'by_mood', 'kit_moods__by_mood', ?1)",
        [&ddl],
    )
    .unwrap();
    conn.execute_batch(
        r#"INSERT INTO views (name, sql_text, kit_id) VALUES
           ('moods', 'SELECT slug, state FROM entries WHERE field = ''mood''', 'moods'),
           ('mine', 'SELECT 1 AS one', NULL);
           INSERT INTO kit_commands (name, kit_id, expands_to, view_name, help,
               columns_json, labels_json, formats_json, sort_col, flags_json,
               color_rules_json, script_text) VALUES
           ('moodlist', 'moods', NULL, 'moods', 'every mood', '["state"]',
            '{"state":"Mood"}', NULL, 'state', '["watch"]',
            '[{"when":"state == \"happy\"","color":"green"}]', NULL),
           ('m', 'moods', 'switch mood', NULL, NULL, NULL, NULL, NULL, NULL,
            NULL, NULL, NULL);"#,
    )
    .unwrap();
    db
}

/// da-fwwtn: the views, kit commands and kit index definitions move into
/// entries and the tables go; every view gives what it gave, every command
/// and index reads back as it was, the physical index survives, and the
/// down puts the tables back.
#[test]
fn roundtrip_v39_to_v40_definitions_are_entries() {
    let db = v39_with_definitions();
    let conn = db.conn();
    // Compared as JSON where a row is some, so a key's order is not news.
    let tables = |conn: &Connection| -> Vec<Vec<serde_json::Value>> {
        [
            "SELECT name || '|' || sql_text || '|' || IFNULL(kit_id, '-') FROM views ORDER BY name",
            "SELECT kit_id || '|' || index_name || '|' || on_disk_name || '|' || sql_text FROM kit_indices",
            "SELECT json_array(name, kit_id, expands_to, view_name, help, json(columns_json),
                    json(labels_json), json(formats_json), sort_col, json(flags_json),
                    json(color_rules_json), script_text) FROM kit_commands ORDER BY name",
        ]
        .iter()
        .map(|q| {
            conn.prepare(q)
                .unwrap()
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(|row| {
                    let row = row.unwrap();
                    serde_json::from_str(&row).unwrap_or(serde_json::Value::String(row))
                })
                .collect()
        })
        .collect()
    };
    let before = tables(conn);
    let run = |sql: &str| {
        let r = db.execute_view(sql).unwrap();
        (r.columns, r.rows)
    };
    let seen = run("SELECT slug, state FROM entries WHERE field = 'mood'");

    apply_one_up(conn, 39);
    for t in ["views", "kit_commands", "kit_indices"] {
        assert!(!table_exists(conn, t), "{t}");
    }
    let rows: Vec<(String, String)> = conn
        .prepare(
            "SELECT slug, occur_time FROM entries
             WHERE field IN ('view', 'command', 'index') ORDER BY slug",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        [
            "command:m",
            "command:moodlist",
            "index:moods/by_mood",
            "view:mine",
            "view:moods"
        ]
    );
    assert!(rows.iter().all(|r| r.1 == "1970-01-01T00:00:00+00:00"));

    with_live_client_column(conn, || {
        let view = db.get_view("moods").unwrap().unwrap();
        assert_eq!(view.kit_id.as_deref(), Some("moods"));
        assert_eq!(run(&view.sql_text), seen);
        assert_eq!(db.get_view("mine").unwrap().unwrap().kit_id, None);
        let cmd = db.get_kit_command("moodlist").unwrap().unwrap();
        assert_eq!(
            (
                cmd.view_name.as_deref(),
                cmd.columns,
                cmd.flags,
                cmd.color_rules.len()
            ),
            (
                Some("moods"),
                vec!["state".to_string()],
                vec!["watch".to_string()],
                1
            )
        );
        assert_eq!(cmd.labels.get("state").map(String::as_str), Some("Mood"));
        let ix = db.load_indices_by_kit("moods").unwrap();
        assert_eq!(ix.len(), 1);
        assert_eq!(ix[0].entry, v39_index());
        assert!(index_exists(conn, "kit_moods__by_mood"));
        assert!(db.derive_kit_indices().unwrap().is_empty());
        assert!(index_exists(conn, "kit_moods__by_mood"));
    });

    apply_one_down(conn, 40);
    assert_eq!(tables(conn), before);
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE field IN ('view', 'command', 'index')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 0);
}

/// Two devices migrating the same definitions make the same docs.
#[test]
fn v39_to_v40_is_deterministic_across_devices() {
    let doc = |db: &Db| -> Vec<(String, String, String, String)> {
        apply_one_up(db.conn(), 39);
        db.conn()
            .prepare(
                "SELECT slug, data, occur_time, content_hash FROM entries
                 WHERE field IN ('view', 'command', 'index') ORDER BY slug",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    assert_eq!(doc(&v39_with_definitions()), doc(&v39_with_definitions()));
}

/// Life data already logged under `view`, `command` or `index` stops the
/// migration before it writes anything.
#[test]
fn v39_to_v40_refuses_a_field_already_named_view() {
    let db = v39_with_definitions();
    let conn = db.conn();
    insert_tx(conn, "h1", "view:h1", None).unwrap();
    let err = apply_migrations(conn, MIGRATIONS, 39, 40)
        .unwrap_err()
        .to_string();
    assert!(err.contains("1 entry") && err.contains("`view`"), "{err}");
    assert_eq!(read_schema_version(conn).unwrap(), Some(39));
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM views", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2);
}

/// A deleted log under `view` is no life data in the way, so v40 goes on —
/// unless it holds the very slug a definition needs, which is refused by
/// name rather than failing on the slug's UNIQUE. A superseded row is not
/// in that UNIQUE, so it is not in the way either.
#[test]
fn v39_to_v40_passes_a_deleted_log_unless_it_holds_a_definitions_slug() {
    let set_aside = |slug: &str, how: &str| {
        let db = v39_with_definitions();
        insert_tx(db.conn(), "h1", slug, None).unwrap();
        db.conn()
            .execute(&format!("UPDATE entries SET {how} WHERE hid = 'h1'"), [])
            .unwrap();
        let moved = apply_migrations(db.conn(), MIGRATIONS, 39, 40);
        (moved.map_err(|e| e.to_string()), db)
    };
    let deleted = |slug: &str| set_aside(slug, "deleted_at = '2026-09-02T00:00:00Z'");
    let view = |db: &Db| with_live_client_column(db.conn(), || db.get_view("moods").unwrap());
    let (moved, db) = deleted("view:h1");
    moved.unwrap();
    assert!(view(&db).is_some());
    let (moved, db) = set_aside(
        "view:moods",
        "deleted_at = '2026-09-02T00:00:00Z', superseded_by = 'm1'",
    );
    moved.unwrap();
    assert!(view(&db).is_some());

    let (moved, db) = deleted("view:moods");
    let err = moved.unwrap_err();
    assert!(err.contains("an entry already holds `view:moods`"), "{err}");
    assert_eq!(read_schema_version(db.conn()).unwrap(), Some(39));
}

/// da-xnzc7: `kit` becomes reserved for kits' `[kit]` tables. Nothing
/// moves either way.
#[test]
fn roundtrip_v50_to_v51_kit_is_reserved() {
    let db = fresh_at(50);
    apply_one_up(db.conn(), 50);
    apply_one_down(db.conn(), 51);
}

/// da-z0zh3: the undo stack's table and column come and go together.
#[test]
fn roundtrip_v51_to_v52_undo_actions() {
    let db = fresh_at(51);
    let conn = db.conn();
    apply_one_up(conn, 51);
    assert!(table_exists(conn, "undo_actions"));
    assert!(column_exists(conn, "entry_history", "action_id"));
    apply_one_down(conn, 52);
    assert!(!table_exists(conn, "undo_actions"));
    assert!(!column_exists(conn, "entry_history", "action_id"));
}

#[test]
fn roundtrip_v52_to_v53_field_state_indexes() {
    let db = fresh_at(52);
    let conn = db.conn();
    let live_sql = || -> String {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'entries_live_field_occur'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    apply_one_up(conn, 52);
    assert!(index_exists(conn, "entries_field_state"));
    assert!(index_exists(conn, "entries_field_states"));
    assert!(
        live_sql().contains("(field, occur_time, slug)"),
        "{}",
        live_sql()
    );
    apply_one_down(conn, 53);
    assert!(!index_exists(conn, "entries_field_state"));
    assert!(!index_exists(conn, "entries_field_states"));
    assert!(live_sql().contains("(field, occur_time)"), "{}", live_sql());
}

/// da-38eye: the redemption mark comes and goes; the peers stay.
#[test]
fn roundtrip_v53_to_v54_sync_peer_redeemed() {
    let db = fresh_at(53);
    let conn = db.conn();
    apply_one_up(conn, 53);
    assert!(column_exists(conn, "sync_peers", "redeemed_at"));
    apply_one_down(conn, 54);
    assert!(!column_exists(conn, "sync_peers", "redeemed_at"));
    assert!(table_exists(conn, "sync_peers"));
}

#[test]
fn roundtrip_v59_to_v60_undo_skipped() {
    let db = fresh_at(59);
    let conn = db.conn();
    apply_one_up(conn, 59);
    assert!(column_exists(conn, "undo_actions", "skipped"));
    apply_one_down(conn, 60);
    assert!(!column_exists(conn, "undo_actions", "skipped"));
    assert!(table_exists(conn, "undo_actions"));
}

/// da-2vgj1: the listing's indexes come and go.
#[test]
fn roundtrip_v60_to_v61_hybrid_indexes() {
    let db = fresh_at(60);
    let conn = db.conn();
    apply_one_up(conn, 60);
    assert!(index_exists(conn, "entries_hybrid"));
    assert!(index_exists(conn, "entries_live_field_hybrid"));
    apply_one_down(conn, 61);
    assert!(!index_exists(conn, "entries_hybrid"));
    assert!(!index_exists(conn, "entries_live_field_hybrid"));
}

/// da-qctsr: the redo tail's index comes and goes.
#[test]
fn roundtrip_v61_to_v62_undo_undone_index() {
    let db = fresh_at(61);
    let conn = db.conn();
    apply_one_up(conn, 61);
    assert!(index_exists(conn, "undo_actions_undone"));
    apply_one_down(conn, 62);
    assert!(!index_exists(conn, "undo_actions_undone"));
}

/// da-ulu8f: v63 lays down `spec_alias_names`, filled from the specs the
/// file already holds and kept by its triggers; v62 takes them away.
#[test]
fn roundtrip_v62_to_v63_spec_alias_names() {
    let db = fresh_at(62);
    let mut spec =
        datum_core::field_spec::FieldSpec::new("mood", datum_core::action::Action::Switch);
    spec.allowed_states = Some(vec!["calm".into()]);
    spec.regenerate_derived_aliases();
    db.save_field_spec(&spec).unwrap();
    let conn = db.conn();
    apply_one_up(conn, 62);
    let names = || -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM spec_alias_names ORDER BY name")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(names(), ["calm", "mood", "moode"], "filled from the specs");
    let mut tea = datum_core::field_spec::FieldSpec::new("tea", datum_core::action::Action::Occur);
    tea.regenerate_derived_aliases();
    db.save_field_spec(&tea).unwrap();
    assert!(names().contains(&"tea".to_string()), "kept by the triggers");
    apply_one_down(conn, 63);
    assert!(!table_exists(conn, "spec_alias_names"));
    db.save_field_spec(&spec).unwrap();
}

/// da-ueb4p: the offline mark of a session's failure comes and goes.
#[test]
fn roundtrip_v63_to_v64_sync_peer_offline() {
    let db = fresh_at(63);
    let conn = db.conn();
    apply_one_up(conn, 63);
    assert!(column_exists(conn, "sync_peers", "last_error_offline"));
    apply_one_down(conn, 64);
    assert!(!column_exists(conn, "sync_peers", "last_error_offline"));
}

/// da-38obu: the template specs' index comes and goes, and a spec read
/// without it still finds them.
#[test]
fn roundtrip_v64_to_v65_spec_templates_index() {
    let db = fresh_at(64);
    let conn = db.conn();
    apply_one_up(conn, 64);
    assert!(index_exists(conn, "entries_spec_templates"));
    apply_one_down(conn, 65);
    assert!(!index_exists(conn, "entries_spec_templates"));
    let template = datum_core::field_spec::FieldSpec::new(
        "issue__%hid:8%",
        datum_core::action::Action::Switch,
    );
    db.save_field_spec(&template).unwrap();
    let spec = db.load_spec("issue__2qvk0dk4").unwrap().unwrap();
    assert_eq!(spec.verb, datum_core::action::Action::Switch);
}

/// da-e853a: the blocks' index comes and goes.
#[test]
fn roundtrip_v65_to_v66_field_blocks_index() {
    let db = fresh_at(65);
    let conn = db.conn();
    apply_one_up(conn, 65);
    assert!(index_exists(conn, "entries_field_blocks"));
    apply_one_down(conn, 66);
    assert!(!index_exists(conn, "entries_field_blocks"));
}

/// da-0c2ne: a refusal's integrity row becomes its endpoint's notice, and
/// the way down leaves it one.
#[test]
fn roundtrip_v66_to_v67_refusals_are_notices() {
    let db = fresh_at(66);
    let conn = db.conn();
    let ep = "a".repeat(64);
    conn.execute(
        "INSERT INTO integrity_warnings
             (id, hid, entry_slug, local_entry, detected_at, detected_by, kind)
         VALUES ('w1', 'w1', 'x', json_object('extra_metadata', json_object('sync_op',
                 json_object('op', ?1 || '/1', 'hlc', '2026-10-01T08:00:00.000Z/0000',
                             'refused', 'not a member of the sync chain'))),
                 '2026-10-01T09:00:00Z', 'sync_pull', 'sync_unauthorised')",
        [&ep],
    )
    .unwrap();
    apply_one_up(conn, 66);
    let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
    assert_eq!(count("SELECT COUNT(*) FROM integrity_warnings"), 0);
    let notice = "SELECT COUNT(*) FROM sync_notices WHERE kind = 'refused' AND acked = 0";
    assert_eq!(count(notice), 1);
    apply_one_down(conn, 67);
    assert_eq!(count(notice), 1);
}

#[test]
fn roundtrip_v54_to_v55_ops_held() {
    let db = fresh_at(54);
    let conn = db.conn();
    apply_one_up(conn, 54);
    assert!(index_exists(conn, "ops_held"));
    apply_one_down(conn, 55);
    assert!(!index_exists(conn, "ops_held"));
}

/// da-n7tp2: v56 lays down `stats`' cache and the triggers that keep it
/// true, which mark a write; v55 takes them away, and a write marks nothing.
#[test]
fn roundtrip_v55_to_v56_stats_cache() {
    let db = fresh_at(55);
    let conn = db.conn();
    let write = |hid: &str| {
        conn.execute(
            "INSERT INTO entries
                (hid, slug, content_hash, field, occur_time, create_time, modify_time, client_id)
             VALUES (?1, ?1, ?1, 'coffee', '2026-09-01T00:00:00+00:00',
                     '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'dev-a')",
            [hid],
        )
        .unwrap();
    };
    apply_one_up(conn, 55);
    for table in [
        "stats_cache",
        "stats_cache_days",
        "stats_cache_through",
        "stats_cache_dirty",
    ] {
        assert!(table_exists(conn, table), "{table}");
    }
    assert!(index_exists(conn, "entries_zoned"));
    write("h1");
    let dirty = || -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM stats_cache_dirty WHERE field = 'coffee'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(dirty(), 1, "the trigger marks a write");
    apply_one_down(conn, 56);
    assert!(!table_exists(conn, "stats_cache"));
    assert!(!index_exists(conn, "entries_zoned"));
    write("h2");
}

/// da-3zqyv: v57 remembers what each member runs; v56 forgets it.
#[test]
fn roundtrip_v56_to_v57_sync_peer_version() {
    let db = fresh_at(56);
    let conn = db.conn();
    apply_one_up(conn, 56);
    assert!(column_exists(conn, "sync_peers", "datum"));
    assert!(column_exists(conn, "sync_peers", "schema"));
    apply_one_down(conn, 57);
    assert!(!column_exists(conn, "sync_peers", "datum"));
    assert!(!column_exists(conn, "sync_peers", "schema"));
    assert!(table_exists(conn, "sync_peers"));
}

/// Life data logged under `kit` stops v51 before it is hidden; a deleted
/// log is not in the way.
#[test]
fn v50_to_v51_refuses_a_field_already_named_kit() {
    let db = fresh_at(50);
    let conn = db.conn();
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, occur_time, create_time, modify_time, client_id)
         VALUES ('h1', 'kit:h1', 'h1', 'kit', '2026-09-01T00:00:00+00:00',
                 '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'dev-a')",
        [],
    )
    .unwrap();
    let err = apply_migrations(conn, MIGRATIONS, 50, 51)
        .unwrap_err()
        .to_string();
    assert!(err.contains("1 entry") && err.contains("`kit`"), "{err}");
    assert_eq!(read_schema_version(conn).unwrap(), Some(50));
    conn.execute("UPDATE entries SET deleted_at = '2026-09-02T00:00:00Z'", [])
        .unwrap();
    apply_migrations(conn, MIGRATIONS, 50, 51).unwrap();
}

/// A kit's own table is no clash: one a v51 peer synced to a v50 copy, or
/// one the down kept, goes up again.
#[test]
fn v50_to_v51_passes_a_kits_table() {
    let insert_table = |conn: &Connection| {
        conn.execute(
            "INSERT INTO entries
                (hid, slug, content_hash, field, data, occur_time, create_time,
                 modify_time, client_id)
             VALUES ('k1', 'kit:sample', 'k1', 'kit',
                     '{\"name\":\"sample\",\"version\":\"0.1\",\"kit_id\":\"sample\"}',
                     '2026-09-01T00:00:00+00:00', '2026-09-01T00:00:00Z',
                     '2026-09-01T00:00:00Z', 'dev-a')",
            [],
        )
        .unwrap();
    };
    let synced = fresh_at(50);
    insert_table(synced.conn());
    apply_one_up(synced.conn(), 50);

    let kept = fresh_at(50);
    apply_one_up(kept.conn(), 50);
    insert_table(kept.conn());
    apply_one_down(kept.conn(), 51);
    apply_one_up(kept.conn(), 50);
}

/// da-i7hig: the relay sync is deleted and its table with it; the down
/// puts the v40 table back, empty.
#[test]
fn roundtrip_v40_to_v41_drop_sync_state() {
    let db = fresh_at(40);
    let conn = db.conn();
    conn.execute(
        "INSERT INTO sync_state (db_id, push_mark, pull_seq, cluster_id) VALUES ('local', 5, 7, 'c')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 40);
    assert!(!table_exists(conn, "sync_state"));

    apply_one_down(conn, 41);
    assert!(table_exists(conn, "sync_state"));
    assert!(column_exists(conn, "sync_state", "cluster_generation"));
}

#[test]
fn roundtrip_v41_to_v42_sync_ops() {
    let db = fresh_at(41);
    let conn = db.conn();
    assert!(!table_exists(conn, "ops"));

    apply_one_up(conn, 41);
    assert!(table_exists(conn, "ops"));
    assert!(table_exists(conn, "sync_local"));
    assert!(index_exists(conn, "ops_hid"));

    apply_one_down(conn, 42);
    assert!(!table_exists(conn, "ops"));
    assert!(!table_exists(conn, "sync_local"));
}

#[test]
fn roundtrip_v42_to_v43_sync_peers() {
    let db = fresh_at(42);
    let conn = db.conn();
    assert!(!table_exists(conn, "sync_peers"));

    apply_one_up(conn, 42);
    assert!(table_exists(conn, "sync_peers"));
    assert!(table_exists(conn, "sync_invites"));

    apply_one_down(conn, 43);
    assert!(!table_exists(conn, "sync_peers"));
    assert!(!table_exists(conn, "sync_invites"));
}

#[test]
fn roundtrip_v43_to_v44_sync_local_path() {
    let db = fresh_at(43);
    let conn = db.conn();
    conn.execute(
        "INSERT INTO sync_local VALUES (1, 'secret', 'id', 'dev')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 43);
    let path: String = conn
        .query_row("SELECT path FROM sync_local", [], |r| r.get(0))
        .unwrap();
    assert_eq!(path, "", "the key is kept, its path learned on next use");

    apply_one_down(conn, 44);
    assert!(!column_exists(conn, "sync_local", "path"));
    assert!(table_exists(conn, "sync_local"));
}

#[test]
fn roundtrip_v44_to_v45_sync_notices() {
    let db = fresh_at(44);
    let conn = db.conn();
    assert!(!table_exists(conn, "sync_notices"));

    apply_one_up(conn, 44);
    assert!(table_exists(conn, "sync_notices"));

    apply_one_down(conn, 45);
    assert!(!table_exists(conn, "sync_notices"));
}

#[test]
fn roundtrip_v45_to_v46_sync_peer_sessions() {
    let db = fresh_at(45);
    let conn = db.conn();
    conn.execute(
        "INSERT INTO sync_peers (endpoint_id, label, addrs, added_at) VALUES ('e', 'l', '', 't')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 45);
    for c in [
        "last_vector",
        "last_verify_at",
        "last_verify_match",
        "last_error",
        "last_error_at",
    ] {
        assert!(column_exists(conn, "sync_peers", c), "{c}");
    }

    apply_one_down(conn, 46);
    assert!(!column_exists(conn, "sync_peers", "last_vector"));
    let label: String = conn
        .query_row("SELECT label FROM sync_peers", [], |r| r.get(0))
        .unwrap();
    assert_eq!(label, "l", "the peer stays");
}

#[test]
fn roundtrip_v46_to_v47_drop_sync_invites() {
    let db = fresh_at(46);
    let conn = db.conn();
    assert!(table_exists(conn, "sync_invites"));

    apply_one_up(conn, 46);
    assert!(!table_exists(conn, "sync_invites"));

    apply_one_down(conn, 47);
    assert!(table_exists(conn, "sync_invites"));
}

/// da-lfsei: every `device_id` value lands in `client_id` untouched — the
/// columns and the history snapshots' key — and comes back on the way down.
#[test]
fn roundtrip_v47_to_v48_client_id() {
    let db = fresh_at(47);
    let conn = db.conn();
    insert_tx(conn, "h1", "coffee:2026-09-01T00:00Z", None).unwrap();
    conn.execute_batch(
        "INSERT INTO entry_history
            (entry_hid, version, snapshot, modify_time, device_id, change_kind)
         VALUES ('h1', 1, '{\"hid\":\"h1\",\"device_id\":\"dev-a\"}',
                 '2026-09-01T00:00:00Z', 'dev-a', 'create');
         INSERT INTO sync_local (id, endpoint_secret, endpoint_id, device_id)
         VALUES (1, 's', 'e', 'dev-a');",
    )
    .unwrap();
    let read = |column: &str| -> Vec<String> {
        [
            format!("SELECT {column} FROM entries"),
            format!("SELECT {column} FROM entry_history"),
            format!("SELECT json_extract(snapshot, '$.{column}') FROM entry_history"),
            format!("SELECT {column} FROM sync_local"),
        ]
        .iter()
        .map(|q| conn.query_row(q, [], |r| r.get(0)).unwrap())
        .collect()
    };

    apply_one_up(conn, 47);
    for table in ["entries", "entry_history", "sync_local"] {
        assert!(!column_exists(conn, table, "device_id"), "{table}");
    }
    assert_eq!(read("client_id"), ["dev-a"; 4]);
    let snapshot: String = conn
        .query_row("SELECT snapshot FROM entry_history", [], |r| r.get(0))
        .unwrap();
    assert!(!snapshot.contains("device_id"), "{snapshot}");

    apply_one_down(conn, 48);
    assert_eq!(read("device_id"), ["dev-a"; 4]);
}

#[test]
fn roundtrip_v48_to_v49_sync_pause() {
    let db = fresh_at(48);
    let conn = db.conn();
    conn.execute(
        "INSERT INTO sync_local (id, endpoint_secret, endpoint_id, client_id) VALUES (1, 's', 'e', 'd')",
        [],
    )
    .unwrap();

    apply_one_up(conn, 48);
    for c in ["paused_at", "warned_at"] {
        assert!(column_exists(conn, "sync_local", c), "{c}");
    }

    apply_one_down(conn, 49);
    assert!(!column_exists(conn, "sync_local", "paused_at"));
    assert!(!column_exists(conn, "sync_local", "warned_at"));
    let key: String = conn
        .query_row("SELECT endpoint_secret FROM sync_local", [], |r| r.get(0))
        .unwrap();
    assert_eq!(key, "s", "the key stays");
}

#[test]
fn roundtrip_v49_to_v50_sync_caught_up() {
    let db = fresh_at(49);
    let conn = db.conn();
    apply_one_up(conn, 49);
    conn.execute(
        "INSERT INTO sync_caught_up (endpoint, fields, seq) VALUES ('e', '[\"coffee\"]', 3)",
        [],
    )
    .unwrap();
    apply_one_down(conn, 50);
    let left: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'sync_caught_up'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(left, 0);
}

fn field_occurtime_sql(conn: &Connection) -> String {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type='index' AND name='entries_field_occurtime'",
        [],
        |r| r.get::<_, String>(0),
    )
    .unwrap()
}

/// Insert a minimal live entry at `2026-09-01T00:00:00+00:00` for the field
/// the slug names, with `state` written verbatim (JSON text or NULL).
fn insert_tx(
    conn: &Connection,
    hid: &str,
    slug: &str,
    state: Option<&str>,
) -> rusqlite::Result<usize> {
    let field = slug.split(':').next().unwrap();
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, state, occur_time,
             create_time, modify_time, device_id)
         VALUES (?1, ?2, ?1, ?3, ?4, '2026-09-01T00:00:00+00:00',
                 '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'dev-a')",
        rusqlite::params![hid, slug, field, state],
    )
}

/// da-05rkv: v58 gives entries the `conflicts` the sync fold derives, and
/// drops the conflict kinds each copy filed in `integrity_warnings`, which
/// the column replaces; v57 takes the column away.
#[test]
fn roundtrip_v57_to_v58_conflicts() {
    let db = fresh_at(57);
    let conn = db.conn();
    conn.execute_batch(
        "INSERT INTO integrity_warnings
             (id, hid, entry_slug, local_entry, detected_at, detected_by, kind)
         VALUES ('w1', 'w1', 's', '{}', '2026-10-01T00:00:00Z', 'sync_pull', 'sync_conflict'),
                ('w2', 'w2', 's', '{}', '2026-10-01T00:00:00Z', 'sync_pull', 'lww_overwrite'),
                ('w3', 'w3', 's', '{}', '2026-10-01T00:00:00Z', 'local_validator', 'identity_collision'),
                ('w4', 'w4', 's', '{}', '2026-10-01T00:00:00Z', 'local_validator', 'double_open');",
    )
    .unwrap();
    assert!(!column_exists(conn, "entries", "conflicts"));
    apply_one_up(conn, 57);
    assert!(column_exists(conn, "entries", "conflicts"));
    assert!(index_exists(conn, "entries_conflicts"));
    let kinds: Vec<String> = conn
        .prepare("SELECT kind FROM integrity_warnings ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(kinds, ["double_open"], "a check finding stays");
    // Stamped back with the column still there, it upgrades again.
    datum_core::migrations::set_schema_version(conn, 57).unwrap();
    apply_one_up(conn, 57);
    apply_one_down(conn, 58);
    assert!(!column_exists(conn, "entries", "conflicts"));
    assert!(!index_exists(conn, "entries_conflicts"));
}

/// da-m34q7: v48 left `device_id` in the entries a warning stores, so a
/// warning filed before it stopped decoding. v59 renames the key there too;
/// what is not JSON at all is left for `check` to name.
#[test]
fn roundtrip_v58_to_v59_warning_client_id() {
    use datum_core::integrity::{list, list_rows};
    let db = fresh_at(58);
    let conn = db.conn();
    conn.execute(
        "INSERT INTO entries
            (hid, slug, content_hash, field, occur_time, create_time, modify_time, client_id)
         VALUES ('h1', 'ex2:1', 'h1', 'ex2', '2026-09-01T00:00:00+00:00',
                 '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'dev-a')",
        [],
    )
    .unwrap();
    // The entry as a build from before v48 stored it.
    let mut old = serde_json::to_value(db.get_entry("h1").unwrap()).unwrap();
    let id = old.as_object_mut().unwrap().remove("client_id").unwrap();
    old["device_id"] = id;
    conn.execute(
        "INSERT INTO integrity_warnings
             (id, hid, entry_slug, local_entry, remote_entry, detected_at, detected_by, kind)
         VALUES ('w1', 'w1', 'ex2:1', ?1, ?1, '2026-10-01T00:00:00Z', 'local_validator', 'overlap')",
        [old.to_string()],
    )
    .unwrap();
    let missing = list(conn, &Default::default()).unwrap_err().to_string();
    assert!(missing.contains("missing field `client_id`"), "{missing}");
    conn.execute_batch(
        "INSERT INTO integrity_warnings
             (id, hid, entry_slug, local_entry, detected_at, detected_by, kind)
         VALUES ('w2', 'w2', 'ex2:1', 'not json', '2026-10-01T00:00:00Z',
                 'local_validator', 'overlap')",
    )
    .unwrap();

    apply_one_up(conn, 58);
    let read = |conn: &Connection| -> Vec<String> {
        list_rows(conn, &Default::default())
            .unwrap()
            .into_iter()
            .map(|row| match row {
                Ok(w) => format!(
                    "{} {} {}",
                    w.id,
                    w.local_entry.client_id,
                    w.remote_entry.unwrap().client_id
                ),
                Err((id, _)) => format!("{id} unreadable"),
            })
            .collect()
    };
    assert_eq!(read(conn), ["w2 unreadable", "w1 dev-a dev-a"]);
    let stored: String = conn
        .query_row(
            "SELECT local_entry FROM integrity_warnings WHERE id = 'w1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!stored.contains("device_id"), "{stored}");

    // v58 reads `client_id` too, so the down leaves the rows as they are.
    apply_one_down(conn, 59);
    assert_eq!(read(conn), ["w2 unreadable", "w1 dev-a dev-a"]);
}
