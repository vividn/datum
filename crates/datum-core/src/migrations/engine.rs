//! The migration runner: version bookkeeping, the forward and backward
//! walkers, and the pre-migration snapshot.
//!
//! This module knows how to *walk* a registry; it does not know what any
//! individual step does. That separation is the point — the registry in
//! [`super`] grows with the schema, the runner does not.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{DatumError, Result};

use super::support::has_column;
use super::{Migration, CURRENT_SCHEMA_VERSION, MIGRATIONS};

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
/// version we should stamp it at, by introspecting actual table/column
/// shapes — never blindly 1.
///
/// **Why this matters (2026-04-30 incident).** Before this function was
/// shape-aware, it returned 1 unconditionally and `schema.sql` carried a
/// literal `INSERT OR IGNORE INTO _meta VALUES ('schema_version', '18')`
/// seed. Together those two paths produced silent half-states: a legacy
/// DB without `_meta` had its version stamped at 18 by schema.sql, then
/// `migrate_to_current` saw no work to do, and migrations v1..v17 — including
/// the `m12_to_13` step that adds `field_specs.kit_id` — never ran. The
/// next `dm` invocation hit "no such column: kit_id". Fix: schema.sql no
/// longer seeds `_meta`; this function does the stamping after walking the
/// real shape.
///
/// **Floor at v3.** Pre-v3 entries (ULID `id` PK) are rejected upstream in
/// `Db::migrate`. A fresh DB (no datum tables yet) short-circuits to
/// [`CURRENT_SCHEMA_VERSION`] before any column probe can return low, so
/// the destructive migrations v1→v2 (DROP/CREATE `sync_state`) and v2→v3
/// (DROP `entries`) are unreachable via infer — both would wipe live rows
/// if they ran against a real-data DB. Real-data DBs always have at least
/// one datum table present, fall through past the short-circuit, and the
/// column probes return ≥ 6 (or [`CURRENT_SCHEMA_VERSION`] when fully
/// current).
///
/// **How we detect.** First a fresh-DB short-circuit: if no non-`_meta`
/// datum table exists, the DB was just created and schema.sql will lay
/// down the current shape — stamp current and skip the registry walk.
/// Otherwise walk the column-add migrations in order. Adding a column to
/// a pre-existing table is the one operation `CREATE TABLE IF NOT EXISTS`
/// cannot mask: if a legacy DB is at v12 shape, `field_specs.kit_id` is
/// genuinely absent, and we can prove it. The first missing column fixes
/// the resume point at the version *immediately before* the migration
/// that adds it.
pub fn infer_legacy_version(conn: &Connection) -> Result<u32> {
    // Fresh-DB short-circuit. `Db::migrate` no longer runs schema.sql
    // before this function, so a brand-new file has zero datum tables.
    // Stamp at CURRENT_SCHEMA_VERSION so schema.sql can lay down the
    // latest shape directly — and so the destructive v1→v2 / v2→v3
    // migrations (which assume legacy `sync_state` / `entries` tables to
    // drop) are unreachable. `_meta` is excluded: `read_schema_version`
    // creates it as a side-effect via `ensure_meta_table`, so its presence
    // is not evidence of pre-existing user data.
    let datum_tables: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != '_meta'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if datum_tables == 0 {
        return Ok(CURRENT_SCHEMA_VERSION);
    }

    // m6_to_7: sync_state.my_push_seq.
    if !has_column(conn, "sync_state", "my_push_seq")? {
        return Ok(6);
    }
    // m7_to_8: entries.user_asserted_last_state.
    if !has_column(conn, "entries", "user_asserted_last_state")? {
        return Ok(7);
    }
    // m11_to_12: entries.extra_metadata.
    if !has_column(conn, "entries", "extra_metadata")? {
        return Ok(11);
    }
    // m12_to_13: field_specs.kit_id — the 2026-04-30 regression site.
    if !has_column(conn, "field_specs", "kit_id")? {
        return Ok(12);
    }
    // m15_to_16: aliases.kit_id.
    if !has_column(conn, "aliases", "kit_id")? {
        return Ok(15);
    }
    // m17_to_18: field_specs.tags.
    if !has_column(conn, "field_specs", "tags")? {
        return Ok(17);
    }
    // m18_to_19: field_specs.render_json.
    if !has_column(conn, "field_specs", "render_json")? {
        return Ok(18);
    }
    // m19_to_20: time-shape pivot — entries.start_time was dropped in
    // favour of entries.occur_time. A v=19 DB still carries `start_time`
    // post-schema.sql (the `CREATE TABLE IF NOT EXISTS` no-ops on the
    // existing v19 entries shape), so its presence pins the DB at v=19
    // and the runner walks it forward through the rebuild.
    if has_column(conn, "entries", "start_time")? {
        return Ok(19);
    }
    // m20_to_21: aliases.derived_from_field was dropped at v=21. Its
    // presence pins the DB at v=20.
    if has_column(conn, "aliases", "derived_from_field")? {
        return Ok(20);
    }
    // m22_to_23: entries.unit was dropped (and entries.dur_precision
    // added) at v=23. A legacy v=22 DB still carries `unit` post-
    // schema.sql because `CREATE TABLE IF NOT EXISTS` no-ops on the
    // existing v=22 entries shape; its presence pins the DB at v=22 so
    // the runner walks the rebuild forward. Pre-v22 verb canonicalization
    // (m21→m22) is an in-place UPDATE with no structural footprint, so
    // we do not probe for it here.
    if has_column(conn, "entries", "unit")? {
        return Ok(22);
    }
    // All column-add features present → DB is already at current shape.
    // Idempotent re-runs are CREATE/INDEX-IF-NOT-EXISTS and will no-op.
    Ok(CURRENT_SCHEMA_VERSION)
}

