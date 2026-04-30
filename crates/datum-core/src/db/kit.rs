//! Kit-scoped operations on `field_specs`.
//!
//! A kit install writes a batch of [`FieldSpec`] rows with their `kit_id`
//! column set to the kit's name. Subsequent `kit list` / `kit export` /
//! `kit uninstall` use that column to find and remove the rows owned by
//! that kit. User-declared rows (`kit_id IS NULL`) are never touched by
//! kit-scoped operations.
//!
//! Conflict policy on install: if a row already exists for the same field
//! name, the existing row wins by default (returned in `skipped`). When
//! the caller passes `force = true`, the kit's spec overwrites the row
//! and re-tags it with the kit's `kit_id`.

use rusqlite::{params, OptionalExtension};

use crate::error::{DatumError, Result};
use crate::field_spec::{Alias, FieldSpec};
use crate::kit::IndexEntry;

use super::Db;

/// Per-field outcome reported by [`Db::install_kit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitFieldOutcome {
    /// Field newly installed (or overwritten under `force`).
    Installed,
    /// A row for this field already existed and the kit's spec was
    /// dropped silently; the existing row remains the source of truth.
    SkippedExisting {
        /// Name of the kit that currently owns the row, or `None` if it's
        /// a user-declared row.
        existing_kit: Option<String>,
    },
}

/// Aggregate report from [`Db::install_kit`].
#[derive(Debug, Clone, Default)]
pub struct InstallReport {
    /// Field name → outcome.
    pub outcomes: Vec<(String, KitFieldOutcome)>,
    /// Index names (TOML-side, not the on-disk SQLite name) that were
    /// created (or recreated) by this install.
    pub indices_installed: Vec<String>,
    /// On-disk SQLite names of indices dropped by this install — only
    /// non-empty on a re-install where the kit previously owned indices
    /// that are now gone or replaced.
    pub indices_dropped: Vec<String>,
}

impl InstallReport {
    pub fn installed(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter_map(|(name, o)| matches!(o, KitFieldOutcome::Installed).then(|| name.as_str()))
    }

    pub fn skipped(&self) -> impl Iterator<Item = &str> {
        self.outcomes.iter().filter_map(|(name, o)| {
            matches!(o, KitFieldOutcome::SkippedExisting { .. }).then(|| name.as_str())
        })
    }
}

/// One row of [`Db::list_kits`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledKit {
    pub kit_id: String,
    pub field_count: usize,
    pub view_count: usize,
    pub alias_count: usize,
    pub index_count: usize,
}

/// One row from `kit_indices`, returned by [`Db::load_indices_by_kit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledKitIndex {
    pub kit_id: String,
    pub index_name: String,
    pub on_disk_name: String,
    pub sql_text: String,
}

