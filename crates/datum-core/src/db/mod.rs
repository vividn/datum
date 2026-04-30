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
mod conflict;
mod insert;
mod kit;
mod list;
mod lookup;
mod query;
mod row_mapper;
mod spec;
mod state;
mod time;
mod tombstone;
mod types;
mod view;

use rusqlite::{params, Connection};

use crate::error::{DatumError, Result};

pub use kit::{InstallReport, InstalledKit, InstalledKitIndex, KitFieldOutcome};
pub use row_mapper::row_to_entry_public;
pub use types::{
    AliasConflict, AliasConflictReason, ConflictStrategy, GroupColumn, InsertOutcome, ListParams,
    RetimeFilter, SaveSpecReport, TimeMetric, TimeOrderIssue, TrackedField, IGNORED_STATE_VALUE,
};
pub use view::{InstallViewsReport, InstalledView, KitViewOutcome, ViewResult};

// `try_insert` is a `pub(crate)` free function in `insert.rs`; re-export it
// at `crate::db::try_insert` to preserve the historical import path used by
// `crate::migrations`.
pub(crate) use insert::try_insert;

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (or create) the database at the given path.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        // Pre-a0 aliases table only carried (name, alias_json); add the
        // derived_from_field column before schema.sql tries to index it.
        // Missing-table errors on a fresh DB are ignored.
        let _ = self
            .conn
            .execute("ALTER TABLE aliases ADD COLUMN derived_from_field TEXT", []);

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
                     backup with `datum migrate`."
                        .to_string(),
                ));
            }
        }

        self.conn.execute_batch(include_str!("../schema.sql"))?;
        // Stamp / advance the schema version. Forward-compat migrations
        // apply silently; incompatible versions return an error here.
        crate::migrations::migrate_to_current(&self.conn)?;
        // Migrations may drop tables destructively (e.g. v2→v3 replaces the
        // entries table). Re-run schema.sql so any dropped tables come back
        // in their latest shape. IF NOT EXISTS keeps this idempotent on
        // fresh databases.
        self.conn.execute_batch(include_str!("../schema.sql"))?;
        Ok(())
    }

    /// Borrow the underlying connection. Low-level — callers should prefer
    /// the typed API; this is intended for `datum setup`, `datum info`,
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
    /// `datum setup --force-recreate`. Caller is responsible for warning
    /// about data loss before invoking this.
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

#[cfg(test)]
mod tests;
