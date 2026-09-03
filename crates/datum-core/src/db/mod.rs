//! SQLite database layer for datum.
//!
//! All entry data, field specs, and aliases live in a single SQLite file.
//! The schema is designed to be CRDT-friendly (append-mostly, ULID-ordered).
//!
//! FieldSpec stores a field's schema (default verb, allowed states, color,
//! derived aliases). Tracking state — whether a field is tracked or ignored
//! — is NOT on the spec; it derives from the entry log via [`Db::state_at`].
//! [`Db::get_tracked_fields`] is a thin projection that walks every distinct
//! field in `entries` and labels each as ignored or tracked according to its
//! most recent state-bearing entry.
//!
//! ## Module layout
//!
//! The implementation is split across siblings inside `db/`. Each submodule
//! adds its own `impl Db { … }` block; this top-level file only carries the
//! `Db` struct, lifecycle (open / migrate / schema_version), and the
//! re-exports that make `datum_core::db::TimeMetric` etc. resolve at the
//! historical paths.

mod alias;
mod insert;
mod integrity;
mod kit;
mod kit_command;
mod list;
mod lookup;
mod multi;
mod now;
mod query;
mod row_mapper;
mod spec;
mod spec_alias_index;
mod state;
mod time;
mod tombstone;
mod types;
mod view;
mod zone_timeline;

use std::cell::RefCell;

use rusqlite::{params, Connection};

use crate::error::{DatumError, Result};

pub use crate::migrations::{MigrationReport, MAX_PRE_MIGRATION_BACKUPS};

pub use alias::{InstallAliasesReport, KitAliasOutcome};
pub use kit::{InstallReport, InstalledKit, InstalledKitIndex, KitFieldOutcome};
pub use kit_command::{
    validate_command, InstallCommandsReport, InstalledKitCommand, KitCommandOutcome,
};
pub use multi::merge_group_key;
pub use now::{
    NowFieldActive, NowFieldIgnored, NowFieldTrackedFalse, NowFilter, NowSnapshot,
    SCHEMA_ID as NOW_SCHEMA_ID,
};
pub use row_mapper::row_to_entry_public;
pub use types::{
    AliasConflict, AliasConflictReason, ConflictStrategy, GroupColumn, InsertOutcome, ListParams,
    RetimeFilter, Retimed, SaveSpecReport, SpecAliasCollision, SpecAliasIndex, TimeMetric,
    TimeOrderIssue, TrackedField, IGNORED_STATE_VALUE,
};
pub use view::{InstallViewsReport, InstalledView, KitViewOutcome, ViewResult};

// `try_insert` is a `pub(crate)` free function in `insert.rs`; re-export it
// at `crate::db::try_insert` to preserve the historical import path used by
// `crate::migrations`.
pub(crate) use insert::try_insert;

// Canonical state-participation predicate + ordering (da-ylet), shared by
// every state oracle inside and outside the db module (`crate::state`,
// `crate::integrity`).
pub(crate) use types::{
    STATE_ORDER_NEWEST_FIRST, STATE_ORDER_OLDEST_FIRST, STATE_PARTICIPATION_SQL,
};

pub struct Db {
    conn: Connection,
    /// Lazy cache of the spec-derived alias index. Built on first access
    /// after open or after invalidation. Mutating operations on
    /// `field_specs` clear it via [`Db::invalidate_spec_alias_index`].
    spec_alias_cache: RefCell<Option<SpecAliasIndex>>,
    /// What this open did to the schema, if anything. `None` when the DB
    /// was already current. datum-core never prints; the CLI reads this
    /// and tells the user (da-ksao).
    last_migration: Option<MigrationReport>,
}

impl Db {
    /// Configure a file-backed connection for concurrent use (da-11qq).
    ///
    /// datum's design makes concurrency the normal mode — the sync agent
    /// pulls and `--watch` loops read while the CLI writes — so every
    /// connection gets, explicitly and before any migration write:
    ///
    /// * `busy_timeout = 5000` — absorbs writer-vs-writer overlap instead
    ///   of surfacing SQLITE_BUSY. rusqlite happens to set this same 5s
    ///   default on open, but that is an internal detail of rusqlite —
    ///   datum's concurrency story shouldn't hinge on it.
    /// * `journal_mode = WAL` — readers never block the writer (in
    ///   rollback-journal mode a reader holding a transaction BUSYs any
    ///   writer). Persistent in the DB file, but set here rather than in
    ///   schema.sql so unmigrated opens and the migration walk itself run
    ///   under WAL too.
    /// * `synchronous = NORMAL` — the standard WAL pairing; fewer fsyncs
    ///   per commit with no integrity loss (WAL stays consistent on power
    ///   loss; at worst the last commit rolls back).
    ///
    /// All three pragmas cost microseconds — the CLI hot path stays
    /// instantaneous. (`execute_batch` because these pragmas return rows.)
    fn configure_connection(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "PRAGMA busy_timeout = 5000;\n\
             PRAGMA journal_mode = WAL;\n\
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(())
    }

