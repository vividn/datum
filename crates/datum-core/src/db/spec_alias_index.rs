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

use rusqlite::{Connection, OptionalExtension};

use crate::error::Result;
use crate::field_spec::{Alias, FieldSpec, SpecAliasIndex};

use super::spec::SPEC_DOC_SQL;
use super::Db;

/// Every derived alias name by the `spec` row carrying it, and the
/// triggers keeping it so (`spec_alias_names.sql`, da-ulu8f).
pub(crate) const SPEC_ALIAS_NAMES_SQL: &str = include_str!("spec_alias_names.sql");

/// Lay down `spec_alias_names` and fill it from every `spec` row, as its
/// triggers would have: for a file whose rows were written without them.
pub(crate) fn fill_spec_alias_names(conn: &Connection) -> Result<()> {
    conn.execute_batch(SPEC_ALIAS_NAMES_SQL)?;
    conn.execute_batch(
        "DELETE FROM spec_alias_names;
         INSERT OR IGNORE INTO spec_alias_names
             SELECT json_extract(e.data, a.fullkey || '.state') AS name, e.rowid
               FROM entries e,
                    json_each(CASE WHEN json_valid(e.data) THEN e.data END, '$.derived_aliases') a
              WHERE e.field = 'spec' AND name IS NOT NULL;",
    )?;
    Ok(())
}

impl Db {
    /// Borrow the lazy spec-derived alias index. Built on first access by
    /// walking every spec and pulling its `derived_aliases` into a single
    /// map. Cross-field collisions are resolved in the order the specs were
    /// first written — first registrant wins, later collisions are
    /// recorded under [`SpecAliasIndex::collisions`].
    ///
    /// The cache is stamped with the connection's `total_changes()`, so
    /// ANY write — `setup`, a kit install, or an `update` / `undo` of a
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

    /// The spec that wins the derived alias `name` in
    /// [`Db::spec_alias_index`]: the live spec carrying it with the lowest
    /// rowid. Read by name, so it costs the same however many specs there
    /// are (da-ulu8f).
    pub(crate) fn derived_alias_owner(&self, name: &str) -> Result<Option<FieldSpec>> {
        // A file below v63, as a test builds one, has no names to read.
        let laid: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name = 'spec_alias_names')",
            [],
            |row| row.get(0),
        )?;
        if !laid {
            let owner = self.spec_alias_index()?.owner_of(name).map(str::to_string);
            return owner.map_or(Ok(None), |field| self.load_spec(&field));
        }
        // CROSS JOIN: from the name to its rows, never the other way round.
        let json: Option<String> = self
            .conn
            .query_row(
                &format!(
                    "SELECT data FROM spec_alias_names CROSS JOIN entries
                      ON entries.rowid = spec_alias_names.spec
                     WHERE spec_alias_names.name = ?1 AND {SPEC_DOC_SQL}
                     ORDER BY spec_alias_names.spec LIMIT 1"
                ),
                [name],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|j| self.parse_spec(&j)).transpose()
    }

    /// The derived alias `name`, as [`Db::spec_alias_index`] has it.
    pub(crate) fn derived_alias(&self, name: &str) -> Result<Option<Alias>> {
        let owner = self.derived_alias_owner(name)?;
        Ok(SpecAliasIndex::from_specs(&owner).entries.remove(name))
    }
}

#[cfg(test)]
mod tests {
    use crate::action::Action;
    use crate::db::Db;
    use crate::field_spec::FieldSpec;

    fn spec(field: &str, verb: Action, states: &[&str]) -> FieldSpec {
        let mut spec = FieldSpec::new(field, verb);
        spec.allowed_states =
            (!states.is_empty()).then(|| states.iter().map(|s| s.to_string()).collect());
        spec.regenerate_derived_aliases();
        spec
    }

    /// Every name the whole index holds, and two it does not, read by
    /// name; and the table the triggers kept is the one a fill writes.
    fn agrees(db: &Db) {
        let index = db.spec_alias_index().unwrap().clone();
        let names = index
            .entries
            .keys()
            .map(String::as_str)
            .chain(["nothing", "moodx"]);
        for name in names {
            let by_name = db.derived_alias(name).unwrap();
            assert_eq!(by_name.as_ref(), index.entries.get(name), "{name}");
        }
        let rows = |db: &Db| -> Vec<(String, i64)> {
            let mut stmt = db
                .conn
                .prepare("SELECT name, spec FROM spec_alias_names")
                .unwrap();
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
            rows.map(Result::unwrap).collect()
        };
        let kept = rows(db);
        super::fill_spec_alias_names(&db.conn).unwrap();
        assert_eq!(rows(db), kept);
    }

    /// da-ulu8f: the word a command starts with is read by name, and reads
    /// what the index of every spec says, collisions (first saved wins)
    /// and all, through each write a spec takes.
    #[test]
    fn a_derived_alias_read_by_name_is_the_one_the_index_holds() {
        let db = Db::open_in_memory().unwrap();
        db.save_field_spec(&spec("mood", Action::Switch, &["happy", "sad"]))
            .unwrap();
        db.save_field_spec(&spec("feel", Action::Switch, &["sad", "calm"]))
            .unwrap();
        db.save_field_spec(&spec("tea", Action::Occur, &[]))
            .unwrap();
        agrees(&db);
        let owner = |name| db.derived_alias_owner(name).unwrap().map(|s| s.field);
        assert_eq!(owner("sad").as_deref(), Some("mood"));

        db.delete_field_spec("mood").unwrap();
        agrees(&db);
        assert_eq!(owner("sad").as_deref(), Some("feel"));

        // Revived in place, so saved first again.
        db.save_field_spec(&spec("mood", Action::Switch, &["sad"]))
            .unwrap();
        agrees(&db);
        assert_eq!(owner("sad").as_deref(), Some("mood"));

        db.save_field_spec(&spec("feel", Action::Start, &[]))
            .unwrap();
        agrees(&db);
        assert_eq!(owner("calm"), None);
        assert_eq!(owner("feele").as_deref(), Some("feel"));

        db.conn
            .execute("DELETE FROM entries WHERE slug = 'spec:tea'", [])
            .unwrap();
        agrees(&db);
        assert_eq!(owner("tea"), None);
    }

    /// A spec saved while another owns one of its names reports it skipped,
    /// naming the owner, as the whole index did.
    #[test]
    fn a_spec_save_reports_a_name_another_spec_owns() {
        let db = Db::open_in_memory().unwrap();
        db.save_field_spec(&spec("mood", Action::Switch, &["sad"]))
            .unwrap();
        let report = db
            .save_field_spec_with_report(&spec("feel", Action::Switch, &["sad"]))
            .unwrap();
        assert_eq!(report.saved, ["feel", "feele"]);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].name, "sad");
        assert_eq!(
            report.skipped[0].reason,
            crate::db::types::AliasConflictReason::OwnedByField("mood".into())
        );
    }
}
