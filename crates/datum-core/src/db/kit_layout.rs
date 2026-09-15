//! Kit-installable dashboard layouts (da-gg0v).
//!
//! Every other kit block has a table: fields go to `field_specs`, views to
//! `views`, aliases to `aliases`. A LAYOUT has none. It is an ordinary
//! entry of field `layout`, slug `layout:<name>`, in the database the kit
//! installs into — byte for byte the row the web's layout store writes one
//! database over (`web/src/lib/layout/store.ts`), plus a `kit_id` in its
//! data naming the kit that shipped it.
//!
//! That is vividn's own arithmetic (2026-09-15 18:53): "layouts have to go
//! inside the db proper, not the _internal db". A layout in the data db
//! syncs with the data, so a kit installed on one device reaches every
//! device of that database; a layout in `_internal` is the person's own and
//! is untouched by an install.
//!
//! `kit_id` in the data blob is therefore the ownership mark that `kit_id`
//! is a COLUMN for elsewhere, and every query here is `json_extract` over
//! it. A user's own layout has none, so uninstall can never take one.

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use crate::dur::Dur;
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::ids::Slug;
use crate::kit::{validate_layout_entry, LayoutEntry};
use crate::time::DatumTime;
use crate::verb::Verb;

use super::Db;

/// The field every layout entry is logged under, here and in the web.
pub const LAYOUT_FIELD: &str = "layout";

/// The data key naming the kit that shipped a layout. A layout without one
/// is the person's own.
pub const LAYOUT_KIT_KEY: &str = "kit_id";

/// `layout:<name>` — the slug the web derives from `--id %name`, and the
/// one this writes, so the two stores agree on a layout's identity.
fn layout_slug(name: &str) -> String {
    format!("{LAYOUT_FIELD}:{name}")
}

/// Per-layout outcome reported by [`Db::install_kit_layouts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitLayoutOutcome {
    /// Layout newly installed, or overwritten under `force`.
    Installed,
    /// An entry already held this layout's slug; the kit's is dropped.
    /// `existing_kit` is the kit that owns it, or `None` for the person's own.
    SkippedExisting { existing_kit: Option<String> },
}

/// Aggregate report from [`Db::install_kit_layouts`].
#[derive(Debug, Clone, Default)]
pub struct InstallLayoutsReport {
    pub outcomes: Vec<(String, KitLayoutOutcome)>,
}

impl InstallLayoutsReport {
    pub fn installed(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter(|&(_name, o)| matches!(o, KitLayoutOutcome::Installed))
            .map(|(name, _o)| name.as_str())
    }

    pub fn skipped(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter(|&(_name, o)| matches!(o, KitLayoutOutcome::SkippedExisting { .. }))
            .map(|(name, _o)| name.as_str())
    }
}

/// The entry's data blob: what the web writes (`name`, `version`, `root`,
/// `designedFor`) plus the kit's mark. The web's spelling is the contract —
/// `designedFor`, not `designed_for` — because the same row is read there.
fn data_of(entry: &LayoutEntry, root: JsonValue, kit_id: &str) -> JsonValue {
    let mut data = JsonMap::new();
    data.insert("name".into(), json!(entry.name));
    data.insert("version".into(), json!(crate::kit::LAYOUT_VERSION));
    data.insert("root".into(), root);
    if let Some(device) = &entry.designed_for {
        data.insert("designedFor".into(), json!(device));
    }
    data.insert(LAYOUT_KIT_KEY.into(), json!(kit_id));
    JsonValue::Object(data)
}

