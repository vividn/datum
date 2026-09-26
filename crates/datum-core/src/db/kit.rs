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
//! write, as each layout and each index definition is (da-fwwtn), so an
//! index that fails to build leaves the specs installed. Re-running the
//! install is idempotent.
//!
//! Conflict policy on install: if a row already exists for the same field
//! name, the existing row wins by default (returned in `skipped`). When
//! the caller passes `force = true`, the kit's spec overwrites the row
//! and re-tags it with the kit's `kit_id`, keeping the spec it displaced
//! for `kit uninstall` to write back (da-uokdb).

use rusqlite::params;
use serde_json::Value as JsonValue;

use crate::error::{DatumError, Result};
use crate::field_spec::FieldSpec;
use crate::kit::IndexEntry;

use super::definition::{definition_data, INDEX_FIELD, KIT_KEY};
use super::spec::{SPEC_DISPLACED_KEY, SPEC_KIT_KEY};
use super::types::LIVE_SQL;
use super::Db;

/// The kit a spec entry's data names, if any.
fn spec_kit(data: &JsonValue) -> Option<&str> {
    data.get(SPEC_KIT_KEY)?.as_str()
}

/// Spec entry `data` with every layer `kit_id` wrote taken out: a layer of
/// the kit's gives way to the spec it displaced, or to none (da-uokdb).
fn without_kit(data: &JsonValue, kit_id: &str) -> Option<JsonValue> {
    let mut data = data.clone();
    let obj = data.as_object_mut()?;
    let inner = obj
        .remove(SPEC_DISPLACED_KEY)
        .and_then(|d| without_kit(&d, kit_id));
    if let Some(inner) = inner {
        if obj.get(SPEC_KIT_KEY).and_then(JsonValue::as_str) == Some(kit_id) {
            return Some(inner);
        }
        obj.insert(SPEC_DISPLACED_KEY.into(), inner);
    }
    (spec_kit(&data) != Some(kit_id)).then_some(data)
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstalledKit {
    pub kit_id: String,
    pub field_count: usize,
    pub view_count: usize,
    pub alias_count: usize,
    pub index_count: usize,
    pub command_count: usize,
    pub layout_count: usize,
}

/// One kit index definition, returned by [`Db::load_indices_by_kit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledKitIndex {
    pub kit_id: String,
    pub index_name: String,
    pub on_disk_name: String,
    pub sql_text: String,
    /// The definition itself, as the kit's TOML declared it.
    pub entry: IndexEntry,
}