    /// Open (or create) the database at the given path.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::configure_connection(&conn)?;
        let mut db = Db {
            conn,
            spec_alias_cache: RefCell::new(None),
            last_migration: None,
        };
        db.last_migration = db.migrate(Some(std::path::Path::new(path)))?;
        Ok(db)
    }

    /// Open the database without running migrations. Use only for
    /// destructive recovery paths (`datum db purge`) that will rebuild the
    /// schema from scratch — auto-migrate on a half-state DB can fail
    /// before the recovery code has a chance to run, leaving the user with
    /// no escape hatch.
    pub fn open_unmigrated(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::configure_connection(&conn)?;
        Ok(Db {
            conn,
            spec_alias_cache: RefCell::new(None),
            last_migration: None,
        })
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mut db = Db {
            conn,
            spec_alias_cache: RefCell::new(None),
            last_migration: None,
        };
        db.last_migration = db.migrate(None)?;
        Ok(db)
    }

    /// What the migration walk during [`Db::open`] did, or `None` if the
    /// database was already at the current schema version.
    pub fn last_migration(&self) -> Option<&MigrationReport> {
        self.last_migration.as_ref()
    }

    /// Bring the schema to [`crate::migrations::CURRENT_SCHEMA_VERSION`],
    /// snapshotting the file first when there is real work to do.
    ///
    /// `db_path` is `None` only for in-memory databases, which are always
    /// created fresh at the current version and so never migrate.
    fn migrate(&self, db_path: Option<&std::path::Path>) -> Result<Option<MigrationReport>> {
        // Reject pre-refactor entries tables (which have an `id` column):
        // this refactor drops ULID entirely and slug is now PK. Silent
        // migration would lose or re-key data unpredictably; the user must
        // delete the DB and re-create it.
        let legacy_entries_sql: String = self
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='entries'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();
        if !legacy_entries_sql.is_empty() {
            let upper = legacy_entries_sql.to_uppercase();
            // The pre-refactor schema named the ULID column `id` as the PK.
            // Our new schema has no such column.
            if upper.contains(" ID ") || upper.contains("\tID\t") || upper.contains("(ID ") {
                return Err(DatumError::IncompatibleSchema(
                    "legacy entries table with `id` column found; this version drops ULID \
                     and uses slug as PK. Delete the DB and recreate it, or migrate from a \
                     backup with `datum db import`."
                        .to_string(),
                ));
            }
        }

        // Stamp / advance the schema version. `infer_legacy_version`
        // short-circuits a fresh DB (no datum tables yet) to
        // CURRENT_SCHEMA_VERSION; legacy DBs walk their column-add probes
        // forward. Migrations add or rebuild the tables they need, so we
        // run them BEFORE schema.sql — no pre-flight ALTER hacks required.
        // Snapshot BEFORE the first write. A migration that runs without
        // its backup is the failure this guards against, so a backup
        // error aborts the open rather than being logged and ignored.
        let from = crate::migrations::peek_schema_version(&self.conn)?;
        let to = crate::migrations::CURRENT_SCHEMA_VERSION;
        let backup_path = match db_path {
            Some(path) if from < to => Some(crate::migrations::backup_before_migration(
                &self.conn, path, from, to,
            )?),
            _ => None,
        };

        let applied = crate::migrations::migrate_to_current(&self.conn)?;
        // schema.sql lands the final shape: any indexes / tables the
        // migration registry doesn't create (because they were always part
        // of schema.sql since they were introduced) are filled in here.
        // CREATE … IF NOT EXISTS keeps the pass idempotent on already-current
        // databases.
        self.conn.execute_batch(include_str!("../schema.sql"))?;

        if applied.is_empty() {
            return Ok(None);
        }
        // Retention runs only after the migration succeeded — we never
        // delete an old snapshot to make room for one that may be needed.
        let pruned = db_path
            .map(crate::migrations::prune_pre_migration_backups)
            .unwrap_or_default();
        Ok(Some(MigrationReport {
            from_version: from,
            to_version: to,
            backup_path,
            applied,
            pruned,
        }))
    }

    /// Borrow the underlying connection. Low-level — callers should prefer
    /// the typed API; this is intended for `datum db setup`, `datum info`,
    /// and other schema-level tooling.
    pub fn conn(&self) -> &rusqlite::Connection {
        &self.conn
    }

    /// Current stored schema version, read fresh from `_meta`.
    pub fn schema_version(&self) -> Result<u32> {
        match crate::migrations::read_schema_version(&self.conn)? {
            Some(v) => Ok(v),
            None => Ok(crate::migrations::CURRENT_SCHEMA_VERSION),
        }
    }

    /// Drop every datum table and rebuild the schema from scratch. Used by
    /// `datum db purge`. Caller is responsible for warning about data loss
    /// before invoking this.
    pub fn force_recreate_schema(&self) -> Result<Vec<String>> {
        let tables = [
            "entries",
            "entry_history",
            "field_specs",
            "kit_indices",
            "aliases",
            "sync_state",
            "sync_conflicts",
            "views",
            "kit_commands",
            "_meta",
        ];
        let mut dropped = Vec::new();
        for t in tables {
            let exists: bool = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    params![t],
                    |row| row.get::<_, i64>(0).map(|n| n > 0),
                )
                .unwrap_or(false);
            if exists {
                self.conn
                    .execute(&format!("DROP TABLE IF EXISTS {t}"), [])?;
                dropped.push(t.to_string());
            }
        }
        self.conn.execute_batch(include_str!("../schema.sql"))?;
        crate::migrations::set_schema_version(
            &self.conn,
            crate::migrations::CURRENT_SCHEMA_VERSION,
        )?;
        Ok(dropped)
    }
}
