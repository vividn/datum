//! Schema version tracking and forward migrations.
//!
//! Every DB carries a `_meta.schema_version` row that this binary compares
//! against [`CURRENT_SCHEMA_VERSION`]. Forward-compatible migrations
//! (ADD COLUMN, CREATE INDEX IF NOT EXISTS, CREATE TABLE IF NOT EXISTS)
//! run silently on open; anything destructive (column removal, PK change)
//! must go through `datum setup --force-recreate` or a proper migration
//! entry here.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{DatumError, Result};

/// The schema version this binary writes.
///
/// Bump this whenever `schema.sql` changes AND append a [`Migration`] that
/// lifts the previous version to the new one. The invariant is that a DB
/// at any version in `[1, CURRENT_SCHEMA_VERSION]` can be migrated to the
/// current shape by walking [`MIGRATIONS`] in order.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// A single forward migration between two adjacent schema versions.
///
/// Migrations run inside a transaction opened by the caller. `apply` must
/// be idempotent against a partially-applied state where possible — in
/// practice this means using `CREATE … IF NOT EXISTS` / `ALTER … ADD
/// COLUMN` patterns so interrupted runs can be retried safely.
pub struct Migration {
    pub from: u32,
    pub to: u32,
    pub summary: &'static str,
    pub apply: fn(&Connection) -> Result<()>,
}

/// Ordered forward migrations. Append-only — never reorder, rename, or
/// delete entries here. The current shipped schema is v1, so this table
/// is empty; future schema changes bump [`CURRENT_SCHEMA_VERSION`] and
/// add their lift here.
pub const MIGRATIONS: &[Migration] = &[];

/// Create the `_meta` key/value table if it does not exist.
pub fn ensure_meta_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _meta (
            key    TEXT PRIMARY KEY,
            value  TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// Read the stored schema version, if any.
pub fn read_schema_version(conn: &Connection) -> Result<Option<u32>> {
    ensure_meta_table(conn)?;
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    match value {
        Some(s) => s
            .parse::<u32>()
            .map(Some)
            .map_err(|e| DatumError::Other(format!("bad schema_version in _meta: {e}"))),
        None => Ok(None),
    }
}

/// Write the schema version.
pub fn set_schema_version(conn: &Connection, version: u32) -> Result<()> {
    ensure_meta_table(conn)?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![version.to_string()],
    )?;
    Ok(())
}

/// Detect the schema shape of a DB that predates `_meta`. Returns the
/// version we should stamp it at. For now, any slug-as-PK shape is v1;
/// anything older is rejected upstream by `Db::open`.
pub fn infer_legacy_version(_conn: &Connection) -> Result<u32> {
    // The legacy check in `Db::migrate()` has already rejected pre-refactor
    // ULID-based tables by the time we get here. Anything else is v1.
    Ok(1)
}

/// Run forward migrations from `current` up to `target`, using the given
/// registry. Returns a list of summary strings for each migration applied.
pub fn apply_migrations(
    conn: &Connection,
    migrations: &[Migration],
    current: u32,
    target: u32,
) -> Result<Vec<String>> {
    if current == target {
        return Ok(vec![]);
    }
    if current > target {
        return Err(DatumError::IncompatibleSchema(format!(
            "database schema version {current} is newer than this binary supports \
             (expected ≤ {target}); upgrade datum"
        )));
    }
    let mut applied = Vec::new();
    let mut version = current;
    while version < target {
        let next = migrations
            .iter()
            .find(|m| m.from == version)
            .ok_or_else(|| {
                DatumError::IncompatibleSchema(format!(
                    "no migration path from schema v{version} to v{target}"
                ))
            })?;
        (next.apply)(conn)?;
        version = next.to;
        set_schema_version(conn, version)?;
        applied.push(format!("v{}→v{}: {}", next.from, next.to, next.summary));
    }
    Ok(applied)
}

/// Migrate an open connection to [`CURRENT_SCHEMA_VERSION`] using the
/// module-level [`MIGRATIONS`] registry. Returns the summary lines.
pub fn migrate_to_current(conn: &Connection) -> Result<Vec<String>> {
    let current = match read_schema_version(conn)? {
        Some(v) => v,
        None => {
            let inferred = infer_legacy_version(conn)?;
            set_schema_version(conn, inferred)?;
            inferred
        }
    };
    apply_migrations(conn, MIGRATIONS, current, CURRENT_SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn fresh_db_stamps_current_schema_version() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(
            read_schema_version(db.conn()).unwrap(),
            Some(CURRENT_SCHEMA_VERSION)
        );
    }

    #[test]
    fn opening_db_with_newer_schema_version_errors() {
        let db = Db::open_in_memory().unwrap();
        // Force _meta.schema_version above what this binary supports.
        set_schema_version(db.conn(), CURRENT_SCHEMA_VERSION + 5).unwrap();
        drop(db);

        // Re-running migrate_to_current directly (Db::open_in_memory would
        // lose state) emulates what Db::migrate does on a file-backed DB.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        set_schema_version(&conn, CURRENT_SCHEMA_VERSION + 1).unwrap();
        let err = migrate_to_current(&conn).unwrap_err();
        assert!(
            matches!(err, DatumError::IncompatibleSchema(_)),
            "expected IncompatibleSchema, got {err:?}"
        );
    }

    #[test]
    fn apply_migrations_runs_registered_steps_in_order() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        set_schema_version(&conn, 1).unwrap();

        fn m1_to_2(conn: &Connection) -> Result<()> {
            conn.execute_batch("CREATE TABLE IF NOT EXISTS synth_v2 (x INTEGER);")?;
            Ok(())
        }
        fn m2_to_3(conn: &Connection) -> Result<()> {
            conn.execute_batch("CREATE TABLE IF NOT EXISTS synth_v3 (y INTEGER);")?;
            Ok(())
        }
        let migrations = &[
            Migration { from: 1, to: 2, summary: "v2 table", apply: m1_to_2 },
            Migration { from: 2, to: 3, summary: "v3 table", apply: m2_to_3 },
        ];
        let applied = apply_migrations(&conn, migrations, 1, 3).unwrap();
        assert_eq!(applied.len(), 2);
        assert_eq!(read_schema_version(&conn).unwrap(), Some(3));
        // Both migrated tables exist.
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('synth_v2','synth_v3')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn apply_migrations_errors_when_no_path_exists() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        ensure_meta_table(&conn).unwrap();
        set_schema_version(&conn, 1).unwrap();
        let err = apply_migrations(&conn, &[], 1, 2).unwrap_err();
        assert!(
            matches!(err, DatumError::IncompatibleSchema(_)),
            "expected IncompatibleSchema for missing migration path, got {err:?}"
        );
    }

    #[test]
    fn force_recreate_resets_schema_version_to_current() {
        let db = Db::open_in_memory().unwrap();
        // Seed _meta.version to something wrong, then force-recreate.
        set_schema_version(db.conn(), 99).unwrap();
        db.force_recreate_schema().unwrap();
        assert_eq!(
            read_schema_version(db.conn()).unwrap(),
            Some(CURRENT_SCHEMA_VERSION)
        );
    }
}
