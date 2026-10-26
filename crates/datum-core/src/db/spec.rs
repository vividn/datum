//! Field specs are ENTRIES (da-lc26c): a field's spec is the live entry of
//! field `spec` at slug `spec:<field>`, its data the spec's JSON plus the
//! `kit_id` of the kit that shipped it — the shape layouts took in da-gg0v
//! (`kit_layout.rs`). The entries table is the one source of truth, so a
//! spec backs up, syncs and has a history like any doc, and `update` /
//! `edit` / `undo` / `history spec:mood` work on it.
//!
//! This module loads, saves and deletes them, validates any write of one
//! ([`check_spec_entry`], called by every row writer in `insert.rs`), and
//! builds the in-memory [`SpecRegistry`] from them. Derived aliases are
//! read out of the specs lazily — see [`super::spec_alias_index`].

use chrono::DateTime;
use rusqlite::{params, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::action::Action;
use crate::dur::Dur;
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::field_spec::{contains_template, verb_from_log, Covered, FieldSpec, SpecRegistry};
use crate::history::ChangeKind;
use crate::ids::Slug;
use crate::shape::{is_ignored_state, TRACKED_STATE_VALUE};
use crate::time::DatumTime;
use crate::verb::Verb;
use datum_parse::field_template::{is_template, literal, matches, parts, ties};

use super::alias::ALIAS_FIELD;
use super::definition::{COMMAND_FIELD, INDEX_FIELD, KIT_FIELD, SETTING_FIELD, VIEW_FIELD};
use super::insert::{insert_new_in_tx, save_existing_entry_with_kind_in_tx};
use super::kit_layout::LAYOUT_FIELD;
use super::types::{AliasConflict, AliasConflictReason, SaveSpecReport, LIVE_SQL};
use super::Db;

/// What keeps a write from creating a field ([`Db::creation_closer`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Closer {
    /// The template of this name, whose `auto_setup` is false.
    Template(String),
    /// The database's own `auto_setup` setting (da-w6pp2).
    Db,
}

/// The field every spec entry is logged under.
pub const SPEC_FIELD: &str = "spec";

/// The data key naming the kit that shipped a spec, as for layouts.
pub(crate) const SPEC_KIT_KEY: &str = "kit_id";

/// The data key holding the whole spec entry data a kit took a field over
/// from, which uninstalling the kit writes back (da-uokdb).
pub(crate) const SPEC_DISPLACED_KEY: &str = "displaced";

/// Fields whose entries describe the database rather than the life it
/// records — specs, layouts, aliases (da-b3752), views, kit commands,
/// kit index definitions (da-fwwtn), kits' `[kit]` tables (da-xnzc7) and the
/// database's settings (da-w6pp2). One rule keeps them all off the
/// life-data surfaces — tail, now, dayview, nowview, stats, fields —
/// unless asked for by name (`tail spec`) or with `--show-meta` / `-A`.
pub const META_FIELDS: &[&str] = &[
    SPEC_FIELD,
    LAYOUT_FIELD,
    ALIAS_FIELD,
    VIEW_FIELD,
    COMMAND_FIELD,
    INDEX_FIELD,
    KIT_FIELD,
    SETTING_FIELD,
];

/// [`META_FIELDS`] as a SQL predicate on `entries.field`.
pub(crate) const NOT_META_FIELD_SQL: &str =
    "field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit', 'setting')";

pub fn is_meta_field(field: &str) -> bool {
    META_FIELDS.contains(&field)
}

/// The rows the readers take for specs: a live `spec` entry at the slug
/// its own data names. The writers in `insert.rs` hold every row to that
/// ([`check_spec_entry`]), but sync stores a peer's rows as they arrive,
/// and a peer from before v38 may hold a plain `occur spec` log. Such a row
/// is not a spec to read. A malformed spec at its own slug still errors.
pub(super) const SPEC_DOC_SQL: &str =
    "field = 'spec' AND deleted_at IS NULL AND superseded_by IS NULL \
     AND slug = 'spec:' || json_extract(data, '$.field')";

/// `spec:<field>` — a spec's slug, so the same spec is the same doc on
/// every device.
pub(crate) fn spec_slug(field: &str) -> String {
    format!("{SPEC_FIELD}:{field}")
}

/// The spec rows whose field may be a template (da-38obu), as the partial
/// index `entries_spec_templates` holds them: two `%` in the slug. A field
/// whose percents are escaped (`50\%off\%`) is a row here too, and
/// [`Db::template_specs`] drops it by [`is_template`]; a paired `%off%` is a
/// placeholder, in a spec's name as in a logged field's.
const TEMPLATE_SPEC_SQL: &str = "field = 'spec' AND slug GLOB '*%*%*'";

/// A spec entry's data keys that are not the spec's own to set: its field,
/// and its kit marks.
const NOT_SET: &[&str] = &["field", SPEC_KIT_KEY, SPEC_DISPLACED_KEY];

/// Of `templates` (best first, as [`Db::template_specs`] gives them), the
/// one covering `field`. A template covers no template, nor a meta field.
fn covering<'a>(field: &str, templates: &'a [(String, String)]) -> Option<&'a (String, String)> {
    if is_template(field) || is_meta_field(field) {
        return None;
    }
    templates.iter().find(|(name, _)| matches(name, field))
}

/// Every meta entry's occur time: fixed, not the moment it was
/// written, so two devices writing (or migrating) the same one make the
/// same doc.
pub(crate) fn meta_occur_time() -> DatumTime {
    DatumTime {
        utc: DateTime::UNIX_EPOCH,
        offset_minutes: 0,
        timezone: None,
        uncertainty: None,
    }
}

