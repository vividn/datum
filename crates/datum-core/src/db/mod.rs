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
mod kit_command;
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

pub use alias::{InstallAliasesReport, KitAliasOutcome};
pub use kit::{InstallReport, InstalledKit, InstalledKitIndex, KitFieldOutcome};
pub use kit_command::{
    validate_command, InstallCommandsReport, InstalledKitCommand, KitCommandOutcome,
};
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

    /// Open the database without running migrations. Use only for
    /// destructive recovery paths (`datum db purge`) that will rebuild the
    /// schema from scratch — auto-migrate on a half-state DB can fail
    /// before the recovery code has a chance to run, leaving the user with
    /// no escape hatch.
    pub fn open_unmigrated(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Db { conn })
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
        // derived_from_field and kit_id columns before schema.sql tries to
        // index them. Missing-table errors on a fresh DB are ignored, as are
        // duplicate-column errors when one column already exists.
        let _ = self
            .conn
            .execute("ALTER TABLE aliases ADD COLUMN derived_from_field TEXT", []);
        let _ = self
            .conn
            .execute("ALTER TABLE aliases ADD COLUMN kit_id TEXT", []);

        // da-9u9.1: schema.sql's `CREATE INDEX … ON field_specs(kit_id)` and
        // `field_specs.tags` / `field_specs.render_json` references blow up
        // on a legacy v12-shape DB whose field_specs predates those columns.
        // The proper fix is the migration runner — but schema.sql executes
        // first and would error before `migrate_to_current` ever sees the
        // DB. Mirror the aliases-table escape hatch: idempotently add the
        // columns up front so schema.sql can compile. The migration steps
        // (m12_to_13 / m17_to_18 / m18_to_19) still run and remain the
        // version-stamping authority — these ALTERs are a parser unblock,
        // not a substitute for migrations. Errors are swallowed: a missing
        // table on a fresh DB and a duplicate column on an already-current
        // DB are both expected outcomes here.
        let _ = self
            .conn
            .execute("ALTER TABLE field_specs ADD COLUMN kit_id TEXT", []);
        let _ = self.conn.execute(
            "ALTER TABLE field_specs ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'",
            [],
        );
        let _ = self
            .conn
            .execute("ALTER TABLE field_specs ADD COLUMN render_json TEXT", []);

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

#[cfg(test)]
mod tests;