/// Run forward migrations from `current` up to `target`, using the given
/// registry. All pending steps run inside a single transaction so a partial
/// failure rolls back to `current` instead of leaving a half-migrated DB.
/// Returns a list of summary strings for each migration applied.
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
    let tx = conn.unchecked_transaction()?;
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
        (next.apply)(&tx)?;
        version = next.to;
        set_schema_version(&tx, version)?;
        applied.push(format!("v{}→v{}: {}", next.from, next.to, next.summary));
    }
    tx.commit()?;
    Ok(applied)
}

/// Status row reported by [`migration_status`].
#[derive(Debug, Clone)]
pub struct MigrationStatusRow {
    pub from: u32,
    pub to: u32,
    pub summary: &'static str,
    pub applied: bool,
}

/// Snapshot of migration state for reporting. Returns the current stored
/// schema version, the binary's target version, and a row per registered
/// migration tagged as applied/pending.
pub fn migration_status(
    conn: &Connection,
    migrations: &[Migration],
) -> Result<(u32, u32, Vec<MigrationStatusRow>)> {
    let current = read_schema_version(conn)?.unwrap_or(CURRENT_SCHEMA_VERSION);
    let target = CURRENT_SCHEMA_VERSION;
    let rows = migrations
        .iter()
        .map(|m| MigrationStatusRow {
            from: m.from,
            to: m.to,
            summary: m.summary,
            applied: m.to <= current,
        })
        .collect();
    Ok((current, target, rows))
}

/// Walk the registry backward from `current` down to `target`, calling
/// each step's `down` in version-descending order. All steps run inside
/// a single transaction so a partial failure rolls back to `current`.
/// Returns one summary line per applied down step.
pub fn apply_migrations_down(
    conn: &Connection,
    migrations: &[Migration],
    current: u32,
    target: u32,
) -> Result<Vec<String>> {
    if current == target {
        return Ok(vec![]);
    }
    if current < target {
        return Err(DatumError::IncompatibleSchema(format!(
            "down-walk requires current ({current}) >= target ({target})"
        )));
    }
    let tx = conn.unchecked_transaction()?;
    let mut applied = Vec::new();
    let mut version = current;
    while version > target {
        let step = migrations.iter().find(|m| m.to == version).ok_or_else(|| {
            DatumError::IncompatibleSchema(format!(
                "no down-migration path from schema v{version} toward v{target}"
            ))
        })?;
        (step.down)(&tx)?;
        version = step.from;
        set_schema_version(&tx, version)?;
        applied.push(format!(
            "v{}→v{}: down — {}",
            step.to, step.from, step.summary
        ));
    }
    tx.commit()?;
    Ok(applied)
}

