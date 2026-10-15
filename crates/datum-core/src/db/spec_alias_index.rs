//! Lazy spec-derived alias index (da-8y2q.5).
//!
//! Pre-v21 each field spec wrote ~10 rows to the `aliases` table on every
//! save (bare-field, terminal-end, per-state). Reads always hit the same
//! table, so the cost was paid every save for the benefit of the read
//! path. Cross-spec state-name collisions (two specs both declaring state
//! `done`) silently raced on save order — the loser's row simply
//! disappeared.
//!
//! Derived aliases live only in the specs themselves — since v38 the
//! `spec` entries (da-lc26c). [`Db::spec_alias_index`] walks them on first
//! access, builds a `HashMap<name, Alias>`, and caches it until the next
//! write on the connection. Cross-spec collisions are recorded on the
//! index so the caller can render a warning instead of silently losing
//! one entry.

use std::cell::Ref;

use crate::error::Result;
use crate::field_spec::SpecAliasIndex;

use super::Db;

impl Db {
    /// Borrow the lazy spec-derived alias index. Built on first access by
    /// walking every spec and pulling its `derived_aliases` into a single
    /// map. Cross-field collisions are resolved in the order the specs were
    /// first written — first registrant wins, later collisions are
    /// recorded under [`SpecAliasIndex::collisions`].
    ///
    /// The cache is stamped with the connection's `total_changes()`, so
    /// ANY write — `spec`, a kit install, or an `update` / `undo` of a
    /// spec entry — rebuilds it on the next read. The returned [`Ref`] is
    /// valid until dropped; do not hold it across a write.
    pub fn spec_alias_index(&self) -> Result<Ref<'_, SpecAliasIndex>> {
        let stamp = self.conn.total_changes();
        let fresh = matches!(&*self.spec_alias_cache.borrow(), Some((s, _)) if *s == stamp);
        if !fresh {
            // Rowid order: an update keeps a row's rowid, so the spec
            // written first stays first across edits — the pre-v21
            // "first-save wins" tie-break.
            let specs = self.stored_specs("ORDER BY rowid")?;
            // The indexing itself is pure — the browser runs the same code
            // over the specs it holds (da-5kbj).
            *self.spec_alias_cache.borrow_mut() = Some((stamp, SpecAliasIndex::from_specs(&specs)));
        }
        Ok(Ref::map(self.spec_alias_cache.borrow(), |o| {
            &o.as_ref().expect("spec alias cache populated").1
        }))
    }
}