impl Db {
    /// Install a kit's field specs. `kit_id` tags every persisted row;
    /// callers should pass the kit's `[kit].name` from the TOML.
    ///
    /// On collision (a row already exists for `spec.field`):
    /// - `force = false` → leave the existing row, record `SkippedExisting`.
    /// - `force = true`  → overwrite the row's spec_json and re-tag its
    ///   `kit_id` to the new owner.
    ///
    /// Returns an [`InstallReport`] so the dispatcher can render
    /// install/skip lines to the user.
    pub fn install_kit(
        &self,
        kit_id: &str,
        specs: &[FieldSpec],
        indices: &[IndexEntry],
        force: bool,
    ) -> Result<InstallReport> {
        let tx = self.conn.unchecked_transaction()?;
        let mut report = InstallReport::default();

        for spec in specs {
            let existing_kit: Option<Option<String>> = tx
                .query_row(
                    "SELECT kit_id FROM field_specs WHERE field = ?1",
                    params![spec.field],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?;

            if let Some(existing) = existing_kit {
                if !force {
                    report.outcomes.push((
                        spec.field.clone(),
                        KitFieldOutcome::SkippedExisting {
                            existing_kit: existing,
                        },
                    ));
                    continue;
                }
            }

            let json = serde_json::to_string(spec)?;
            tx.execute(
                "INSERT INTO field_specs (field, spec_json, kit_id) VALUES (?1, ?2, ?3)
                 ON CONFLICT(field) DO UPDATE SET spec_json = excluded.spec_json, kit_id = excluded.kit_id",
                params![spec.field, json, kit_id],
            )?;

            // Replace derived aliases for this field. Mirrors save_field_spec's
            // alias-sync but inline so the whole kit install lives in one tx.
            tx.execute(
                "DELETE FROM aliases WHERE derived_from_field = ?1",
                params![spec.field],
            )?;
            for d in &spec.derived_aliases {
                let already_owned: Option<Option<String>> = tx
                    .query_row(
                        "SELECT derived_from_field FROM aliases WHERE name = ?1",
                        params![&d.state],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .optional()?;
                if let Some(Some(owner)) = already_owned {
                    if owner != spec.field {
                        // State alias collision with another spec — first
                        // owner wins. Bare-field/terminal-end aliases were
                        // already cleared by the DELETE above (or owned by
                        // this same field; the same-field branch falls
                        // through to the upsert below).
                        continue;
                    }
                } else if let Some(None) = already_owned {
                    // User-defined alias under this name — do not overwrite.
                    continue;
                }
                let alias = Alias {
                    name: d.state.clone(),
                    expands_to: d.expands_to.clone(),
                    description: Some(format!("Derived from {} spec", spec.field)),
                };
                let alias_json = serde_json::to_string(&alias)?;
                tx.execute(
                    "INSERT INTO aliases (name, alias_json, derived_from_field) VALUES (?1, ?2, ?3)
                     ON CONFLICT(name) DO UPDATE SET alias_json = excluded.alias_json,
                                                     derived_from_field = excluded.derived_from_field",
                    params![alias.name, alias_json, spec.field],
                )?;
            }

            report
                .outcomes
                .push((spec.field.clone(), KitFieldOutcome::Installed));
        }

        // Indices: drop everything previously owned by this kit, then
        // (re)create from the new TOML. Re-install is idempotent because
        // the DDL uses CREATE INDEX IF NOT EXISTS but we drop-first to
        // ensure obsolete indices don't survive a TOML edit.
        let mut prev_stmt = tx.prepare(
            "SELECT on_disk_name FROM kit_indices WHERE kit_id = ?1 ORDER BY index_name",
        )?;
        let prev_names: Vec<String> = prev_stmt
            .query_map(params![kit_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatumError::Db)?;
        drop(prev_stmt);
        for name in &prev_names {
            tx.execute_batch(&format!("DROP INDEX IF EXISTS \"{name}\""))?;
        }
        tx.execute("DELETE FROM kit_indices WHERE kit_id = ?1", params![kit_id])?;
        report.indices_dropped = prev_names;

        for entry in indices {
            let ddl = entry.build_ddl(kit_id)?;
            let on_disk = entry.sqlite_name(kit_id);
            tx.execute_batch(&ddl)?;
            tx.execute(
                "INSERT INTO kit_indices (kit_id, index_name, on_disk_name, sql_text)
                 VALUES (?1, ?2, ?3, ?4)",
                params![kit_id, entry.name, on_disk, ddl],
            )?;
            report.indices_installed.push(entry.name.clone());
        }

        tx.commit()?;
        Ok(report)
    }

    /// Remove every field spec (and its derived aliases) tagged with this
    /// `kit_id`. User-declared rows (kit_id IS NULL) are untouched. Returns
    /// the list of field names removed.
    pub fn uninstall_kit(&self, kit_id: &str) -> Result<Vec<String>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt =
            tx.prepare("SELECT field FROM field_specs WHERE kit_id = ?1 ORDER BY field")?;
        let fields: Vec<String> = stmt
            .query_map(params![kit_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatumError::Db)?;
        drop(stmt);

        for f in &fields {
            tx.execute(
                "DELETE FROM aliases WHERE derived_from_field = ?1",
                params![f],
            )?;
        }
        tx.execute("DELETE FROM field_specs WHERE kit_id = ?1", params![kit_id])?;

        // Drop installed indices owned by this kit. Index drops are
        // unconditional — the underlying data is fine, and the design
        // explicitly declines to gate uninstall on a trust prompt.
        let mut idx_stmt = tx.prepare("SELECT on_disk_name FROM kit_indices WHERE kit_id = ?1")?;
        let idx_names: Vec<String> = idx_stmt
            .query_map(params![kit_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatumError::Db)?;
        drop(idx_stmt);
        for name in &idx_names {
            tx.execute_batch(&format!("DROP INDEX IF EXISTS \"{name}\""))?;
        }
        tx.execute("DELETE FROM kit_indices WHERE kit_id = ?1", params![kit_id])?;

        tx.commit()?;
        Ok(fields)
    }

    /// List installed kits with per-kit field, view, alias, and index
    /// counts. User-declared rows (`kit_id IS NULL` in any of
    /// `field_specs`, `views`, `aliases`, or `kit_indices`) are excluded.
    /// A kit appears if it owns at least one row in any of those tables.
    pub fn list_kits(&self) -> Result<Vec<InstalledKit>> {
        let mut stmt = self.conn.prepare(
            "WITH
                kit_fields AS (
                    SELECT kit_id, COUNT(*) AS n
                    FROM field_specs
                    WHERE kit_id IS NOT NULL
                    GROUP BY kit_id
                ),
                kit_views AS (
                    SELECT kit_id, COUNT(*) AS n
                    FROM views
                    WHERE kit_id IS NOT NULL
                    GROUP BY kit_id
                ),
                kit_aliases AS (
                    SELECT kit_id, COUNT(*) AS n
                    FROM aliases
                    WHERE kit_id IS NOT NULL
                    GROUP BY kit_id
                ),
                kit_idx AS (
                    SELECT kit_id, COUNT(*) AS n
                    FROM kit_indices
                    GROUP BY kit_id
                )
             SELECT k.kit_id,
                    IFNULL(f.n, 0),
                    IFNULL(v.n, 0),
                    IFNULL(a.n, 0),
                    IFNULL(i.n, 0)
             FROM (
                 SELECT kit_id FROM kit_fields
                 UNION
                 SELECT kit_id FROM kit_views
                 UNION
                 SELECT kit_id FROM kit_aliases
                 UNION
                 SELECT kit_id FROM kit_idx
             ) k
             LEFT JOIN kit_fields  f ON f.kit_id = k.kit_id
             LEFT JOIN kit_views   v ON v.kit_id = k.kit_id
             LEFT JOIN kit_aliases a ON a.kit_id = k.kit_id
             LEFT JOIN kit_idx     i ON i.kit_id = k.kit_id
             ORDER BY k.kit_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(InstalledKit {
                kit_id: row.get::<_, String>(0)?,
                field_count: row.get::<_, i64>(1)? as usize,
                view_count: row.get::<_, i64>(2)? as usize,
                alias_count: row.get::<_, i64>(3)? as usize,
                index_count: row.get::<_, i64>(4)? as usize,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(DatumError::Db)?);
        }
        Ok(out)
    }

    /// Load every kit-owned index entry tagged with `kit_id`. Used by
    /// `datum kit export` to round-trip the `[[index]]` blocks back to
    /// TOML. Rows are returned ordered by `index_name` for stable
    /// output.
    pub fn load_indices_by_kit(&self, kit_id: &str) -> Result<Vec<InstalledKitIndex>> {
        let mut stmt = self.conn.prepare(
            "SELECT kit_id, index_name, on_disk_name, sql_text
             FROM kit_indices
             WHERE kit_id = ?1
             ORDER BY index_name",
        )?;
        let rows = stmt.query_map(params![kit_id], |row| {
            Ok(InstalledKitIndex {
                kit_id: row.get::<_, String>(0)?,
                index_name: row.get::<_, String>(1)?,
                on_disk_name: row.get::<_, String>(2)?,
                sql_text: row.get::<_, String>(3)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(DatumError::Db)?);
        }
        Ok(out)
    }

    /// Load every [`FieldSpec`] tagged with the given `kit_id`. Used by
    /// `datum kit export`. Returns rows ordered by field name for stable
    /// TOML output.
    pub fn load_specs_by_kit(&self, kit_id: &str) -> Result<Vec<FieldSpec>> {
        let mut stmt = self
            .conn
            .prepare("SELECT spec_json FROM field_specs WHERE kit_id = ?1 ORDER BY field")?;
        let rows = stmt.query_map(params![kit_id], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            let json = r.map_err(DatumError::Db)?;
            let spec: FieldSpec = serde_json::from_str(&json)?;
            out.push(spec);
        }
        Ok(out)
    }
}