/// A new spec entry for `data` (the spec's JSON, with its kit mark).
pub(crate) fn new_spec_entry(field: &str, data: JsonValue, client_id: &str) -> Entry {
    let mut row = Entry::new(
        SPEC_FIELD,
        Verb::Point,
        None,
        Some(data),
        Some(meta_occur_time()),
        Some(Dur::moment()),
        client_id,
    );
    row.slug = Slug::new(spec_slug(field));
    // A literal template: the slug stays `spec:<field>` through every edit,
    // a field template's placeholders too (da-38obu).
    row.id_structure = Some(field.replace('%', "\\%"));
    row.content_hash = row.expected_content_hash();
    row
}

/// Refuse a write under a field the database reserves: a spec
/// ([`check_spec_entry`]), an alias ([`super::alias::check_alias_entry`]),
/// or a view, command, index, kit or setting ([`super::definition::check_definition_entry`]).
/// Every row writer in `insert.rs` calls this.
pub(crate) fn check_meta_entry(slug: &str, entry: &Entry, data: Option<&JsonValue>) -> Result<()> {
    check_spec_entry(slug, entry, data)?;
    super::alias::check_alias_entry(slug, entry, data)?;
    super::definition::check_definition_entry(slug, entry, data)
}

/// Refuse a write that would store something other than a spec under the
/// `spec` field — a malformed spec, a spec whose slug is not
/// `spec:<its field>`, or a plain log like `occur spec`. Tombstones pass:
/// deleting is always allowed.
fn check_spec_entry(slug: &str, entry: &Entry, data: Option<&JsonValue>) -> Result<()> {
    if entry.field != SPEC_FIELD || entry.deleted_at.is_some() {
        return Ok(());
    }
    let refuse = |why: String| Err(DatumError::InvalidFieldSpec(format!("`{slug}`: {why}")));
    let Some(mut data) = data.filter(|d| d.is_object()).cloned() else {
        return refuse(format!(
            "`{SPEC_FIELD}` is reserved for field specs, and this entry carries none \
             (see `datum setup`)"
        ));
    };
    // A stored spec may predate its verb (da-c8mh); reading fills it from
    // the log, so a missing one is not malformed.
    let obj = data.as_object_mut().expect("checked is_object");
    let stated = |k: &str| obj.get(k).is_some_and(|v| !v.is_null());
    if !stated("verb") && !stated("default_verb") {
        obj.remove("default_verb");
        obj.insert("verb".into(), serde_json::json!(Action::Occur));
    }
    let spec: FieldSpec = match serde_json::from_value(data) {
        Ok(spec) => spec,
        Err(e) => return refuse(format!("not a valid field spec: {e}")),
    };
    crate::slug::validate_field_name(&spec.field)?;
    if slug != spec_slug(&spec.field) {
        return refuse(format!(
            "the spec for `{}` lives at `{}`",
            spec.field,
            spec_slug(&spec.field)
        ));
    }
    // da-j04cq: whether a write may create a field is a question about the
    // fields a template covers; a plain field's spec is its existence.
    if spec.auto_setup.is_some() && !is_template(&spec.field) {
        return refuse(format!(
            "`auto_setup` is a template's to set (`issue__%hid:8%`), and `{}` is not one: \
             its own spec already says it exists",
            spec.field
        ));
    }
    Ok(())
}

