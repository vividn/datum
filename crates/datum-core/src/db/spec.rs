//! `field_specs` table: load/save/delete specs and the spec → derived alias
//! sync logic. Also `load_registry`, which materializes specs + aliases into
//! the in-memory [`SpecRegistry`].

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

    /// Save a field spec and sync its derived aliases into the aliases table.
    /// Any previously-derived aliases for this field are removed first (so
    /// states that were dropped lose their alias).
    ///
    /// Conflict policy:
    /// - **User aliases** (derived_from_field IS NULL) are never overwritten;
    ///   an auto-derived alias for the same name is dropped silently so the
    ///   user's override stands.
    /// - **Another spec's derived state alias** (`<state>` → `switch foo …`
    ///   from spec `foo`, where this spec is `bar`) stays with the original
    ///   owner — the later spec's conflicting state alias is skipped.
    /// - **Another spec's derived alias of non-State kind** (bare-field,
    ///   terminal-end) is cleared first via the prefix delete above; it
    ///   cannot collide with this spec's bare-field because field names are
    ///   unique PKs in `field_specs`.
    pub fn save_field_spec(&self, spec: &FieldSpec) -> Result<()> {
        let _ = self.save_field_spec_with_report(spec)?;
        Ok(())
    }

    /// Variant of [`Db::save_field_spec`] that returns a structured report of
    /// which derived aliases were saved and which were skipped due to
    /// conflicts. The caller can surface skipped entries to the user as
    /// warnings.
    pub fn save_field_spec_with_report(&self, spec: &FieldSpec) -> Result<SaveSpecReport> {
        let json = serde_json::to_string(spec)?;
        let tags_json = serde_json::to_string(&spec.tags)?;
        let tx = self.conn.unchecked_transaction()?;

        // UPSERT preserves an existing `kit_id` on re-save. Kit installs go
        // through `Db::install_kit` (which sets kit_id explicitly); a
        // user-driven `datum spec set` on a kit-owned field updates the JSON
        // body in place without breaking the kit linkage. `tags` mirrors
        // `spec.tags` so SQL filters can avoid a JSON walk.
        tx.execute(
            "INSERT INTO field_specs (field, spec_json, tags) VALUES (?1, ?2, ?3)
             ON CONFLICT(field) DO UPDATE SET spec_json = excluded.spec_json,
                                              tags      = excluded.tags",
            params![spec.field, json, tags_json],
        )?;

        // Replace the set of derived aliases for this field.
        tx.execute(
            "DELETE FROM aliases WHERE derived_from_field = ?1",
            params![spec.field],
        )?;

        let mut saved: Vec<String> = Vec::new();
        let mut skipped: Vec<AliasConflict> = Vec::new();

        for d in &spec.derived_aliases {
            // Look up any existing row with this alias name.
            let existing: Option<(String, Option<String>)> = tx
                .query_row(
                    "SELECT alias_json, derived_from_field FROM aliases WHERE name = ?1",
                    params![&d.state],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .optional()?;

            if let Some((_json, derived_from)) = existing {
                let reason = match derived_from {
                    None => AliasConflictReason::UserAlias,
                    Some(owner) if owner == spec.field => {
                        // Same-field re-save — safe to overwrite.
                        let alias = Alias {
                            name: d.state.clone(),
                            expands_to: d.expands_to.clone(),
                            description: Some(format!("Derived from {} spec", spec.field)),
                        };
                        let alias_json = serde_json::to_string(&alias)?;
                        tx.execute(
                            "INSERT OR REPLACE INTO aliases (name, alias_json, derived_from_field) VALUES (?1, ?2, ?3)",
                            params![alias.name, alias_json, spec.field],
                        )?;
                        saved.push(d.state.clone());
                        continue;
                    }
                    Some(other) => AliasConflictReason::OwnedByField(other),
                };
                skipped.push(AliasConflict {
                    name: d.state.clone(),
                    reason,
                });
                continue;
            }

            let alias = Alias {
                name: d.state.clone(),
                expands_to: d.expands_to.clone(),
                description: Some(format!("Derived from {} spec", spec.field)),
            };
            let alias_json = serde_json::to_string(&alias)?;
            tx.execute(
                "INSERT INTO aliases (name, alias_json, derived_from_field) VALUES (?1, ?2, ?3)",
                params![alias.name, alias_json, spec.field],
            )?;
            saved.push(d.state.clone());
        }

        tx.commit()?;
        Ok(SaveSpecReport { saved, skipped })
    }

    /// Delete a field spec. Cascades to its derived aliases.
    pub fn delete_field_spec(&self, field: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM aliases WHERE derived_from_field = ?1",
            params![field],
        )?;
        tx.execute("DELETE FROM field_specs WHERE field = ?1", params![field])?;
        tx.commit()?;
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
    use crate::entry::Verb;
    use crate::field_spec::FieldSpec;

    #[test]
    fn save_mirrors_tags_into_column() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("dishes", Verb::Occur);
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
        let mut dishes = FieldSpec::new("dishes", Verb::Occur);
        dishes.tags = vec!["chore".into()];
        let mut laundry = FieldSpec::new("laundry", Verb::Occur);
        laundry.tags = vec!["chore".into(), "weekly".into()];
        let mood = FieldSpec::new("mood", Verb::Switch);
        // `chores` (with trailing s) must NOT match a search for `chore` —
        // the LIKE prefilter would, but the Rust verifier rejects it.
        let mut almost = FieldSpec::new("almost", Verb::Occur);
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
}
