//! Lazy spec-derived alias index (da-8y2q.5).
//!
//! Pre-v21 each field spec wrote ~10 rows to the `aliases` table on every
//! save (bare-field, terminal-end, per-state). Reads always hit the same
//! table, so the cost was paid every save for the benefit of the read
//! path. Cross-spec state-name collisions (two specs both declaring state
//! `done`) silently raced on save order — the loser's row simply
//! disappeared.
//!
//! From v21 derived aliases live exclusively in `field_specs.spec_json`;
//! [`Db::spec_alias_index`] walks the rows on first access, builds a
//! `HashMap<name, Alias>`, and caches it. Any mutation of `field_specs`
//! clears the cache via [`Db::invalidate_spec_alias_index`]. Cross-spec
//! collisions are recorded on the index so the caller can render a
//! warning instead of silently losing one entry.

use std::cell::Ref;
use std::collections::HashMap;

use crate::error::Result;
use crate::field_spec::{Alias, FieldSpec};

use super::types::{SpecAliasCollision, SpecAliasIndex};
use super::Db;

impl Db {
    /// Borrow the lazy spec-derived alias index. Built on first access by
    /// scanning every row of `field_specs` and pulling each spec's
    /// `derived_aliases` into a single map. Cross-field collisions are
    /// resolved in `field`-name lexicographic order — first registrant
    /// wins, later collisions are recorded under
    /// [`SpecAliasIndex::collisions`].
    ///
    /// The returned [`Ref`] is valid until dropped; callers should not
    /// hold it across calls that mutate `field_specs`.
    pub fn spec_alias_index(&self) -> Result<Ref<'_, SpecAliasIndex>> {
        if self.spec_alias_cache.borrow().is_none() {
            let built = build_spec_alias_index(self)?;
            *self.spec_alias_cache.borrow_mut() = Some(built);
        }
        Ok(Ref::map(self.spec_alias_cache.borrow(), |o| {
            // Safe: we just populated the cell above.
            o.as_ref().expect("spec alias cache populated")
        }))
    }

    /// Drop the cached spec-alias index. Mutating operations on
    /// `field_specs` (`save_field_spec`, `delete_field_spec`,
    /// `install_kit`, `uninstall_kit`) call this so the next read
    /// rebuilds.
    pub(crate) fn invalidate_spec_alias_index(&self) {
        *self.spec_alias_cache.borrow_mut() = None;
    }
}

fn build_spec_alias_index(db: &Db) -> Result<SpecAliasIndex> {
    // Walk specs in insertion order (rowid). UPSERT preserves rowid on
    // re-save, so a spec that was saved first stays first-in across edits.
    // This keeps the pre-v21 "first-save wins" tie-break for state-name
    // collisions; later registrants are recorded as `collisions`.
    let mut stmt = db
        .conn()
        .prepare("SELECT spec_json FROM field_specs ORDER BY rowid")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

    let mut entries: HashMap<String, Alias> = HashMap::new();
    let mut by_field: HashMap<String, Vec<String>> = HashMap::new();
    let mut entry_owner: HashMap<String, String> = HashMap::new();
    let mut collisions: Vec<SpecAliasCollision> = Vec::new();

    for row in rows {
        let json = row?;
        let spec: FieldSpec = serde_json::from_str(&json)?;
        for d in &spec.derived_aliases {
            if let Some(existing_owner) = entry_owner.get(&d.name) {
                collisions.push(SpecAliasCollision {
                    name: d.name.clone(),
                    winner: existing_owner.clone(),
                    loser: spec.field.clone(),
                });
                continue;
            }
            let alias = Alias {
                name: d.name.clone(),
                expands_to: d.expands_to.clone(),
                description: Some(format!("Derived from {} spec", spec.field)),
            };
            entries.insert(d.name.clone(), alias);
            entry_owner.insert(d.name.clone(), spec.field.clone());
            by_field
                .entry(spec.field.clone())
                .or_default()
                .push(d.name.clone());
        }
    }

    Ok(SpecAliasIndex {
        entries,
        by_field,
        collisions,
    })
}
