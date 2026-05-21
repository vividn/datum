//! `field_specs` table: load/save/delete specs and the read-side helpers
//! that surface them. Also `load_registry`, which materializes specs +
//! user/kit aliases + spec-derived aliases into the in-memory
//! [`SpecRegistry`].
//!
//! Derived aliases are no longer written to the `aliases` table — see
//! [`super::spec_alias_index`] (da-8y2q.5).

use rusqlite::{params, OptionalExtension};

use crate::error::{DatumError, Result};
use crate::field_spec::{Alias, FieldSpec, SpecRegistry};

use super::types::{AliasConflict, AliasConflictReason, SaveSpecReport};
use super::Db;

impl Db {
    /// Load a field spec by name. None if not defined.
    pub fn load_spec(&self, field: &str) -> Result<Option<FieldSpec>> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT spec_json FROM field_specs WHERE field = ?1",
                params![field],
                |row| row.get(0),
            )
            .optional()?;
        match json {
            Some(j) => Ok(Some(serde_json::from_str(&j)?)),
            None => Ok(None),
        }
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
        let sql =
            format!("SELECT field, spec_json FROM field_specs WHERE field IN ({placeholders})");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(fields.iter()), |row| {
            let field: String = row.get(0)?;
            let json: String = row.get(1)?;
            Ok((field, json))
        })?;
        for r in rows {
            let (field, json) = r.map_err(DatumError::Db)?;
            let spec: FieldSpec = serde_json::from_str(&json)?;
            out.insert(field, spec);
        }
        Ok(out)
    }

    /// Save a field spec. Derived aliases are not written to the
    /// `aliases` table — they live in `spec_json` and are surfaced
    /// lazily via [`Db::spec_alias_index`].
    pub fn save_field_spec(&self, spec: &FieldSpec) -> Result<()> {
        let _ = self.save_field_spec_with_report(spec)?;
        Ok(())
    }

    /// Variant of [`Db::save_field_spec`] that returns a structured
    /// report of which derived aliases this spec contributed to the
    /// effective alias namespace and which were shadowed by an existing
    /// user/kit alias row or by another spec's earlier registration in
    /// the [`super::types::SpecAliasIndex`].
    pub fn save_field_spec_with_report(&self, spec: &FieldSpec) -> Result<SaveSpecReport> {
        crate::slug::validate_field_name(&spec.field)?;
        let json = serde_json::to_string(spec)?;
        let tags_json = serde_json::to_string(&spec.tags)?;
        // da-2zd: NULL when the spec leaves render unset (i.e. visible
        // everywhere), otherwise the canonical JSON form of the Render
        // enum. The column shape lets us add a SQL-side visibility filter
        // later without re-parsing spec_json.
        let render_json: Option<String> = match &spec.presentation.render {
            Some(r) => Some(serde_json::to_string(r)?),
            None => None,
        };

        // UPSERT preserves an existing `kit_id` on re-save. Kit installs go
        // through `Db::install_kit` (which sets kit_id explicitly); a
        // user-driven `datum spec set` on a kit-owned field updates the JSON
        // body in place without breaking the kit linkage. `tags` mirrors
        // `spec.tags` so SQL filters can avoid a JSON walk; `render_json`
        // mirrors `spec.presentation.render` for the same reason.
        self.conn.execute(
            "INSERT INTO field_specs (field, spec_json, tags, render_json) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(field) DO UPDATE SET spec_json   = excluded.spec_json,
                                              tags        = excluded.tags,
                                              render_json = excluded.render_json",
            params![spec.field, json, tags_json, render_json],
        )?;
        self.invalidate_spec_alias_index();

        // Compute the report by inspecting the now-current state of the
        // alias namespace. A derived alias is "saved" iff (a) no user/kit
        // row in `aliases` shadows it and (b) the spec-alias index attributes
        // the name to this field. Otherwise it is shadowed and we attach
        // the cause.
        let mut saved: Vec<String> = Vec::new();
        let mut skipped: Vec<AliasConflict> = Vec::new();
        let index = self.spec_alias_index()?;
        for d in &spec.derived_aliases {
            // Shadowed by a user/kit alias row?
            let existing_row: Option<i64> = self
                .conn
                .query_row(
                    "SELECT 1 FROM aliases WHERE name = ?1",
                    params![&d.state],
                    |row| row.get(0),
                )
                .optional()?;
            if existing_row.is_some() {
                skipped.push(AliasConflict {
                    name: d.state.clone(),
                    reason: AliasConflictReason::UserAlias,
                });
                continue;
            }
            // Cross-spec collision: index points the name at a different field.
            match index.by_field.get(&spec.field) {
                Some(names) if names.iter().any(|n| n == &d.state) => {
                    saved.push(d.state.clone());
                }
                _ => {
                    let owner = index
                        .collisions
                        .iter()
                        .find(|c| c.name == d.state && c.loser == spec.field)
                        .map(|c| c.winner.clone())
                        .unwrap_or_else(|| spec.field.clone());
                    skipped.push(AliasConflict {
                        name: d.state.clone(),
                        reason: AliasConflictReason::OwnedByField(owner),
                    });
                }
            }
        }
        drop(index);
        Ok(SaveSpecReport { saved, skipped })
    }

    /// Delete a field spec. Derived aliases are not stored as rows, so
    /// no alias-table cleanup is required — invalidating the spec-alias
    /// index ensures subsequent reads no longer surface them.
    pub fn delete_field_spec(&self, field: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM field_specs WHERE field = ?1", params![field])?;
        self.invalidate_spec_alias_index();
        Ok(())
    }

    /// Return field specs whose `tags` array contains `tag`. The `tags`
    /// column stores the canonical JSON form (e.g. `["chore","daily"]`); a
    /// coarse LIKE prefilter narrows the scan, then Rust-side `has_tag`
    /// verifies (so a tag substring like `"chore"` doesn't match a tag named
    /// `"chores"`).
    pub fn list_specs_by_tag(&self, tag: &str) -> Result<Vec<FieldSpec>> {
        let needle = format!("%{tag}%");
        let mut stmt = self
            .conn
            .prepare("SELECT spec_json FROM field_specs WHERE tags LIKE ?1 ORDER BY field")?;
        let rows = stmt.query_map(params![needle], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            let spec: FieldSpec = serde_json::from_str(&row?)?;
            if spec.has_tag(tag) {
                out.push(spec);
            }
        }
        Ok(out)
    }

    pub fn load_registry(&self) -> Result<SpecRegistry> {
        let mut registry = SpecRegistry::new();

        let mut stmt = self.conn.prepare("SELECT spec_json FROM field_specs")?;
        let specs = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for spec_json in specs {
            let spec: FieldSpec = serde_json::from_str(&spec_json?)?;
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

        let mut stmt = self.conn.prepare("SELECT alias_json FROM aliases")?;
        let aliases = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for alias_json in aliases {
            let alias: Alias = serde_json::from_str(&alias_json?)?;
            registry.register_alias(alias);
        }

        Ok(registry)
    }
}

