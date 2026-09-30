//! Views, kit commands and kit index definitions are ENTRIES (da-fwwtn),
//! following specs (da-lc26c) and aliases (da-b3752): each is the live entry
//! of its field at the slug its own data names, its data the definition's
//! JSON plus the `kit_id` of the kit that shipped it. So they back up, sync
//! and have a history like any doc.
//!
//! | field     | slug                   | data                          |
//! |-----------|------------------------|-------------------------------|
//! | `view`    | `view:<name>`          | [`ViewEntry`] (+ `kit_id`)    |
//! | `command` | `command:<name>`       | [`CommandEntry`] (+ `kit_id`) |
//! | `index`   | `index:<kit_id>/<name>`| [`IndexEntry`] + `kit_id`     |
//!
//! Only an index's DEFINITION travels: the SQLite index it describes is this
//! device's, derived from the definitions by [`Db::derive_kit_indices`].
//!
//! A write is held to the definition's SHAPE only. What it may DO is checked
//! where it acts: a view's SQL by `Db::execute_view` on every run, an
//! index's DDL by [`IndexEntry::build_ddl`] before it is built — so a
//! definition arriving by sync is screened exactly as a kit's is.

use rusqlite::{params, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::dur::Dur;
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::ids::{Hid, Slug};
use crate::kit::{CommandEntry, IndexEntry, ViewEntry};
use crate::verb::Verb;

use super::types::LIVE_SQL;
use super::Db;

/// The field every view entry is logged under.
pub const VIEW_FIELD: &str = "view";
/// The field every kit command entry is logged under.
pub const COMMAND_FIELD: &str = "command";
/// The field every kit index definition is logged under.
pub const INDEX_FIELD: &str = "index";

/// The data key naming the kit that shipped a definition, as for specs.
pub(crate) const KIT_KEY: &str = "kit_id";

/// The slug's tail for each definition field, as an `id_structure` and as
/// SQL over `data` — an index is its kit's, so two kits may share a name.
fn slug_shape(field: &str) -> (&'static str, &'static str) {
    if field == INDEX_FIELD {
        (
            "%kit_id%/%name%",
            "json_extract(data, '$.kit_id') || '/' || json_extract(data, '$.name')",
        )
    } else {
        ("%name%", "json_extract(data, '$.name')")
    }
}

/// The slug `data` belongs at under `field`.
fn definition_slug(field: &str, data: &JsonValue) -> String {
    let name = data["name"].as_str().unwrap_or_default();
    match data[KIT_KEY].as_str() {
        Some(kit) if field == INDEX_FIELD => format!("{field}:{kit}/{name}"),
        _ => format!("{field}:{name}"),
    }
}

/// `data` without its null, `[]` and `{}` keys, which every definition
/// reads back as its default — so an entry holds what was said, no more.
fn compact(mut data: JsonValue) -> JsonValue {
    if let Some(obj) = data.as_object_mut() {
        obj.retain(|_, v| match v {
            JsonValue::Null => false,
            JsonValue::Array(a) => !a.is_empty(),
            JsonValue::Object(o) => !o.is_empty(),
            _ => true,
        });
    }
    data
}

/// `def` as a definition entry's data, marked with `kit_id`.
pub(crate) fn definition_data<T: serde::Serialize>(
    def: &T,
    kit_id: Option<&str>,
) -> Result<JsonValue> {
    let mut data = compact(serde_json::to_value(def)?);
    if let (Some(kit), Some(obj)) = (kit_id, data.as_object_mut()) {
        obj.insert(KIT_KEY.into(), kit.into());
    }
    Ok(data)
}

/// A new `field` entry for `data`, at the slug its data names.
pub(crate) fn new_definition_entry(field: &str, data: JsonValue, client_id: &str) -> Entry {
    let slug = definition_slug(field, &data);
    let mut row = Entry::new(
        field,
        Verb::Point,
        None,
        Some(data),
        Some(super::spec::meta_occur_time()),
        Some(Dur::moment()),
        client_id,
    );
    row.slug = Slug::new(slug);
    // As an alias's: the slug follows the data's name through every edit.
    row.id_structure = Some(slug_shape(field).0.into());
    row.content_hash = row.expected_content_hash();
    row
}

/// Refuse a write that would store something other than a definition under
/// `view`, `command` or `index` — a malformed one, one at a slug its data
/// does not name, or a plain log like `occur view`. Tombstones pass.
pub(crate) fn check_definition_entry(
    slug: &str,
    entry: &Entry,
    data: Option<&JsonValue>,
) -> Result<()> {
    let field = entry.field.as_str();
    if ![VIEW_FIELD, COMMAND_FIELD, INDEX_FIELD].contains(&field) || entry.deleted_at.is_some() {
        return Ok(());
    }
    let refuse = |why: String| Err(DatumError::Other(format!("`{slug}`: {why}")));
    let Some(data) = data.filter(|d| d.is_object()) else {
        return refuse(format!(
            "`{field}` is reserved for kit {field}s, and this entry carries none \
             (see `datum kit install`)"
        ));
    };
    let parsed = match field {
        VIEW_FIELD => serde_json::from_value::<ViewEntry>(data.clone()).map(|v| v.name),
        COMMAND_FIELD => serde_json::from_value::<CommandEntry>(data.clone()).map(|c| c.name),
        _ => serde_json::from_value::<IndexEntry>(data.clone()).map(|i| i.name),
    };
    let name = match parsed {
        Ok(name) => name,
        Err(e) => return refuse(format!("not a valid {field}: {e}")),
    };
    if name.is_empty() {
        return refuse(format!("a {field} needs a name"));
    }
    if field == INDEX_FIELD && data[KIT_KEY].as_str().is_none() {
        return refuse("an index belongs to a kit, and this one names none".into());
    }
    let home = definition_slug(field, data);
    if slug != home {
        return refuse(format!("this {field} lives at `{home}`"));
    }
    Ok(())
}

impl Db {
    /// The live `field` entry at `slug`, if there is one.
    pub(super) fn definition_entry(&self, slug: &str) -> Result<Option<Entry>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT * FROM entries WHERE slug = ?1 AND {LIVE_SQL}"),
                params![slug],
                super::row_mapper::row_to_entry,
            )
            .optional()?)
    }

    /// The data of every live `field` definition, in slug order — only
    /// those of `kit_id` when one is given. A row not at the slug its data
    /// names (a peer's plain log from before v40) is not a definition.
    pub(super) fn definitions(&self, field: &str, kit_id: Option<&str>) -> Result<Vec<JsonValue>> {
        let body = slug_shape(field).1;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT data FROM entries
             WHERE field = ?1 AND {LIVE_SQL} AND slug = ?1 || ':' || {body}
               AND (?2 IS NULL OR json_extract(data, '$.kit_id') = ?2)
             ORDER BY slug"
        ))?;
        let rows = stmt.query_map(params![field, kit_id], |row| row.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r.map_err(DatumError::Db)?)?))
            .collect()
    }

    /// Write `data` as its `field` entry: an update of the live one (a new
    /// version in its history, nothing at all when unchanged), else an
    /// insert — which revives a deleted one's tombstone in place.
    pub(super) fn write_definition(&self, field: &str, data: JsonValue) -> Result<Hid> {
        match self.definition_entry(&definition_slug(field, &data))? {
            Some(row) if row.data.as_ref() == Some(&data) => Ok(row.hid),
            Some(mut row) => {
                row.data = Some(data);
                self.save_existing_entry(&row)?;
                Ok(row.hid)
            }
            None => {
                let client = self.client_id.get().map_or("local", String::as_str);
                let row = new_definition_entry(field, data, client);
                self.insert_entry(&row)?;
                Ok(self.get_entry(&row.slug)?.hid)
            }
        }
    }

    /// Install a kit's `field` definitions (each `(name, data)`), marked
    /// with `kit_id`. An existing one of the same slug is kept unless
    /// `force` — the person's own and another kit's alike — and reported as
    /// `Some(its kit)`; an installed one is reported as `None`.
    pub(super) fn install_definitions(
        &self,
        field: &str,
        kit_id: &str,
        defs: Vec<(String, JsonValue)>,
        force: bool,
    ) -> Result<Vec<(String, Option<Option<String>>)>> {
        let mut outcomes = Vec::with_capacity(defs.len());
        for (name, mut data) in defs {
            if let Some(obj) = data.as_object_mut() {
                obj.insert(KIT_KEY.into(), kit_id.into());
            }
            if !force {
                if let Some(row) = self.definition_entry(&definition_slug(field, &data))? {
                    let kit = row.data.as_ref().and_then(|d| d[KIT_KEY].as_str());
                    outcomes.push((name, Some(kit.map(str::to_string))));
                    continue;
                }
            }
            self.write_definition(field, data)?;
            outcomes.push((name, None));
        }
        Ok(outcomes)
    }

    /// Delete (softly, so it travels) every `field` definition marked with
    /// `kit_id`. Returns the names removed.
    pub(super) fn uninstall_definitions(&self, field: &str, kit_id: &str) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for data in self.definitions(field, Some(kit_id))? {
            self.soft_delete_entry(&definition_slug(field, &data))?;
            names.push(data["name"].as_str().unwrap_or_default().to_string());
        }
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(name: &str, expression: &str, unique: bool) -> IndexEntry {
        IndexEntry {
            name: name.into(),
            on: "entries".into(),
            expression: expression.into(),
            where_clause: None,
            unique,
        }
    }

    fn built(db: &Db, name: &str) -> bool {
        db.conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [name],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            > 0
    }

    /// A definition that arrives past the install screen — by sync, by an
    /// edit — is built only through the install's own validation: one that
    /// fails it is reported and never built, the rest still are, and a
    /// definition deleted takes its index with it.
    #[test]
    fn an_index_definition_is_built_only_through_its_validation() {
        let db = Db::open_in_memory().unwrap();
        let evil = index("evil", "field); DROP TABLE entries; --", false);
        for ix in [&evil, &index("ok", "field", false)] {
            db.write_definition(INDEX_FIELD, definition_data(ix, Some("k")).unwrap())
                .unwrap();
        }
        let failed = db.derive_kit_indices().unwrap();
        assert_eq!(
            failed.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(),
            ["index:k/evil"]
        );
        assert!(built(&db, "kit_k__ok") && !built(&db, "kit_k__evil"));
        // Nor does the bad one stop the kit being read or uninstalled.
        let listed = db.load_indices_by_kit("k").unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|i| i.index_name.as_str())
                .collect::<Vec<_>>(),
            ["ok"]
        );

        db.uninstall_kit("k").unwrap();
        assert!(!built(&db, "kit_k__ok"));
        assert!(db.definitions(INDEX_FIELD, None).unwrap().is_empty());
    }

    /// A UNIQUE index over duplicates will not build: the install fails, and
    /// the kit's indices are left as they were — none on a first install,
    /// the working one on a re-install — so nothing is retried on every
    /// open. Indices go first, so no spec has been written (da-45amw).
    #[test]
    fn an_index_that_will_not_build_fails_the_install() {
        let db = Db::open_in_memory().unwrap();
        for _ in 0..2 {
            crate::test_util::entry_builder("x").insert(&db);
        }
        let spec = |f: &str| crate::test_util::spec_builder(f).build();
        let unique = index("one", "field", true);
        let err = db
            .install_kit("k", &[spec("a")], std::slice::from_ref(&unique), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("UNIQUE"), "{err}");
        assert!(db.load_indices_by_kit("k").unwrap().is_empty());
        assert!(!built(&db, "kit_k__one"));
        assert!(db.load_spec("a").unwrap().is_none());

        let working = index("one", "field", false);
        db.install_kit("k", &[], std::slice::from_ref(&working), false)
            .unwrap();
        db.install_kit("k", &[spec("b")], &[unique], false)
            .unwrap_err();
        let kept = db.load_indices_by_kit("k").unwrap();
        assert_eq!(
            kept.iter().map(|i| &i.entry).collect::<Vec<_>>(),
            [&working]
        );
        assert!(built(&db, "kit_k__one"));
        assert!(db.load_spec("b").unwrap().is_none());
    }
}
