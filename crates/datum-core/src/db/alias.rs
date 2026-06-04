//! User aliases: `lookup_alias`, `delete_alias`, `save_alias`.
//!
//! Spec-derived aliases are *not* stored as rows here. Each
//! [`crate::field_spec::FieldSpec`] carries its own `derived_aliases`
//! list inside `spec_json`; reads consult the lazy
//! [`super::types::SpecAliasIndex`] (built by
//! [`super::Db::spec_alias_index`]). User-declared and kit-installed
//! aliases shadow spec-derived names — see [`Db::lookup_alias`] (da-8y2q.5).
//!
//! Kit-scoped aliases (slice 3, da-d4t.3) live alongside user-declared
//! ones in the same `aliases` table. The `kit_id` column tags rows that
//! came from a `[[alias]]` block in a kit TOML; `install_kit_aliases` /
//! `uninstall_kit_aliases` / `load_aliases_by_kit` operate on those rows
//! and leave `kit_id IS NULL` rows alone (mirroring the field-spec /
//! view kit pattern).

use rusqlite::{params, OptionalExtension};

use crate::error::{DatumError, Result};
use crate::field_spec::Alias;
use crate::kit::AliasEntry;

use super::Db;

/// Per-alias outcome reported by [`Db::install_kit_aliases`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitAliasOutcome {
    /// Alias newly installed (or overwritten under `force`).
    Installed,
    /// A row for this alias name already existed and the kit's row was
    /// dropped silently; the existing row remains the source of truth.
    SkippedExisting {
        /// Name of the kit that currently owns the row, or `None` if it's
        /// a user-declared row.
        existing_kit: Option<String>,
    },
}

/// Aggregate report from [`Db::install_kit_aliases`].
#[derive(Debug, Clone, Default)]
pub struct InstallAliasesReport {
    pub outcomes: Vec<(String, KitAliasOutcome)>,
}

impl InstallAliasesReport {
    pub fn installed(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter(|&(_name, o)| matches!(o, KitAliasOutcome::Installed))
            .map(|(name, _o)| name.as_str())
    }

    pub fn skipped(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter(|&(_name, o)| matches!(o, KitAliasOutcome::SkippedExisting { .. }))
            .map(|(name, _o)| name.as_str())
    }
}

impl Db {
    /// Look up a single alias by name. User/kit aliases (rows in the
    /// `aliases` table) shadow spec-derived aliases; on a miss, falls
    /// back to the [`super::types::SpecAliasIndex`].
    pub fn lookup_alias(&self, name: &str) -> Result<Option<Alias>> {
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT alias_json FROM aliases WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(json) = result {
            return Ok(Some(serde_json::from_str(&json)?));
        }
        // Fall through to the spec-alias index (da-8y2q.5).
        let index = self.spec_alias_index()?;
        Ok(index.entries.get(name).cloned())
    }

    /// Delete a user-created alias row. Returns whether a row was
    /// actually deleted, so callers can report honestly instead of
    /// hedging. Spec-derived aliases live in `field_specs.spec_json`,
    /// not in this table, so they cannot be deleted via this entrypoint.
    pub fn delete_alias(&self, name: &str) -> Result<bool> {
        let deleted = self
            .conn
            .execute("DELETE FROM aliases WHERE name = ?1", params![name])?;
        Ok(deleted > 0)
    }

    /// Save a user-created alias (kit_id = NULL).
    pub fn save_alias(&self, alias: &Alias) -> Result<()> {
        let json = serde_json::to_string(alias)?;
        self.conn.execute(
            "INSERT INTO aliases (name, alias_json, kit_id)
             VALUES (?1, ?2, NULL)
             ON CONFLICT(name) DO UPDATE SET alias_json = excluded.alias_json,
                                             kit_id = NULL",
            params![alias.name, json],
        )?;
        Ok(())
    }

    /// Install a kit's aliases. `kit_id` tags every persisted row; callers
    /// should pass the kit's `[kit].name` from the TOML.
    ///
    /// Conflict policy mirrors [`Db::install_kit`] / [`Db::install_kit_views`]:
    /// - `force = false` → leave any existing row, record `SkippedExisting`.
    ///   This protects user-declared aliases (`kit_id IS NULL`) and
    ///   aliases owned by another kit alike — both are reported as a skip
    ///   carrying the existing owner.
    /// - `force = true`  → overwrite the row's `alias_json` and re-tag its
    ///   `kit_id` to the new owner.
    pub fn install_kit_aliases(
        &self,
        kit_id: &str,
        aliases: &[AliasEntry],
        force: bool,
    ) -> Result<InstallAliasesReport> {
        let tx = self.conn.unchecked_transaction()?;
        let mut report = InstallAliasesReport::default();

        for entry in aliases {
            let existing_kit: Option<Option<String>> = tx
                .query_row(
                    "SELECT kit_id FROM aliases WHERE name = ?1",
                    params![entry.name],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?;

            if let Some(existing) = existing_kit {
                if !force {
                    report.outcomes.push((
                        entry.name.clone(),
                        KitAliasOutcome::SkippedExisting {
                            existing_kit: existing,
                        },
                    ));
                    continue;
                }
            }

            let alias = entry.clone().into_alias();
            let json = serde_json::to_string(&alias)?;
            tx.execute(
                "INSERT INTO aliases (name, alias_json, kit_id)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(name) DO UPDATE SET alias_json = excluded.alias_json,
                                                 kit_id     = excluded.kit_id",
                params![alias.name, json, kit_id],
            )?;

            report
                .outcomes
                .push((entry.name.clone(), KitAliasOutcome::Installed));
        }

        tx.commit()?;
        Ok(report)
    }

    /// Drop every alias tagged with this `kit_id`. User-declared rows
    /// (`kit_id IS NULL`) are untouched. Returns the names removed.
    pub fn uninstall_kit_aliases(&self, kit_id: &str) -> Result<Vec<String>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = tx.prepare("SELECT name FROM aliases WHERE kit_id = ?1 ORDER BY name")?;
        let names: Vec<String> = stmt
            .query_map(params![kit_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatumError::Db)?;
        drop(stmt);
        tx.execute("DELETE FROM aliases WHERE kit_id = ?1", params![kit_id])?;
        tx.commit()?;
        Ok(names)
    }

    /// Load every user/kit alias row from the `aliases` table. Spec-derived
    /// aliases are NOT included — they live in `field_specs.spec_json` and
    /// are accessible via [`Db::spec_alias_index`]. Used by `dm check
    /// --aliases` to walk every user-defined chain for orphan and cycle
    /// detection. Ordered by name for stable output.
    pub fn list_user_aliases(&self) -> Result<Vec<Alias>> {
        let mut stmt = self
            .conn
            .prepare("SELECT alias_json FROM aliases ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            let json = r.map_err(DatumError::Db)?;
            let alias: Alias = serde_json::from_str(&json)?;
            out.push(alias);
        }
        Ok(out)
    }

    /// Load every [`Alias`] tagged with the given `kit_id`. Used by
    /// `datum kit export`. Returns rows ordered by name for stable TOML
    /// output.
    pub fn load_aliases_by_kit(&self, kit_id: &str) -> Result<Vec<Alias>> {
        let mut stmt = self
            .conn
            .prepare("SELECT alias_json FROM aliases WHERE kit_id = ?1 ORDER BY name")?;
        let rows = stmt.query_map(params![kit_id], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            let json = r.map_err(DatumError::Db)?;
            let alias: Alias = serde_json::from_str(&json)?;
            out.push(alias);
        }
        Ok(out)
    }
}