impl Db {
    /// Deserialize a stored `spec_json`.
    ///
    /// A spec written before da-c8mh spells its verb `default_verb` —
    /// the serde alias on [`FieldSpec::verb`] reads that — and the
    /// verbless specs da-emze allowed store it as `null`. Those carry no
    /// answer at all, so the verb is filled from the field's own log
    /// ([`Db::verb_from_field_log`]), the same evidence `track` leans on.
    /// An existing database therefore needs no migration.
    pub(crate) fn parse_spec(&self, json: &str) -> Result<FieldSpec> {
        let err = match serde_json::from_str::<FieldSpec>(json) {
            Ok(spec) => return Ok(spec),
            Err(err) => err,
        };
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json) else {
            return Err(err.into());
        };
        let Some(obj) = value.as_object_mut() else {
            return Err(err.into());
        };
        let stated = |key: &str| obj.get(key).is_some_and(|v| !v.is_null());
        if stated("verb") || stated("default_verb") {
            // A verb IS stated and the spec still does not read: that is
            // a real error, not the old shape.
            return Err(err.into());
        }
        let field = obj
            .get("field")
            .and_then(|f| f.as_str())
            .unwrap_or_default()
            .to_string();
        let verb = self.verb_from_field_log(&field)?.unwrap_or(Action::Occur);
        // Both keys would be the same field to serde (they are aliases),
        // and two of them is a duplicate-field error.
        obj.remove("default_verb");
        obj.insert("verb".to_string(), serde_json::to_value(verb)?);
        Ok(serde_json::from_value(value)?)
    }

    /// Every live spec, parsed, in the order `tail_sql` (an `ORDER BY`)
    /// gives. The kit mark in the data is not a spec key, and serde
    /// passes over it.
    pub(crate) fn stored_specs(&self, tail_sql: &str) -> Result<Vec<FieldSpec>> {
        self.stored_specs_where("", tail_sql, [])
    }

    pub(super) fn stored_specs_where<P: rusqlite::Params>(
        &self,
        filter_sql: &str,
        tail_sql: &str,
        params: P,
    ) -> Result<Vec<FieldSpec>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT data FROM entries WHERE {SPEC_DOC_SQL} {filter_sql} {tail_sql}"
        ))?;
        let rows = stmt.query_map(params, |row| row.get::<_, String>(0))?;
        rows.map(|r| self.parse_spec(&r.map_err(DatumError::Db)?))
            .collect()
    }

    /// The live entry holding `field`'s spec, if it has one.
    pub fn spec_entry(&self, field: &str) -> Result<Option<Entry>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT * FROM entries WHERE slug = ?1 AND {LIVE_SQL}"),
                params![spec_slug(field)],
                super::row_mapper::row_to_entry,
            )
            .optional()?)
    }

    /// Load a field spec by name. None if not defined. A field a template
    /// spec covers reads through it (da-38obu, [`Db::covered_spec`]).
    pub fn load_spec(&self, field: &str) -> Result<Option<FieldSpec>> {
        let own = self.own_spec_data(field)?;
        match covering(field, &self.template_specs()?) {
            Some((template, data)) => Ok(Some(self.covered_spec(field, template, data, own)?)),
            None => own.map(|j| self.parse_spec(&j)).transpose(),
        }
    }

    /// The data of `field`'s own spec entry, as stored.
    fn own_spec_data(&self, field: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT data FROM entries WHERE slug = ?1 AND field = '{SPEC_FIELD}' AND {LIVE_SQL}"),
                params![spec_slug(field)],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Every template spec (da-38obu) as its name and stored data, best
    /// first: the longer literal text, then the first by name, so every
    /// copy reads a field through the same one. Few, and read by their
    /// own index.
    pub(crate) fn template_specs(&self) -> Result<Vec<(String, String)>> {
        // Named, or the planner takes `field`'s index and reads every spec.
        let sql = |index: &str| {
            format!(
                "SELECT json_extract(data, '$.field'), data FROM entries {index}
                  WHERE {TEMPLATE_SPEC_SQL} AND {SPEC_DOC_SQL}"
            )
        };
        let mut stmt = match self
            .conn
            .prepare_cached(&sql("INDEXED BY entries_spec_templates"))
        {
            Ok(stmt) => stmt,
            // A file a read-only handle opens may predate the index.
            Err(_) => self.conn.prepare_cached(&sql(""))?,
        };
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut templates = rows.collect::<rusqlite::Result<Vec<(String, String)>>>()?;
        templates.retain(|(name, _)| is_template(name));
        templates.sort_by_cached_key(|(name, _)| {
            (
                std::cmp::Reverse(literal(name).chars().count()),
                name.clone(),
            )
        });
        Ok(templates)
    }

    /// `field`'s spec read through `template` (da-38obu): the template's
    /// keys, and over them, key by key, those `own` — the field's own spec
    /// entry's data — sets. An own spec carrying a verb and nothing chosen
    /// is the one a first log wrote before the template came, and sets none.
    fn covered_spec(
        &self,
        field: &str,
        template: &str,
        data: &str,
        own: Option<String>,
    ) -> Result<FieldSpec> {
        let mut data: JsonValue = serde_json::from_str(data)?;
        let obj = data
            .as_object_mut()
            .ok_or_else(|| DatumError::InvalidFieldSpec(format!("`{template}` is no spec")))?;
        obj.insert("field".into(), field.into());
        let mut set = Vec::new();
        let own: Option<JsonValue> = own.map(|j| serde_json::from_str(&j)).transpose()?;
        let bare = |own: &JsonValue| {
            own.get("verb").is_some()
                && self.parse_spec(&own.to_string()).is_ok_and(|s| s.is_bare())
        };
        if let Some(own) = own.filter(|own| !bare(own)) {
            for (k, v) in own.as_object().into_iter().flatten() {
                if !NOT_SET.contains(&k.as_str()) {
                    obj.insert(k.clone(), v.clone());
                    set.push(k.clone());
                }
            }
        }
        let mut spec = self.parse_spec(&data.to_string())?;
        spec.template = Some(Covered {
            template: template.to_string(),
            own: set,
        });
        Ok(spec)
    }

    /// The data `spec`'s own entry stores (da-38obu): the keys it sets
    /// differently from the template covering its field, all of them for a
    /// field none covers. A template's that ties one already stored is
    /// refused.
    fn own_spec_json(&self, spec: &FieldSpec, new: bool) -> Result<JsonValue> {
        let mut data = serde_json::to_value(spec)?;
        let templates = self.template_specs()?;
        if new && is_template(&spec.field) {
            if let Some((other, _)) = templates.iter().find(|(t, _)| ties(t, &spec.field)) {
                return Err(DatumError::InvalidFieldSpec(format!(
                    "`{}` ties the template `{other}`: a field could match both, and \
                     neither has the longer literal text to win",
                    spec.field
                )));
            }
        }
        if let Some((template, tdata)) = covering(&spec.field, &templates) {
            let from =
                serde_json::to_value(self.covered_spec(&spec.field, template, tdata, None)?)?;
            if let Some(obj) = data.as_object_mut() {
                obj.retain(|k, v| k == "field" || from.get(k) != Some(v));
            }
        }
        Ok(data)
    }

    /// What a bare press of `field` means: the verb its spec declares,
    /// else the one its log implies ([`verb_from_log`]).
    ///
    /// Every spec has a verb (da-c8mh), so this only falls back to the log
    /// for a field with no spec at all — a field the first log is about to
    /// mint one for.
    pub fn field_verb(&self, field: &str) -> Result<Action> {
        if let Some(spec) = self.load_spec(field)? {
            return Ok(spec.verb);
        }
        Ok(self.verb_from_field_log(field)?.unwrap_or(Action::Occur))
    }

    /// The verb `field`'s own log implies, or `None` when the field has
    /// no logged state to read — the evidence [`Db::field_verb`] and
    /// `datum track` lean on for a field that has never been specced.
    /// TRACKED / IGNORED rows are statuses, not states, so they neither
    /// count as evidence nor as "has a log" (da-5ifp).
    pub fn verb_from_field_log(&self, field: &str) -> Result<Option<Action>> {
        let states: Vec<serde_json::Value> = self
            .distinct_field_states()?
            .into_iter()
            .filter(|(f, s)| f == field && *s != TRACKED_STATE_VALUE && !is_ignored_state(s))
            .map(|(_, s)| s)
            .collect();
        if states.is_empty() && !self.field_has_entries(field)? {
            return Ok(None);
        }
        Ok(Some(verb_from_log(&states)))
    }

    /// True when `field` exists (da-j04cq): it has an entry or a spec of its
    /// own. A template's spec covering the name does not make it one, or
    /// every mistyped id a template matches would.
    pub fn field_exists(&self, field: &str) -> Result<bool> {
        Ok(self.field_has_entries(field)? || self.own_spec_data(field)?.is_some())
    }

    /// What closes `field` to a write that would create it (da-j04cq,
    /// da-tldkb, da-w6pp2). The template covering it, when that sets
    /// `auto_setup` (`false` closes, `true` opens whatever the database
    /// says); with none covering it, a template saying `false` whose literal
    /// head (`issue__`) the name begins with, so `issue__k7f` is closed though
    /// no 8-character id; then the database's own `auto_setup` setting.
    /// `None`: a write may create it.
    pub fn creation_closer(&self, field: &str) -> Result<Option<Closer>> {
        if is_template(field) || is_meta_field(field) {
            return Ok(None);
        }
        let templates = self.template_specs()?;
        let auto_setup = |data: &str| {
            serde_json::from_str::<JsonValue>(data)
                .ok()
                .and_then(|d| d.get("auto_setup").and_then(JsonValue::as_bool))
        };
        match covering(field, &templates) {
            Some((name, data)) => match auto_setup(data) {
                Some(false) => return Ok(Some(Closer::Template(name.clone()))),
                Some(true) => return Ok(None),
                None => {}
            },
            None => {
                if let Some((name, _)) = templates.iter().find(|(name, data)| {
                    let head = &parts(name)[0];
                    !head.is_empty()
                        && field.starts_with(head.as_str())
                        && auto_setup(data) == Some(false)
                }) {
                    return Ok(Some(Closer::Template(name.clone())));
                }
            }
        }
        Ok((self.setting("auto_setup")? == Some(false)).then_some(Closer::Db))
    }

    /// The fields whose names start with `prefix` (da-tldkb), alphabetical,
    /// at most `cap`: a live entry's or a spec's of its own, and no meta
    /// field or template. A skip-scan of the prefix's range, as
    /// [`Db::distinct_fields`] is of every name, stopping at `cap`, and the
    /// specs read by slug (by `field`, the planner read every spec), so it
    /// reads at most `cap` names of each.
    pub fn fields_starting(&self, prefix: &str, cap: usize) -> Result<Vec<String>> {
        // Past every name starting `prefix` but one whose next character
        // is U+10FFFF itself.
        let end = format!("{prefix}\u{10FFFF}");
        let mut stmt = self.conn.prepare_cached(&format!(
            "WITH RECURSIVE f(field, n) AS (
                 SELECT min(field), 1 FROM entries WHERE field >= ?1 AND field < ?2 AND {LIVE_SQL}
                 UNION ALL
                 SELECT (SELECT min(field) FROM entries
                          WHERE field > f.field AND field < ?2 AND {LIVE_SQL}), n + 1
                   FROM f WHERE f.field IS NOT NULL AND n < ?3
             )
             SELECT field FROM f WHERE field IS NOT NULL AND {NOT_META_FIELD_SQL}
             UNION
             SELECT * FROM (
                 SELECT json_extract(data, '$.field') FROM entries INDEXED BY entries_slug_lookup
                  WHERE slug >= 'spec:' || ?1 AND slug < 'spec:' || ?2 AND {SPEC_DOC_SQL}
                  LIMIT ?3)
             ORDER BY 1 LIMIT ?3"
        ))?;
        let rows = stmt.query_map(params![prefix, end, cap as i64], |row| {
            row.get::<_, String>(0)
        })?;
        let mut fields = rows.collect::<rusqlite::Result<Vec<String>>>()?;
        fields.retain(|f| !is_meta_field(f) && !is_template(f));
        Ok(fields)
    }

    /// True when some entry, live or not, holds `hid` (da-tldkb): no field
    /// may be named as one.
    pub fn hid_exists(&self, hid: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE hid = ?1)",
            params![hid],
            |row| row.get(0),
        )?;
        Ok(n != 0)
    }

    /// True when at least one entry exists for `field` (any state,
    /// including none).
    pub fn field_has_entries(&self, field: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE field = ?1)",
            params![field],
            |row| row.get(0),
        )?;
        Ok(n != 0)
    }

    /// Every field that has a declared spec, alphabetical. A spec is a
    /// field the user has NAMED, so it belongs to the field list even
    /// with no entry logged against it yet — `fields` has always listed
    /// it (`compute_fields` unions the registry in) and
    /// [`Db::get_tracked_fields`] does the same (da-m1l8).
    /// A meta field's spec, if one exists, is not a life field to list,
    /// nor is a template's (`issue__%hid:8%`): it covers fields and is not
    /// one (da-jguh2).
    pub fn spec_fields(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT json_extract(data, '$.field') AS f FROM entries
             WHERE {SPEC_DOC_SQL}
               AND f NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit', 'setting')
             ORDER BY f"
        ))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.filter(|r| !r.as_ref().is_ok_and(|f| contains_template(f)))
            .map(|r| r.map_err(DatumError::Db))
            .collect()
    }

    /// Load specs for many fields in a single query. Fields without a
    /// stored spec are absent from the returned map.
    pub fn batch_load_specs(
        &self,
        fields: &[&str],
    ) -> Result<std::collections::HashMap<String, FieldSpec>> {
        let mut out: std::collections::HashMap<String, FieldSpec> =
            std::collections::HashMap::with_capacity(fields.len());
        if fields.is_empty() {
            return Ok(out);
        }
        let placeholders = std::iter::repeat_n("?", fields.len())
            .collect::<Vec<_>>()
            .join(",");
        let slugs = fields.iter().map(|f| spec_slug(f));
        for spec in self.stored_specs_where(
            &format!("AND slug IN ({placeholders})"),
            "",
            rusqlite::params_from_iter(slugs),
        )? {
            out.insert(spec.field.clone(), spec);
        }
        let templates = self.template_specs()?;
        for field in fields {
            if let Some((template, data)) = covering(field, &templates) {
                let own = self.own_spec_data(field)?;
                out.insert(
                    field.to_string(),
                    self.covered_spec(field, template, data, own)?,
                );
            }
        }
        Ok(out)
    }

    /// Save a field spec. Derived aliases are not written to the
    /// alias entries — they live in the spec and are surfaced lazily via
    /// [`Db::spec_alias_index`].
    pub fn save_field_spec(&self, spec: &FieldSpec) -> Result<()> {
        let _ = self.save_field_spec_with_report(spec)?;
        Ok(())
    }

    /// Stamp the spec entries this handle writes with `client_id`. The
    /// first call wins; unset, they say `local`.
    pub fn set_client_id(&self, client_id: &str) {
        let _ = self.client_id.set(client_id.to_string());
    }

    /// Write `spec` as its entry: an update of the live one (a new version
    /// in its history, and nothing at all when the spec is unchanged), else
    /// an insert — which revives a deleted spec's tombstone in place.
    /// `kit_id` is the mark to write: `Some` sets it, `None` keeps the one
    /// the entry already carries, so a user's `spec` on a kit field
    /// leaves it the kit's. A kit taking the spec over from its owner keeps
    /// the owner's data as `displaced`, for uninstall to hand back; any
    /// other write carries the `displaced` already there. Returns the entry.
    pub(crate) fn write_spec_entry(
        &self,
        spec: &FieldSpec,
        kit_id: Option<&str>,
        with: Option<&Entry>,
    ) -> Result<Entry> {
        crate::slug::validate_field_name(&spec.field)?;
        // The spec it replaces is read before the lock, and `with` lands in
        // the same transaction: both or neither (da-2pbvd, da-8i7f0).
        let slug = self.compute_then_write_with(
            |_| {
                let existing = self.spec_entry(&spec.field)?;
                if existing.is_none() {
                    crate::slug::validate_new_field_name(&spec.field)?;
                }
                let mut data = self.own_spec_json(spec, existing.is_none())?;
                let old = existing.as_ref().and_then(|e| e.data.as_ref());
                let old_kit = old.and_then(|d| d.get(SPEC_KIT_KEY));
                let (kit, displaced) = match kit_id {
                    Some(k) if old_kit.and_then(JsonValue::as_str) != Some(k) => {
                        (Some(k.into()), old)
                    }
                    _ => (
                        kit_id.map(JsonValue::from).or_else(|| old_kit.cloned()),
                        old.and_then(|d| d.get(SPEC_DISPLACED_KEY)),
                    ),
                };
                if let Some(obj) = data.as_object_mut() {
                    if let Some(kit) = kit {
                        obj.insert(SPEC_KIT_KEY.into(), kit);
                    }
                    if let Some(displaced) = displaced {
                        obj.insert(SPEC_DISPLACED_KEY.into(), displaced.clone());
                    }
                }
                Ok((existing, data))
            },
            |tx, (existing, data)| {
                let slug = match existing {
                    Some(row) if row.data.as_ref() == Some(&data) => row.slug,
                    Some(mut row) => {
                        row.data = Some(data);
                        save_existing_entry_with_kind_in_tx(tx, &row, ChangeKind::UserEdit)?;
                        row.slug
                    }
                    None => {
                        let client = self.client_id.get().map_or("local", String::as_str);
                        let row = new_spec_entry(&spec.field, data, client);
                        insert_new_in_tx(tx, &row)?;
                        row.slug
                    }
                };
                if let Some(entry) = with {
                    insert_new_in_tx(tx, entry)?;
                }
                Ok::<_, DatumError>(slug)
            },
        )?;
        self.get_entry(&slug)
    }

    /// Variant of [`Db::save_field_spec`] that returns a structured
    /// report of which derived aliases this spec contributed to the
    /// effective alias namespace and which were shadowed by an existing
    /// user/kit alias or by another spec's earlier registration in
    /// the [`super::types::SpecAliasIndex`].
    pub fn save_field_spec_with_report(&self, spec: &FieldSpec) -> Result<SaveSpecReport> {
        self.save_field_spec_with(spec, None)
    }

    /// [`Db::save_field_spec_with_report`], with `status` — a field's TRACK
    /// or IGNORED row — written in the same transaction (da-2pbvd).
    pub fn save_field_spec_with(
        &self,
        spec: &FieldSpec,
        status: Option<&Entry>,
    ) -> Result<SaveSpecReport> {
        let written = self.write_spec_entry(spec, None, status)?;

        // Compute the report by inspecting the now-current state of the
        // alias namespace. A derived alias is "saved" iff (a) no user/kit
        // alias shadows it and (b) the spec that wins the name is this
        // field's. Otherwise it is shadowed and we attach the cause.
        let mut saved: Vec<String> = Vec::new();
        let mut skipped: Vec<AliasConflict> = Vec::new();
        let user_aliases = self.list_user_aliases()?;
        for d in &spec.derived_aliases {
            // Shadowed by a user/kit alias?
            if user_aliases.iter().any(|a| a.name == d.name) {
                skipped.push(AliasConflict {
                    name: d.name.clone(),
                    reason: AliasConflictReason::UserAlias,
                });
                continue;
            }
            // Cross-spec collision: an earlier spec owns the name.
            match self.derived_alias_owner(&d.name)?.map(|s| s.field) {
                Some(owner) if owner == spec.field => saved.push(d.name.clone()),
                owner => skipped.push(AliasConflict {
                    name: d.name.clone(),
                    reason: AliasConflictReason::OwnedByField(owner.unwrap_or(spec.field.clone())),
                }),
            }
        }
        Ok(SaveSpecReport {
            saved,
            skipped,
            hid: written.hid,
        })
    }

    /// Delete a field spec: a SOFT delete, as `datum del` makes, so it
    /// travels to the other devices and `undo` brings it back. Derived
    /// aliases go with it — they are read out of the live specs.
    /// Returns the deleted entry's hid, `None` when there was no spec.
    pub fn delete_field_spec(&self, field: &str) -> Result<Option<crate::ids::Hid>> {
        let Some(row) = self.spec_entry(field)? else {
            return Ok(None);
        };
        self.soft_delete_entry(&row.hid)?;
        Ok(Some(row.hid))
    }

    /// Delete `field`'s spec when nothing live is logged against the field
    /// and the spec is still the bare one its first log wrote (da-elv6d):
    /// a delete of a field's last entry takes the field with it. A spec
    /// someone shaped ([`FieldSpec::is_bare`] false) or a kit declared
    /// stays, a field with no entries. `kind` is the step's own, so an
    /// `undo`'s spec delete is one `redo` steps forward again. Returns the
    /// tombstoned spec row.
    pub fn delete_spec_if_orphaned_bare(
        &self,
        field: &str,
        kind: crate::history::ChangeKind,
    ) -> Result<Option<Entry>> {
        let live: bool = self.conn.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM entries WHERE field = ?1 AND {LIVE_SQL})"),
            params![field],
            |row| row.get(0),
        )?;
        if live || is_meta_field(field) {
            return Ok(None);
        }
        let Some(row) = self.spec_entry(field)? else {
            return Ok(None);
        };
        let Some(data) = row.data.as_ref().filter(|d| d.get(SPEC_KIT_KEY).is_none()) else {
            return Ok(None);
        };
        if !self.parse_spec(&data.to_string())?.is_bare() {
            return Ok(None);
        }
        self.soft_delete_entry_as(&row.hid, kind)?;
        Ok(Some(self.get_entry_including_deleted(&row.hid)?))
    }

    /// Return field specs whose `tags` array contains `tag`, by field.
    /// Membership, not substring: `chore` does not match `chores`.
    pub fn list_specs_by_tag(&self, tag: &str) -> Result<Vec<FieldSpec>> {
        let mut specs = self.stored_specs("ORDER BY json_extract(data, '$.field')")?;
        specs.retain(|s| s.has_tag(tag));
        Ok(specs)
    }

    pub fn load_registry(&self) -> Result<SpecRegistry> {
        let mut registry = SpecRegistry::new();

        for spec in self.stored_specs("")? {
            registry.register_field(spec);
        }

        // Spec-derived aliases come first so user/kit rows (registered
        // below) can overwrite them — mirroring the runtime precedence
        // applied by `lookup_alias`.
        let index = self.spec_alias_index()?;
        for alias in index.entries.values() {
            registry.register_alias(alias.clone());
        }
        drop(index);

        for alias in self.list_user_aliases()? {
            registry.register_alias(alias);
        }

        Ok(registry)
    }

    /// The registry of `fields`' specs alone, and no aliases: what a
    /// command rendering entries of those fields reads, by their slugs.
    /// [`Db::load_registry`] parses every spec twice over, which costs a
    /// command time in the number of fields it never names (da-ulu8f).
    pub fn load_registry_for(&self, fields: &[&str]) -> Result<SpecRegistry> {
        Ok(SpecRegistry {
            fields: self.batch_load_specs(fields)?,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tag_tests {
    use super::Db;
    use crate::action::Action;
    use crate::field_spec::{FieldSpec, Render};
    use std::collections::BTreeMap;

    /// da-lc26c: a spec is the entry `spec:<field>`, and a re-save is a new
    /// version of that one entry, not a second row.
    #[test]
    fn a_spec_is_an_entry_and_a_resave_is_a_new_version() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("dishes", Action::Occur);
        spec.tags = vec!["chore".into(), "daily".into()];
        let first = db.save_field_spec_with_report(&spec).unwrap().hid;

        let row = db.get_entry("spec:dishes").unwrap();
        assert_eq!(row.hid, first);
        assert_eq!(row.field, "spec");
        assert_eq!(
            row.data.as_ref().unwrap()["tags"],
            serde_json::json!(["chore", "daily"])
        );

        spec.tags = vec!["chore".into()];
        let second = db.save_field_spec_with_report(&spec).unwrap().hid;
        assert_eq!(second, first, "the same entry, edited");
        assert_eq!(db.load_spec("dishes").unwrap().unwrap().tags, vec!["chore"]);
        let versions = crate::history::latest_version(db.conn(), &first).unwrap();
        assert_eq!(versions, Some(2), "create + the edit's pre-image");

        // An unchanged save writes nothing.
        db.save_field_spec(&spec).unwrap();
        assert_eq!(
            crate::history::latest_version(db.conn(), &first).unwrap(),
            Some(2)
        );
    }

    /// A malformed spec, a plain log of the reserved field, or a spec at
    /// the wrong slug is refused with the reason, never stored.
    #[test]
    fn a_write_under_the_spec_field_is_validated_as_a_spec() {
        use crate::error::DatumError;
        let db = Db::open_in_memory().unwrap();

        let plain = crate::test_util::entry_builder("spec").build();
        let err = db.insert_entry(&plain).unwrap_err();
        assert!(
            matches!(err, DatumError::InvalidFieldSpec(ref m) if m.contains("reserved")),
            "{err:?}"
        );

        let bad = super::new_spec_entry(
            "mood",
            serde_json::json!({"field": "mood", "verb": "pirouette"}),
            "d",
        );
        let err = db.insert_entry(&bad).unwrap_err();
        assert!(
            matches!(err, DatumError::InvalidFieldSpec(ref m) if m.contains("not a valid field spec")),
            "{err:?}"
        );

        let moved = super::new_spec_entry(
            "mood",
            serde_json::json!({"field": "sleep", "verb": "start"}),
            "d",
        );
        let err = db.insert_entry(&moved).unwrap_err();
        assert!(
            matches!(err, DatumError::InvalidFieldSpec(ref m) if m.contains("spec:sleep")),
            "{err:?}"
        );

        // An edit is checked too.
        db.save_field_spec(&FieldSpec::new("mood", Action::Switch))
            .unwrap();
        let mut row = db.get_entry("spec:mood").unwrap();
        row.data = Some(serde_json::json!({"field": "mood", "verb": 7}));
        assert!(db.save_existing_entry(&row).is_err());
        assert_eq!(db.load_spec("mood").unwrap().unwrap().verb, Action::Switch);
    }

    /// Sync stores a peer's rows unchecked, and a peer from before v38 can
    /// hold a plain log of the field `spec`. The readers pass over it.
    #[test]
    fn a_synced_plain_log_of_spec_is_not_read_as_a_spec() {
        let db = Db::open_in_memory().unwrap();
        db.save_field_spec(&FieldSpec::new("mood", Action::Switch))
            .unwrap();
        let plain = crate::test_util::entry_builder("spec").build();
        db.conn()
            .execute(
                "INSERT INTO entries (hid, slug, content_hash, field, occur_time, create_time,
                                      modify_time, client_id)
                 VALUES (?1, ?2, ?1, 'spec', '2026-09-01T00:00:00+00:00',
                         '2026-09-01T00:00:00Z', '2026-09-01T00:00:00Z', 'peer')",
                rusqlite::params![plain.hid, plain.slug],
            )
            .unwrap();

        let registry = db.load_registry().unwrap();
        assert_eq!(registry.fields.keys().collect::<Vec<_>>(), ["mood"]);
        assert_eq!(db.spec_fields().unwrap(), ["mood"]);
        assert!(db.get_all_state_colors().unwrap().is_empty());
        db.spec_alias_index().unwrap();
    }

    #[test]
    fn delete_is_a_tombstone_and_a_resave_revives_it() {
        let db = Db::open_in_memory().unwrap();
        let hid = db
            .save_field_spec_with_report(&FieldSpec::new("mood", Action::Switch))
            .unwrap()
            .hid;
        assert_eq!(db.delete_field_spec("mood").unwrap(), Some(hid.clone()));
        assert!(db.load_spec("mood").unwrap().is_none());
        assert_eq!(db.delete_field_spec("mood").unwrap(), None);

        let again = db
            .save_field_spec_with_report(&FieldSpec::new("mood", Action::Occur))
            .unwrap()
            .hid;
        assert_eq!(again, hid, "the tombstone is revived in place");
        assert_eq!(db.load_spec("mood").unwrap().unwrap().verb, Action::Occur);
    }

    #[test]
    fn list_specs_by_tag_filters_membership_not_substring() {
        let db = Db::open_in_memory().unwrap();
        let mut dishes = FieldSpec::new("dishes", Action::Occur);
        dishes.tags = vec!["chore".into()];
        let mut laundry = FieldSpec::new("laundry", Action::Occur);
        laundry.tags = vec!["chore".into(), "weekly".into()];
        let mood = FieldSpec::new("mood", Action::Switch);
        // `chores` (with trailing s) must NOT match a search for `chore`.
        let mut almost = FieldSpec::new("almost", Action::Occur);
        almost.tags = vec!["chores".into()];

        db.save_field_spec(&dishes).unwrap();
        db.save_field_spec(&laundry).unwrap();
        db.save_field_spec(&mood).unwrap();
        db.save_field_spec(&almost).unwrap();

        let hits = db.list_specs_by_tag("chore").unwrap();
        let names: Vec<&str> = hits.iter().map(|s| s.field.as_str()).collect();
        assert_eq!(names, vec!["dishes", "laundry"]);

        // Tag with no members → empty list.
        assert!(db.list_specs_by_tag("nonexistent").unwrap().is_empty());
    }

    // ─── Render visibility round-trips (da-2zd) ─────────────────────────────

    #[test]
    fn render_round_trips_unset_bool_and_per_surface() {
        let db = Db::open_in_memory().unwrap();
        db.save_field_spec(&FieldSpec::new("plain", Action::Occur))
            .unwrap();
        assert!(db
            .load_spec("plain")
            .unwrap()
            .unwrap()
            .presentation
            .render
            .is_none());

        let mut spec = FieldSpec::new("background_telemetry", Action::Occur);
        spec.presentation.render = Some(Render::All(false));
        db.save_field_spec(&spec).unwrap();
        let back = db.load_spec("background_telemetry").unwrap().unwrap();
        assert_eq!(back.presentation.render, Some(Render::All(false)));

        // Dropping it again clears it.
        spec.presentation.render = None;
        db.save_field_spec(&spec).unwrap();
        let back = db.load_spec("background_telemetry").unwrap().unwrap();
        assert!(back.presentation.render.is_none());

        let mut spec = FieldSpec::new("stretch_routine", Action::Occur);
        let mut map = BTreeMap::new();
        map.insert("dayview".to_string(), false);
        spec.presentation.render = Some(Render::PerSurface(map));
        db.save_field_spec(&spec).unwrap();
        let back = db.load_spec("stretch_routine").unwrap().unwrap();
        assert!(!back.presentation.renders_on("dayview"));
        assert!(back.presentation.renders_on("nowview"));
    }

    /// da-tlk2: a spec whose field name contains `:` is rejected at save
    /// time so it never reaches storage. The slug format `<field>:<id>`
    /// would otherwise become ambiguous when the doc_ref splitter walks it.
    #[test]
    fn save_field_spec_rejects_colon_in_field_name() {
        use crate::error::DatumError;
        let db = Db::open_in_memory().unwrap();
        let spec = FieldSpec::new("weird:field", Action::Occur);
        let err = db.save_field_spec(&spec).unwrap_err();
        assert!(
            matches!(err, DatumError::InvalidFieldSpec(_)),
            "expected InvalidFieldSpec, got {err:?}",
        );
    }
}

/// da-c8mh's compatibility half: a spec written before this bead is still
/// readable. The old key `default_verb` is an alias of `verb`, and the
/// verbless specs da-emze allowed (`"default_verb": null`) take the verb
/// their own log implies.
#[cfg(test)]
mod stored_verb_tests {
    use super::Db;
    use crate::action::Action;
    use crate::field_spec::FieldSpec;

    /// Store `spec_json` verbatim as `field`'s spec entry, past the write
    /// check — the shape an older datum may have left behind.
    fn store(db: &Db, field: &str, spec_json: &str) {
        db.save_field_spec(&FieldSpec::new(field, Action::Occur))
            .unwrap();
        db.conn()
            .execute(
                "UPDATE entries SET data = ?2 WHERE slug = ?1",
                rusqlite::params![super::spec_slug(field), spec_json],
            )
            .unwrap();
    }

    fn stored(db: &Db, field: &str) -> serde_json::Value {
        db.get_entry(&super::spec_slug(field))
            .unwrap()
            .data
            .unwrap()
    }

    #[test]
    fn the_old_key_is_read_as_the_verb() {
        let db = Db::open_in_memory().unwrap();
        store(
            &db,
            "activity",
            r#"{"field":"activity","default_verb":"switch","color":"blue"}"#,
        );

        let spec = db.load_spec("activity").unwrap().unwrap();
        assert_eq!(spec.verb, Action::Switch);
        assert_eq!(spec.color.as_deref(), Some("blue"));
        assert_eq!(db.field_verb("activity").unwrap(), Action::Switch);

        // Every other reader of a stored spec goes the same way.
        let batch = db.batch_load_specs(&["activity"]).unwrap();
        assert_eq!(batch["activity"].verb, Action::Switch);
        let registry = db.load_registry().unwrap();
        assert_eq!(
            registry.field_spec("activity").unwrap().verb,
            Action::Switch
        );

        // And a re-save writes the new key.
        db.save_field_spec(&spec).unwrap();
        let stored = stored(&db, "activity");
        assert_eq!(stored["verb"], "switch");
        assert!(stored["default_verb"].is_null());
    }

    #[test]
    fn a_verbless_spec_takes_the_verb_its_log_implies() {
        let db = Db::open_in_memory().unwrap();
        store(&db, "mood", r#"{"field":"mood","default_verb":null}"#);
        store(&db, "shape", r#"{"field":"shape"}"#);

        // No log yet: the fallback verb, and nothing errors.
        assert_eq!(db.load_spec("mood").unwrap().unwrap().verb, Action::Occur);

        crate::test_util::entry_builder("mood")
            .state(serde_json::json!("happy"))
            .insert(&db);
        crate::test_util::entry_builder("shape")
            .state(serde_json::json!(true))
            .insert(&db);

        assert_eq!(db.load_spec("mood").unwrap().unwrap().verb, Action::Switch);
        assert_eq!(db.load_spec("shape").unwrap().unwrap().verb, Action::Start);
    }

    #[test]
    fn a_spec_that_is_broken_for_another_reason_still_errors() {
        let db = Db::open_in_memory().unwrap();
        store(&db, "bad", r#"{"field":"bad","verb":"pirouette"}"#);
        assert!(db.load_spec("bad").is_err());
    }
}