impl Db {
    /// Install a kit's layouts. `kit_id` marks every row; `device_id` is
    /// stamped on the entries as any write stamps one.
    ///
    /// Conflict policy mirrors [`Db::install_kit_views`]: an existing entry
    /// under the layout's slug is left alone unless `force`, in which case
    /// its data is rewritten in place — a new version of the same entry, so
    /// the tree it held stays in its history and `undo` is still the way
    /// back.
    pub fn install_kit_layouts(
        &self,
        kit_id: &str,
        layouts: &[LayoutEntry],
        device_id: &str,
        force: bool,
    ) -> Result<InstallLayoutsReport> {
        // Screen every block before any row lands, as the view path does:
        // a kit with one bad layout installs none of them.
        let roots = layouts
            .iter()
            .map(validate_layout_entry)
            .collect::<Result<Vec<_>>>()?;

        let mut report = InstallLayoutsReport::default();
        for (entry, root) in layouts.iter().zip(roots) {
            let slug = layout_slug(&entry.name);
            let existing: Option<Option<String>> = self
                .conn
                .query_row(
                    "SELECT json_extract(data, '$.kit_id') FROM entries
                     WHERE slug = ?1 AND deleted_at IS NULL",
                    params![slug],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?;

            let data = data_of(entry, root, kit_id);
            match existing {
                Some(owner) if !force => {
                    report.outcomes.push((
                        entry.name.clone(),
                        KitLayoutOutcome::SkippedExisting {
                            existing_kit: owner,
                        },
                    ));
                }
                Some(_) => {
                    let mut row = self.get_entry(&slug)?;
                    row.data = Some(data);
                    self.save_existing_entry(&row)?;
                    report
                        .outcomes
                        .push((entry.name.clone(), KitLayoutOutcome::Installed));
                }
                None => {
                    let mut row = Entry::new(
                        LAYOUT_FIELD,
                        Verb::Point,
                        None,
                        Some(data),
                        Some(DatumTime::now()),
                        Some(Dur::moment()),
                        device_id,
                    );
                    // The slug is the NAME's, not the instant's, so a
                    // reinstall finds the same row and the web's
                    // `layout:<name>` lookup finds this one. `id_structure`
                    // is what keeps it that way through a later edit.
                    row.slug = Slug::new(&slug);
                    row.id_structure = Some("%name%".into());
                    row.content_hash = row.expected_content_hash();
                    self.insert_entry(&row)?;
                    report
                        .outcomes
                        .push((entry.name.clone(), KitLayoutOutcome::Installed));
                }
            }
        }
        Ok(report)
    }

    /// Every layout this kit shipped, newest name first — `kit export`'s
    /// source, and the round-trip of what [`Db::install_kit_layouts`] wrote.
    pub fn load_layouts_by_kit(&self, kit_id: &str) -> Result<Vec<LayoutEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT data FROM entries
             WHERE field = ?1 AND deleted_at IS NULL AND superseded_by IS NULL
               AND json_extract(data, '$.kit_id') = ?2
             ORDER BY json_extract(data, '$.name')",
        )?;
        let rows = stmt.query_map(params![LAYOUT_FIELD, kit_id], |row| {
            row.get::<_, Option<String>>(0)
        })?;
        let mut out = Vec::new();
        for blob in rows {
            let blob = blob.map_err(DatumError::Db)?.unwrap_or_default();
            let data: JsonValue = serde_json::from_str(&blob)
                .map_err(|e| DatumError::Other(format!("kit layout row is not JSON: {e}")))?;
            let name = data
                .get("name")
                .and_then(JsonValue::as_str)
                .unwrap_or_default()
                .to_string();
            out.push(LayoutEntry {
                name,
                designed_for: data
                    .get("designedFor")
                    .and_then(JsonValue::as_str)
                    .map(str::to_string),
                root: data
                    .get("root")
                    .cloned()
                    .unwrap_or(JsonValue::Null)
                    .to_string(),
            });
        }
        Ok(out)
    }

    /// Delete every layout this kit shipped. A SOFT delete, as `datum del`
    /// makes: the rows are the person's data now and their removal has to
    /// travel to the other devices of the database the way any deletion
    /// does. Returns the names removed.
    pub fn uninstall_kit_layouts(&self, kit_id: &str) -> Result<Vec<String>> {
        let names: Vec<String> = self
            .load_layouts_by_kit(kit_id)?
            .into_iter()
            .map(|l| l.name)
            .collect();
        for name in &names {
            self.soft_delete_entry(&layout_slug(name))?;
        }
        Ok(names)
    }
}
