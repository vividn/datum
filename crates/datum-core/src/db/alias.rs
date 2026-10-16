//! User and kit aliases are ENTRIES (da-b3752), following specs (da-lc26c,
//! `spec.rs`): an alias is the live entry of field `alias` at slug
//! `alias:<name>`, its data the [`Alias`] plus the `kit_id` of the kit that
//! shipped it. The entries table is the one source of truth, so an alias
//! backs up, syncs and has a history like any doc, and `update` / `undo` /
//! `history alias:vbp` work on it.
//!
//! Spec-derived aliases are *not* entries of their own. Each
//! [`crate::field_spec::FieldSpec`] carries its `derived_aliases`; reads
//! consult the lazy [`super::types::SpecAliasIndex`]. User and kit aliases
//! shadow spec-derived names — see [`Db::lookup_alias`] (da-8y2q.5).
//!
//! A kit's alias carries its `kit_id`; `install_kit_aliases` /
//! `uninstall_kit_aliases` / `load_aliases_by_kit` act on those and leave
//! the person's own (no mark) alone.

use rusqlite::{params, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::dur::Dur;
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::field_spec::Alias;
use crate::ids::{Hid, Slug};
use crate::kit::AliasEntry;
use crate::verb::Verb;

use super::types::LIVE_SQL;
use super::Db;

/// The field every alias entry is logged under.
pub const ALIAS_FIELD: &str = "alias";

/// The data key naming the kit that shipped an alias, as for specs.
const ALIAS_KIT_KEY: &str = "kit_id";

/// The rows the readers take for aliases: a live `alias` entry at the slug
/// its own data names (a synced plain log of `alias` is not one).
const ALIAS_DOC_SQL: &str = "field = 'alias' AND deleted_at IS NULL AND superseded_by IS NULL \
     AND slug = 'alias:' || json_extract(data, '$.name')";

/// `alias:<name>` — an alias's slug, so the same alias is the same doc on
/// every device.
fn alias_slug(name: &str) -> String {
    format!("{ALIAS_FIELD}:{name}")
}

/// A new alias entry for `data` (the alias's JSON, with its kit mark).
pub(crate) fn new_alias_entry(data: JsonValue, client_id: &str) -> Entry {
    let name = data["name"].as_str().unwrap_or_default().to_string();
    let mut row = Entry::new(
        ALIAS_FIELD,
        Verb::Point,
        None,
        Some(data),
        Some(super::spec::meta_occur_time()),
        Some(Dur::moment()),
        client_id,
    );
    row.slug = Slug::new(alias_slug(&name));
    // As a layout's: the slug follows the data's name through every edit.
    row.id_structure = Some("%name%".into());
    row.content_hash = row.expected_content_hash();
    row
}

/// Refuse a write that would store something other than an alias under
/// the `alias` field — a malformed alias, one whose slug is not
/// `alias:<its name>`, or a plain log like `occur alias`. Tombstones pass.
pub(crate) fn check_alias_entry(slug: &str, entry: &Entry, data: Option<&JsonValue>) -> Result<()> {
    if entry.field != ALIAS_FIELD || entry.deleted_at.is_some() {
        return Ok(());
    }
    let refuse = |why: String| Err(DatumError::Other(format!("`{slug}`: {why}")));
    let Some(data) = data.filter(|d| d.is_object()) else {
        return refuse(format!(
            "`{ALIAS_FIELD}` is reserved for aliases, and this entry carries none \
             (see `datum alias set`)"
        ));
    };
    let alias: Alias = match serde_json::from_value(data.clone()) {
        Ok(alias) => alias,
        Err(e) => return refuse(format!("not a valid alias: {e}")),
    };
    if alias.name.is_empty() {
        return refuse("an alias needs a name".into());
    }
    if slug != alias_slug(&alias.name) {
        return refuse(format!(
            "the alias `{}` lives at `{}`",
            alias.name,
            alias_slug(&alias.name)
        ));
    }
    Ok(())
}

/// Per-alias outcome reported by [`Db::install_kit_aliases`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitAliasOutcome {
    /// Alias newly installed (or overwritten under `force`).
    Installed,
    /// An alias of this name already existed and the kit's was dropped
    /// silently; the existing one remains the source of truth.
    SkippedExisting {
        /// Name of the kit that currently owns it, or `None` if it's the
        /// person's own.
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
    /// The live entry holding the alias `name`, if there is one.
    fn alias_entry(&self, name: &str) -> Result<Option<Entry>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT * FROM entries WHERE slug = ?1 AND {LIVE_SQL}"),
                params![alias_slug(name)],
                super::row_mapper::row_to_entry,
            )
            .optional()?)
    }

    /// Every live alias, parsed, in name order, narrowed by `filter_sql`.
    fn stored_aliases<P: rusqlite::Params>(
        &self,
        filter_sql: &str,
        params: P,
    ) -> Result<Vec<Alias>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT data FROM entries WHERE {ALIAS_DOC_SQL} {filter_sql}
             ORDER BY json_extract(data, '$.name')"
        ))?;
        let rows = stmt.query_map(params, |row| row.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r.map_err(DatumError::Db)?)?))
            .collect()
    }

    /// Write `alias` as its entry, marked with `kit_id` (`None`: the
    /// person's own): an update of the live one (a new version in its
    /// history, nothing at all when unchanged), else an insert — which
    /// revives a deleted alias's tombstone in place. Returns its hid.
    fn write_alias_entry(&self, alias: &Alias, kit_id: Option<&str>) -> Result<Hid> {
        let mut data = serde_json::to_value(alias)?;
        if let (Some(kit), Some(obj)) = (kit_id, data.as_object_mut()) {
            obj.insert(ALIAS_KIT_KEY.into(), kit.into());
        }
        match self.alias_entry(&alias.name)? {
            Some(row) if row.data.as_ref() == Some(&data) => Ok(row.hid),
            Some(mut row) => {
                row.data = Some(data);
                self.save_existing_entry(&row)?;
                Ok(row.hid)
            }
            None => {
                let client = self.client_id.get().map_or("local", String::as_str);
                let row = new_alias_entry(data, client);
                self.insert_entry(&row)?;
                Ok(self.get_entry(&row.slug)?.hid)
            }
        }
    }

    /// Look up a single alias by name. User/kit aliases shadow
    /// spec-derived ones; on a miss, falls back to the
    /// [`super::types::SpecAliasIndex`].
    pub fn lookup_alias(&self, name: &str) -> Result<Option<Alias>> {
        let json: Option<String> = self
            .conn
            .query_row(
                &format!("SELECT data FROM entries WHERE slug = ?1 AND {ALIAS_DOC_SQL}"),
                params![alias_slug(name)],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(json) = json {
            return Ok(Some(serde_json::from_str(&json)?));
        }
        // Fall through to the spec-derived aliases (da-8y2q.5), by name
        // (da-ulu8f).
        self.derived_alias(name)
    }

    /// Delete a user or kit alias: a SOFT delete, as `datum del` makes, so
    /// it travels to the other devices and `undo` brings it back. Returns
    /// the deleted entry's hid, `None` when there was no such alias.
    /// Spec-derived aliases live in the specs and cannot be deleted here.
    pub fn delete_alias(&self, name: &str) -> Result<Option<Hid>> {
        let Some(row) = self.alias_entry(name)? else {
            return Ok(None);
        };
        self.soft_delete_entry(&row.hid)?;
        Ok(Some(row.hid))
    }

    /// Save a user-created alias — the person's own, so no kit mark.
    /// Returns the entry's hid.
    pub fn save_alias(&self, alias: &Alias) -> Result<Hid> {
        self.write_alias_entry(alias, None)
    }

    /// Install a kit's aliases, each its own entry write marked with
    /// `kit_id` (the kit's `[kit].name`), as a kit's specs are.
    ///
    /// Conflict policy mirrors [`Db::install_kit`] / [`Db::install_kit_views`]:
    /// - `force = false` → leave any existing alias, record `SkippedExisting`.
    ///   This protects the person's own aliases and another kit's alike —
    ///   both are reported as a skip carrying the existing owner.
    /// - `force = true`  → overwrite it and re-mark it with this kit.
    pub fn install_kit_aliases(
        &self,
        kit_id: &str,
        aliases: &[AliasEntry],
        force: bool,
    ) -> Result<InstallAliasesReport> {
        let mut report = InstallAliasesReport::default();
        for entry in aliases {
            if !force {
                if let Some(row) = self.alias_entry(&entry.name)? {
                    let existing_kit = row
                        .data
                        .as_ref()
                        .and_then(|d| d.get(ALIAS_KIT_KEY)?.as_str())
                        .map(str::to_string);
                    report.outcomes.push((
                        entry.name.clone(),
                        KitAliasOutcome::SkippedExisting { existing_kit },
                    ));
                    continue;
                }
            }
            self.write_alias_entry(&entry.clone().into_alias(), Some(kit_id))?;
            report
                .outcomes
                .push((entry.name.clone(), KitAliasOutcome::Installed));
        }
        Ok(report)
    }

    /// Delete (softly) every alias marked with this `kit_id`. The person's
    /// own are untouched. Returns the names removed.
    pub fn uninstall_kit_aliases(&self, kit_id: &str) -> Result<Vec<String>> {
        let names: Vec<String> = self
            .load_aliases_by_kit(kit_id)?
            .into_iter()
            .map(|a| a.name)
            .collect();
        for name in &names {
            self.delete_alias(name)?;
        }
        Ok(names)
    }

    /// Every user and kit alias, by name. Spec-derived aliases are NOT
    /// included — they live in the specs and are reached via
    /// [`Db::spec_alias_index`]. Used by `dm check --aliases` to walk
    /// every user-defined chain for orphan and cycle detection.
    pub fn list_user_aliases(&self) -> Result<Vec<Alias>> {
        self.stored_aliases("", [])
    }

    /// The kit each kit-installed alias came from, by alias name. The
    /// person's own aliases have no entry.
    pub fn alias_kits(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT json_extract(data, '$.name'), json_extract(data, '$.{ALIAS_KIT_KEY}')
             FROM entries WHERE {ALIAS_DOC_SQL} AND json_extract(data, '$.{ALIAS_KIT_KEY}') IS NOT NULL"
        ))?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Every [`Alias`] marked with the given `kit_id`, by name — `datum
    /// kit export`'s source.
    pub fn load_aliases_by_kit(&self, kit_id: &str) -> Result<Vec<Alias>> {
        self.stored_aliases("AND json_extract(data, '$.kit_id') = ?1", params![kit_id])
    }
}

