//! A spec's symbol keys renamed (da-yc5vi): `glyph`, `state_glyphs`,
//! `glyph_by`, `glyph_scale` and `glyph_aspect` are `symbol` and the rest
//! ([`GLYPH_KEYS`]). The v72→v73 migration marks a file holding specs from
//! before (`_meta` `symbol_keys`); the open that can write then renames them
//! here.

use std::time::Duration;

use serde_json::Value;

use super::row_mapper::row_to_entry;
use super::spec::{SPEC_DISPLACED_KEY, SPEC_FIELD};
use super::{busy, Db, WriteBatch};
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::field_spec::GLYPH_KEYS;

impl Db {
    /// Each live spec holding an old key takes the new one, as an edit in
    /// place signed as an op, so every copy upgrading signs the same values
    /// and the fold reads no conflict between them. A row the save refuses
    /// keeps its old keys, which read as the new. With the lock another's,
    /// the marker stays and the next open goes on from what is left.
    pub(crate) fn rename_symbol_keys(&self) -> Result<()> {
        let marked = self
            .conn
            .prepare_cached("SELECT 1 FROM _meta WHERE key = 'symbol_keys'")?
            .exists([])?;
        if !marked || !self.can_write(Duration::from_millis(100))? {
            return Ok(());
        }
        let specs = self.read(|view| {
            let mut stmt = view.conn().prepare(
                "SELECT * FROM entries WHERE field = ?1 AND deleted_at IS NULL AND data IS NOT NULL",
            )?;
            let rows = stmt.query_map([SPEC_FIELD], row_to_entry)?;
            Ok::<_, DatumError>(rows.collect::<rusqlite::Result<Vec<Entry>>>()?)
        })?;
        for mut e in specs {
            if !e.data.as_mut().is_some_and(rename) {
                continue;
            }
            match self.save_existing_entry(&e) {
                Err(err) if busy(&err) => return Ok(()),
                _ => {}
            }
        }
        let mut done = WriteBatch::default();
        done.push("DELETE FROM _meta WHERE key = 'symbol_keys'", vec![]);
        self.write(&done).map(drop)
    }
}

/// `spec` with each old key under its new name, the spec it displaced too;
/// whether anything moved. A key both name keeps the new one's value, and a
/// state the new `state_symbols` lacks takes the old map's, as
/// [`crate::field_spec::PresentationSpec`] reads it.
fn rename(spec: &mut Value) -> bool {
    let Some(obj) = spec.as_object_mut() else {
        return false;
    };
    let mut moved = obj.get_mut(SPEC_DISPLACED_KEY).is_some_and(rename);
    for (old, new) in GLYPH_KEYS {
        let Some(v) = obj.remove(old) else { continue };
        moved = true;
        match (obj.get_mut(new), v) {
            (Some(Value::Object(kept)), Value::Object(was)) if new == "state_symbols" => {
                for (state, symbol) in was {
                    kept.entry(state).or_insert(symbol);
                }
            }
            (Some(_), _) => {}
            (None, v) => {
                obj.insert(new.into(), v);
            }
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::spec::new_spec_entry;
    use super::*;

    /// A spec stored with the old keys, a displaced one inside it too, takes
    /// the new names as the marked file opens, as one signed edit; a spec
    /// holding none is not written; the mark goes.
    #[test]
    fn a_marked_open_renames_the_stored_specs() {
        let db = Db::open_in_memory().unwrap();
        let old = json!({"field": "coffee", "verb": "occur", "glyph": "☕",
            "state_glyphs": {"big": "B"}, "glyph_by": {"key": "size", "map": {"s": "•"}},
            "glyph_scale": 2.0, "glyph_aspect": 0.5,
            "displaced": {"field": "coffee", "verb": "occur", "glyph": "●"}});
        db.insert_entry(&new_spec_entry("coffee", old, "d"))
            .unwrap();
        let both = json!({"field": "mate", "verb": "switch",
            "state_symbols": {"hot": "H"}, "state_glyphs": {"hot": "h", "cold": "c"}});
        db.insert_entry(&new_spec_entry("mate", both, "d")).unwrap();
        let new = json!({"field": "tea", "verb": "occur", "symbol": "○"});
        db.insert_entry(&new_spec_entry("tea", new.clone(), "d"))
            .unwrap();
        db.conn()
            .execute_batch("INSERT INTO _meta (key, value) VALUES ('symbol_keys', 'glyph')")
            .unwrap();

        db.rename_symbol_keys().unwrap();

        let coffee = db.spec_entry("coffee").unwrap().unwrap();
        assert_eq!(
            coffee.data.unwrap(),
            json!({"field": "coffee", "verb": "occur", "symbol": "☕",
                "state_symbols": {"big": "B"}, "symbol_by": {"key": "size", "map": {"s": "•"}},
                "symbol_scale": 2.0, "symbol_aspect": 0.5,
                "displaced": {"field": "coffee", "verb": "occur", "symbol": "●"}})
        );
        assert_eq!(
            db.spec_entry("mate").unwrap().unwrap().data.unwrap(),
            json!({"field": "mate", "verb": "switch", "state_symbols": {"hot": "H", "cold": "c"}})
        );
        let versions = |hid: &str| crate::history::latest_version(db.conn(), hid).unwrap();
        assert_eq!(
            versions(&coffee.hid),
            Some(2),
            "create + the rename's pre-image"
        );
        let tea = db.spec_entry("tea").unwrap().unwrap();
        assert_eq!(tea.data.unwrap(), new);
        assert_eq!(
            versions(&tea.hid),
            Some(1),
            "nothing to rename, nothing written"
        );
        let marked: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM _meta WHERE key = 'symbol_keys'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(marked, 0);
    }
}