#[cfg(test)]
mod tag_tests {
    use super::Db;
    use crate::action::Action;
    use crate::field_spec::{FieldSpec, Render};
    use std::collections::HashMap;

    #[test]
    fn save_mirrors_tags_into_column() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("dishes", Action::Occur);
        spec.tags = vec!["chore".into(), "daily".into()];
        db.save_field_spec(&spec).unwrap();

        let stored: String = db
            .conn()
            .query_row(
                "SELECT tags FROM field_specs WHERE field = 'dishes'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, r#"["chore","daily"]"#);

        // Re-saving with a different tag set updates the column.
        let mut spec2 = spec.clone();
        spec2.tags = vec!["chore".into()];
        db.save_field_spec(&spec2).unwrap();
        let stored2: String = db
            .conn()
            .query_row(
                "SELECT tags FROM field_specs WHERE field = 'dishes'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored2, r#"["chore"]"#);
    }

    #[test]
    fn list_specs_by_tag_filters_membership_not_substring() {
        let db = Db::open_in_memory().unwrap();
        let mut dishes = FieldSpec::new("dishes", Action::Occur);
        dishes.tags = vec!["chore".into()];
        let mut laundry = FieldSpec::new("laundry", Action::Occur);
        laundry.tags = vec!["chore".into(), "weekly".into()];
        let mood = FieldSpec::new("mood", Action::Switch);
        // `chores` (with trailing s) must NOT match a search for `chore` —
        // the LIKE prefilter would, but the Rust verifier rejects it.
        let mut almost = FieldSpec::new("almost", Action::Occur);
        almost.tags = vec!["chores".into()];

        db.save_field_spec(&dishes).unwrap();
        db.save_field_spec(&laundry).unwrap();
        db.save_field_spec(&mood).unwrap();
        db.save_field_spec(&almost).unwrap();

        let mut hits = db.list_specs_by_tag("chore").unwrap();
        hits.sort_by(|a, b| a.field.cmp(&b.field));
        let names: Vec<&str> = hits.iter().map(|s| s.field.as_str()).collect();
        assert_eq!(names, vec!["dishes", "laundry"]);

        // Tag with no members → empty list.
        assert!(db.list_specs_by_tag("nonexistent").unwrap().is_empty());
    }

    // ─── Render visibility tests (da-2zd) ────────────────────────────────────

    #[test]
    fn save_leaves_render_json_null_when_render_is_none() {
        let db = Db::open_in_memory().unwrap();
        let spec = FieldSpec::new("plain", Action::Occur);
        db.save_field_spec(&spec).unwrap();

        let stored: Option<String> = db
            .conn()
            .query_row(
                "SELECT render_json FROM field_specs WHERE field = 'plain'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            stored.is_none(),
            "render_json must be NULL for default-visible specs, got {stored:?}",
        );

        // Round-trip: load_spec returns a None render.
        let back = db.load_spec("plain").unwrap().unwrap();
        assert!(back.presentation.render.is_none());
    }

    #[test]
    fn save_mirrors_render_bool_into_column() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("background_telemetry", Action::Occur);
        spec.presentation.render = Some(Render::All(false));
        db.save_field_spec(&spec).unwrap();

        let stored: Option<String> = db
            .conn()
            .query_row(
                "SELECT render_json FROM field_specs WHERE field = 'background_telemetry'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored.as_deref(), Some("false"));

        let back = db.load_spec("background_telemetry").unwrap().unwrap();
        assert_eq!(back.presentation.render, Some(Render::All(false)));
    }

    #[test]
    fn save_mirrors_per_surface_render_into_column() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("stretch_routine", Action::Occur);
        let mut map = HashMap::new();
        map.insert("dayview".to_string(), false);
        spec.presentation.render = Some(Render::PerSurface(map));
        db.save_field_spec(&spec).unwrap();

        let stored: Option<String> = db
            .conn()
            .query_row(
                "SELECT render_json FROM field_specs WHERE field = 'stretch_routine'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        // Object form serializes as a JSON object; we don't pin the exact
        // ordering since HashMap iteration is non-deterministic.
        let stored = stored.expect("render_json should be set");
        assert!(stored.contains("\"dayview\":false"), "stored = {stored}");

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

    #[test]
    fn re_save_clears_render_json_when_spec_drops_render() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("toggle", Action::Occur);
        spec.presentation.render = Some(Render::All(false));
        db.save_field_spec(&spec).unwrap();

        // Drop render and re-save — column must reset to NULL.
        spec.presentation.render = None;
        db.save_field_spec(&spec).unwrap();

        let stored: Option<String> = db
            .conn()
            .query_row(
                "SELECT render_json FROM field_specs WHERE field = 'toggle'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored.is_none(), "expected NULL after drop, got {stored:?}");
    }
}