/// Walk the module-level [`MIGRATIONS`] registry backward to `target`.
/// Convenience wrapper around [`apply_migrations_down`].
pub fn migrate_down_to(conn: &Connection, target: u32) -> Result<Vec<String>> {
    let current = read_schema_version(conn)?.unwrap_or(CURRENT_SCHEMA_VERSION);
    apply_migrations_down(conn, MIGRATIONS, current, target)
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

/// The schema version a DB is at, resolved without writing to it.
///
/// [`migrate_to_current`] resolves the same value but stamps `_meta` as a
/// side-effect. The pre-migration backup has to know `from` *before* any
/// write lands, so this variant infers without stamping.
pub fn peek_schema_version(conn: &Connection) -> Result<u32> {
    let has_meta: bool = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_meta'",
        [],
        |row| row.get::<_, i64>(0),
    )? > 0;
    if has_meta {
        let stored: Option<String> = conn
            .query_row(
                "SELECT value FROM _meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(s) = stored {
            return s
                .parse::<u32>()
                .map_err(|e| DatumError::Other(format!("bad schema_version in _meta: {e}")));
        }
    }
    infer_legacy_version(conn)
}

/// What a single [`crate::db::Db::open`] did to the schema, so the CLI can
/// say so. `datum-core` is deliberately silent — the migration happens
/// here, the telling happens at the edge.
#[derive(Debug, Clone)]
pub struct MigrationReport {
    pub from_version: u32,
    pub to_version: u32,
    /// The pre-migration snapshot. `None` only for in-memory databases,
    /// which have no file to snapshot.
    pub backup_path: Option<std::path::PathBuf>,
    /// One summary line per migration step applied.
    pub applied: Vec<String>,
    /// Older snapshots removed by retention after this migration.
    pub pruned: Vec<std::path::PathBuf>,
}

/// How many pre-migration snapshots to keep alongside a database. Older
/// ones are pruned after a successful migration — never before, so a
/// failed migration can never cost you a backup.
pub const MAX_PRE_MIGRATION_BACKUPS: usize = 5;

/// Snapshot the database file before migrating it. Returns the path written.
///
/// `VACUUM INTO`, not a filesystem copy: connections run in WAL mode, so
/// the `.db` file on its own is not the whole database, and a copy taken
/// without the `-wal` sidecar can be stale. `VACUUM INTO` writes one
/// self-contained file holding exactly the committed contents, and — the
/// point of the exercise — it goes through SQLite, not through datum's
/// serialisation code, which is the code the migration is changing.
///
/// The caller must treat a failure here as fatal: a migration that runs
/// after a failed backup is worse than no backup policy at all, because
/// the note printed afterwards would claim a snapshot that isn't there.
pub fn backup_before_migration(
    conn: &Connection,
    db_path: &std::path::Path,
    from: u32,
    to: u32,
) -> Result<std::path::PathBuf> {
    let dest = free_backup_path(db_path, from, to);
    // `VACUUM INTO` refuses to overwrite, so an existing file is an error
    // rather than silent data loss — exactly what we want.
    conn.execute("VACUUM INTO ?1", params![dest.to_string_lossy()])
        .map_err(|e| {
            DatumError::Other(format!(
                "refusing to migrate schema v{from}→v{to}: could not write the \
                 pre-migration backup to {}: {e}",
                dest.display()
            ))
        })?;
    Ok(dest)
}

/// `<db file name>.pre-v<from>-to-v<to>-<UTC timestamp>.bak`, next to the
/// database, with a `-<n>` disambiguator if that name is somehow taken.
fn free_backup_path(db_path: &std::path::Path, from: u32, to: u32) -> std::path::PathBuf {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let base = format!(
        "{}.pre-v{from}-to-v{to}-{stamp}",
        db_path.file_name().unwrap_or_default().to_string_lossy()
    );
    let dir = db_path.parent().unwrap_or(std::path::Path::new("."));
    let mut candidate = dir.join(format!("{base}.bak"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base}-{n}.bak"));
        n += 1;
    }
    candidate
}

/// Drop all but the newest [`MAX_PRE_MIGRATION_BACKUPS`] snapshots taken
/// for this database. Best-effort: anything we cannot read or remove is
/// left alone, since failing to prune is harmless and refusing to open the
/// database over it would not be.
pub fn prune_pre_migration_backups(db_path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let (Some(dir), Some(name)) = (db_path.parent(), db_path.file_name()) else {
        return Vec::new();
    };
    let prefix = format!("{}.pre-v", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with(&prefix) && n.ends_with(".bak")
        })
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.path()))
        })
        .collect();
    if found.len() <= MAX_PRE_MIGRATION_BACKUPS {
        return Vec::new();
    }
    // Newest first, then discard everything past the keep window.
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    found
        .split_off(MAX_PRE_MIGRATION_BACKUPS)
        .into_iter()
        .filter(|(_, path)| std::fs::remove_file(path).is_ok())
        .map(|(_, path)| path)
        .collect()
}