#[cfg(test)]
mod tests {
    use super::Db;
    use crate::field_spec::Alias;

    fn alias(name: &str, to: &str) -> Alias {
        Alias {
            name: name.into(),
            expands_to: to.into(),
            description: None,
        }
    }

    /// An alias is the entry `alias:<name>`; a re-save is a new version of
    /// it, an unchanged one writes nothing, and a delete is a tombstone a
    /// later save revives in place.
    #[test]
    fn an_alias_is_an_entry() {
        let db = Db::open_in_memory().unwrap();
        let hid = db.save_alias(&alias("vbp", "note word")).unwrap();
        let row = db.get_entry("alias:vbp").unwrap();
        assert_eq!(
            (row.hid.clone(), row.field.as_str()),
            (hid.clone(), "alias")
        );

        assert_eq!(db.save_alias(&alias("vbp", "note verb")).unwrap(), hid);
        assert_eq!(
            db.lookup_alias("vbp").unwrap().unwrap().expands_to,
            "note verb"
        );
        db.save_alias(&alias("vbp", "note verb")).unwrap();
        assert_eq!(
            crate::history::latest_version(db.conn(), &hid).unwrap(),
            Some(2)
        );

        assert_eq!(db.delete_alias("vbp").unwrap(), Some(hid.clone()));
        assert!(db.lookup_alias("vbp").unwrap().is_none());
        assert_eq!(db.delete_alias("vbp").unwrap(), None);
        assert_eq!(db.save_alias(&alias("vbp", "occur x")).unwrap(), hid);
    }

    /// A malformed alias, a plain log of the reserved field, or an alias at
    /// the wrong slug is refused, never stored.
    #[test]
    fn a_write_under_the_alias_field_is_validated_as_an_alias() {
        let db = Db::open_in_memory().unwrap();
        let plain = crate::test_util::entry_builder("alias").build();
        let err = db.insert_entry(&plain).unwrap_err().to_string();
        assert!(err.contains("reserved for aliases"), "{err}");

        let bad = super::new_alias_entry(serde_json::json!({"name": "x"}), "d");
        let err = db.insert_entry(&bad).unwrap_err().to_string();
        assert!(err.contains("not a valid alias"), "{err}");

        db.save_alias(&alias("vbp", "note word")).unwrap();
        let mut row = db.get_entry("alias:vbp").unwrap();
        row.data = Some(serde_json::json!({"name": "vbp", "expands_to": 7}));
        assert!(db.save_existing_entry(&row).is_err());
        assert_eq!(
            db.lookup_alias("vbp").unwrap().unwrap().expands_to,
            "note word"
        );
    }
}
