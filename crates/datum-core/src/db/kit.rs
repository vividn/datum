//! Kit-scoped operations on field specs.
//!
//! A kit install writes a batch of [`FieldSpec`]s as `spec` entries whose
//! data carries `kit_id` = the kit's name (da-lc26c), as its layouts do
//! (`kit_layout.rs`). Subsequent `kit list` / `kit export` /
//! `kit uninstall` use that mark to find and remove the specs owned by
//! that kit. A user's own spec (no mark) is never touched by kit-scoped
//! operations.
//!
//! Not atomic across a kit's parts (da-lc26c): each spec is its own entry
//! write, as each layout is, and the indices commit in a transaction of
//! their own, so an index that fails to build leaves the specs installed.
//! Re-running the install is idempotent.
//!
//! Conflict policy on install: if a row already exists for the same field
//! name, the existing row wins by default (returned in `skipped`). When
//! the caller passes `force = true`, the kit's spec overwrites the row
//! and re-tags it with the kit's `kit_id`.

use rusqlite::params;
use serde_json::Value as JsonValue;

use crate::error::{DatumError, Result};
use crate::field_spec::FieldSpec;
use crate::kit::IndexEntry;

use super::spec::SPEC_KIT_KEY;
use super::Db;

/// The kit a spec entry's data names, if any.
fn spec_kit(data: &JsonValue) -> Option<&str> {
    data.get(SPEC_KIT_KEY)?.as_str()
}

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
            .filter(|&(_name, o)| matches!(o, KitFieldOutcome::Installed))
            .map(|(name, _o)| name.as_str())
    }

    pub fn skipped(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter(|&(_name, o)| matches!(o, KitFieldOutcome::SkippedExisting { .. }))
            .map(|(name, _o)| name.as_str())
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
    pub command_count: usize,
    pub layout_count: usize,
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
    /// - `force = false` → leave the existing row, record `SkippedExisting`
    ///   — unless it is the bare spec a first log writes
    ///   ([`FieldSpec::is_bare`]), which the kit replaces.
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
        let mut report = InstallReport::default();

        // Each spec is its own entry write, as a kit's layouts are.
        for spec in specs {
            let existing_kit: Option<Option<String>> = self
                .spec_entry(&spec.field)?
                .map(|e| e.data.as_ref().and_then(spec_kit).map(str::to_string));

            // The bare spec a field's first log wrote is no one's choice,
            // so it does not stand in a kit's way (da-1mkej).
            let bare = existing_kit == Some(None)
                && self.load_spec(&spec.field)?.is_some_and(|s| s.is_bare());
            if let Some(existing) = existing_kit.filter(|_| !bare) {
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
            self.write_spec_entry(spec, Some(kit_id))?;
            report
                .outcomes
                .push((spec.field.clone(), KitFieldOutcome::Installed));
        }

        let tx = self.conn.unchecked_transaction()?;
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

    /// Remove every field spec tagged with this `kit_id` — soft deletes,
    /// as `datum del` makes, so the removal travels like any deletion. A
    /// user's own spec is untouched. Returns the field names removed.
    pub fn uninstall_kit(&self, kit_id: &str) -> Result<Vec<String>> {
        let fields: Vec<String> = self
            .load_specs_by_kit(kit_id)?
            .into_iter()
            .map(|s| s.field)
            .collect();
        for field in &fields {
            self.delete_field_spec(field)?;
        }

        let tx = self.conn.unchecked_transaction()?;
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
    /// counts. User-declared rows (no `kit_id` in any of the spec
    /// or alias entries, `views`, or `kit_indices`) are excluded.
    /// A kit appears if it owns at least one row in any of those tables.
    pub fn list_kits(&self) -> Result<Vec<InstalledKit>> {
        let mut stmt = self.conn.prepare(
            "WITH
                -- Specs, aliases and layouts are ENTRIES (da-lc26c,
                -- da-b3752, da-gg0v): the
                -- kit that shipped one is a key in its data blob.
                kit_fields AS (
                    SELECT json_extract(data, '$.kit_id') AS kit_id, COUNT(*) AS n
                    FROM entries
                    WHERE field = 'spec'
                      AND deleted_at IS NULL
                      AND superseded_by IS NULL
                      AND json_extract(data, '$.kit_id') IS NOT NULL
                    GROUP BY 1
                ),
                kit_views AS (
                    SELECT kit_id, COUNT(*) AS n
                    FROM views
                    WHERE kit_id IS NOT NULL
                    GROUP BY kit_id
                ),
                kit_aliases AS (
                    SELECT json_extract(data, '$.kit_id') AS kit_id, COUNT(*) AS n
                    FROM entries
                    WHERE field = 'alias'
                      AND deleted_at IS NULL
                      AND superseded_by IS NULL
                      AND json_extract(data, '$.kit_id') IS NOT NULL
                    GROUP BY 1
                ),
                kit_idx AS (
                    SELECT kit_id, COUNT(*) AS n
                    FROM kit_indices
                    GROUP BY kit_id
                ),
                kit_cmds AS (
                    SELECT kit_id, COUNT(*) AS n
                    FROM kit_commands
                    WHERE kit_id IS NOT NULL
                    GROUP BY kit_id
                ),
                kit_layouts AS (
                    SELECT json_extract(data, '$.kit_id') AS kit_id, COUNT(*) AS n
                    FROM entries
                    WHERE field = 'layout'
                      AND deleted_at IS NULL
                      AND superseded_by IS NULL
                      AND json_extract(data, '$.kit_id') IS NOT NULL
                    GROUP BY 1
                )
             SELECT k.kit_id,
                    IFNULL(f.n, 0),
                    IFNULL(v.n, 0),
                    IFNULL(a.n, 0),
                    IFNULL(i.n, 0),
                    IFNULL(c.n, 0),
                    IFNULL(l.n, 0)
             FROM (
                 SELECT kit_id FROM kit_fields
                 UNION
                 SELECT kit_id FROM kit_views
                 UNION
                 SELECT kit_id FROM kit_aliases
                 UNION
                 SELECT kit_id FROM kit_idx
                 UNION
                 SELECT kit_id FROM kit_cmds
                 UNION
                 SELECT kit_id FROM kit_layouts
             ) k
             LEFT JOIN kit_fields  f ON f.kit_id = k.kit_id
             LEFT JOIN kit_views   v ON v.kit_id = k.kit_id
             LEFT JOIN kit_aliases a ON a.kit_id = k.kit_id
             LEFT JOIN kit_idx     i ON i.kit_id = k.kit_id
             LEFT JOIN kit_cmds    c ON c.kit_id = k.kit_id
             LEFT JOIN kit_layouts l ON l.kit_id = k.kit_id
             ORDER BY k.kit_id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(InstalledKit {
                kit_id: row.get::<_, String>(0)?,
                field_count: row.get::<_, i64>(1)? as usize,
                view_count: row.get::<_, i64>(2)? as usize,
                alias_count: row.get::<_, i64>(3)? as usize,
                index_count: row.get::<_, i64>(4)? as usize,
                command_count: row.get::<_, i64>(5)? as usize,
                layout_count: row.get::<_, i64>(6)? as usize,
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
        self.stored_specs_where(
            "AND json_extract(data, '$.kit_id') = ?1",
            "ORDER BY json_extract(data, '$.field')",
            params![kit_id],
        )
    }
}