/// `index:<kit>/<name>` — an index definition's slug (`definition.rs`).
fn index_slug(kit_id: &str, name: &str) -> String {
    format!("{INDEX_FIELD}:{kit_id}/{name}")
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

        // Indices: this kit's definitions become exactly the TOML's, and
        // the device's physical indices follow them (da-fwwtn).
        let before = self.load_indices_by_kit(kit_id)?;
        let mut wanted = Vec::with_capacity(indices.len());
        for entry in indices {
            let ddl = entry.build_ddl(kit_id)?;
            wanted.push(entry.sqlite_name(kit_id));
            self.write_definition(INDEX_FIELD, definition_data(entry, Some(kit_id))?)?;
            report.indices_installed.push(entry.name.clone());
            if let Some(old) = before.iter().find(|b| b.index_name == entry.name) {
                if old.sql_text != ddl {
                    report.indices_dropped.push(old.on_disk_name.clone());
                }
            }
        }
        for old in before.iter().filter(|b| !wanted.contains(&b.on_disk_name)) {
            self.soft_delete_entry(&index_slug(kit_id, &old.index_name))?;
            report.indices_dropped.push(old.on_disk_name.clone());
        }
        // An index that will not build on this data (a UNIQUE one over
        // duplicates) fails the install, and the kit's indices go back to
        // what they were — as the one transaction this replaced left them.
        let failed: Vec<_> = self
            .derive_kit_indices()?
            .into_iter()
            .filter(|(name, _)| wanted.contains(name))
            .collect();
        if let Some((_, err)) = failed.first() {
            for entry in indices {
                if !before.iter().any(|b| b.index_name == entry.name) {
                    self.soft_delete_entry(&index_slug(kit_id, &entry.name))?;
                }
            }
            for old in &before {
                self.write_definition(INDEX_FIELD, definition_data(&old.entry, Some(kit_id))?)?;
            }
            self.derive_kit_indices()?;
            return Err(DatumError::Other(format!("kit '{kit_id}': index: {err}")));
        }
        Ok(report)
    }

    /// Remove every field spec tagged with this `kit_id` — soft deletes,
    /// as `datum del` makes, so the removal travels like any deletion —
    /// or, where the kit displaced a spec, write that spec back
    /// (da-uokdb). A spec another kit displaced in turn loses the kit's
    /// layer too, so uninstalling that kit later cannot bring this one
    /// back. A user's own spec is untouched. Returns the field names
    /// removed.
    pub fn uninstall_kit(&self, kit_id: &str) -> Result<Vec<String>> {
        let fields: Vec<String> = self
            .load_specs_by_kit(kit_id)?
            .into_iter()
            .map(|s| s.field)
            .collect();
        let mut stmt = self.conn.prepare(&format!(
            "SELECT json_extract(data, '$.field') FROM entries
             WHERE field = 'spec' AND {LIVE_SQL}
               AND json_extract(data, '$.displaced') IS NOT NULL"
        ))?;
        let displacing: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        for field in fields.iter().chain(&displacing) {
            let Some(mut row) = self.spec_entry(field)? else {
                continue;
            };
            let Some(data) = row.data.clone() else {
                continue;
            };
            match without_kit(&data, kit_id) {
                None => self.soft_delete_entry(&row.hid)?,
                Some(kept) if kept != data => {
                    row.data = Some(kept);
                    self.save_existing_entry(&row)?;
                }
                Some(_) => {}
            }
        }

        // Index drops are unconditional — the underlying data is fine, and
        // the design explicitly declines to gate uninstall on a trust prompt.
        self.uninstall_definitions(INDEX_FIELD, kit_id)?;
        self.derive_kit_indices()?;
        Ok(fields)
    }

    /// List installed kits with a count of each kind of thing they own. A
    /// kit's specs, views, aliases, indices, commands and layouts are all
    /// entries whose data names it (da-lc26c, da-b3752, da-gg0v,
    /// da-fwwtn); the person's own carry no mark and are no kit's.
    pub fn list_kits(&self) -> Result<Vec<InstalledKit>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT json_extract(data, '$.kit_id') AS kit, field, COUNT(*)
             FROM entries
             WHERE field IN ('spec', 'view', 'alias', 'index', 'command', 'layout')
               AND {LIVE_SQL} AND kit IS NOT NULL
             GROUP BY kit, field
             ORDER BY kit"
        ))?;
        let mut rows = stmt.query([])?;
        let mut out: Vec<InstalledKit> = Vec::new();
        while let Some(row) = rows.next()? {
            let kit_id: String = row.get(0)?;
            let field: String = row.get(1)?;
            let n = row.get::<_, i64>(2)? as usize;
            if out.last().is_none_or(|k| k.kit_id != kit_id) {
                out.push(InstalledKit {
                    kit_id,
                    ..InstalledKit::default()
                });
            }
            let kit = out.last_mut().expect("pushed above");
            *match field.as_str() {
                "spec" => &mut kit.field_count,
                "view" => &mut kit.view_count,
                "alias" => &mut kit.alias_count,
                "index" => &mut kit.index_count,
                "command" => &mut kit.command_count,
                _ => &mut kit.layout_count,
            } = n;
        }
        Ok(out)
    }

    /// The physical SQLite indices this device's kit index definitions
    /// describe, made so (da-fwwtn): a declared one that is missing — or
    /// built from another definition — is (re)built, one no definition
    /// declares any more is dropped. Only the definition travels, so this
    /// runs whenever definitions may have changed: on open, after a sync or
    /// a restore, and on a kit install or uninstall.
    ///
    /// Each index's DDL is rebuilt from its definition by
    /// [`IndexEntry::build_ddl`], whose validation is the one a kit install
    /// runs, so a definition that arrived by sync is held to the same rule.
    /// One that fails that, or fails to build on this data (a UNIQUE index
    /// over duplicates), is returned as `(sqlite name, error)` rather than
    /// failing the rest — an index is this device's, not the data's.
    pub fn derive_kit_indices(&self) -> Result<Vec<(String, DatumError)>> {
        let mut failed = Vec::new();
        let mut declared: Vec<(String, String)> = Vec::new();
        for data in self.definitions(INDEX_FIELD, None)? {
            let kit = data[KIT_KEY].as_str().unwrap_or_default().to_string();
            let slug = index_slug(&kit, data["name"].as_str().unwrap_or_default());
            let built = serde_json::from_value::<IndexEntry>(data)
                .map_err(DatumError::from)
                .and_then(|ix| Ok((ix.sqlite_name(&kit), ix.build_ddl(&kit)?)));
            match built {
                Ok(pair) => declared.push(pair),
                Err(e) => failed.push((slug, e)),
            }
        }
        // SQLite keeps a statement's text minus its IF NOT EXISTS.
        let stored = |ddl: &str| ddl.replacen(" IF NOT EXISTS", "", 1);
        // `_` is a LIKE wildcard; `!_` is the underscore itself.
        let mut stmt = self.conn.prepare(
            "SELECT name, sql FROM sqlite_master
             WHERE type = 'index' AND tbl_name = 'entries' AND name LIKE 'kit!_%' ESCAPE '!'",
        )?;
        let existing: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        for (name, sql) in &existing {
            if !declared
                .iter()
                .any(|(n, ddl)| n == name && stored(ddl) == *sql)
            {
                self.conn
                    .execute_batch(&format!("DROP INDEX IF EXISTS \"{name}\""))?;
            }
        }
        for (name, ddl) in declared {
            if !existing
                .iter()
                .any(|(n, sql)| *n == name && stored(&ddl) == *sql)
            {
                if let Err(e) = self.conn.execute_batch(&ddl) {
                    failed.push((name, e.into()));
                }
            }
        }
        Ok(failed)
    }

    /// Every index definition marked with `kit_id`, by name — `datum kit
    /// export`'s source, with the name and DDL it builds on this device.
    /// One that will not build (edited or synced past the install's screen)
    /// is left out, as [`Db::derive_kit_indices`] leaves it unbuilt, so it
    /// cannot stop the kit being exported or uninstalled.
    pub fn load_indices_by_kit(&self, kit_id: &str) -> Result<Vec<InstalledKitIndex>> {
        Ok(self
            .definitions(INDEX_FIELD, Some(kit_id))?
            .into_iter()
            .filter_map(|data| {
                let entry: IndexEntry = serde_json::from_value(data).ok()?;
                Some(InstalledKitIndex {
                    kit_id: kit_id.to_string(),
                    index_name: entry.name.clone(),
                    on_disk_name: entry.sqlite_name(kit_id),
                    sql_text: entry.build_ddl(kit_id).ok()?,
                    entry,
                })
            })
            .collect())
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
