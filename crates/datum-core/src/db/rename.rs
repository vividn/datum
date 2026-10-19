//! `datum rename OLD NEW` (da-ftls7): a field takes another name, its
//! entries, its spec and the aliases the spec derives with it.
//!
//! Each row moves as `update field=NEW` moves one: an edit of the row in
//! place, its hid kept and its slug re-derived as `NEW:<body>`, signed as an
//! op like any edit, so every copy folds to the same rows. The slug is not an
//! identity (the hid is), and an `OLD` entry logged later on a copy that has
//! seen the rename names what it saw in its `deps`, so it joins no renamed
//! row. The whole rename is one transaction and one action on the undo stack.

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::field_spec::{interpolate, Alias, InterpolationContext, DEFAULT_ID_DELIMITER};
use crate::history::ChangeKind;

use super::row_mapper::row_to_entry;
use super::spec::is_meta_field;
use super::Db;

/// What a rename moved, or with `dry_run` would move.
#[derive(Debug)]
pub struct FieldRenamed {
    /// The live entries of the field, now the new name's.
    pub entries: usize,
    /// Whether there was a spec, moved from `spec:OLD` to `spec:NEW`.
    pub spec: bool,
    /// The aliases the moved spec derives under the new name.
    pub derived: Vec<String>,
    /// The aliases whose expansion still names the old field: text, so
    /// they are listed and left as they are.
    pub naming_old: Vec<Alias>,
}

impl Db {
    /// Rename `old` to `new`; with `dry_run`, say what would move.
    /// Refuses a `new` that holds any row or a spec (no silent merge), a
    /// meta field, a field template, and a field its entries derive from a
    /// template (`issue -F id`), which re-deriving would undo.
    pub fn rename_field(&self, old: &str, new: &str, dry_run: bool) -> Result<FieldRenamed> {
        if old == new {
            return Err(DatumError::Other(format!("`{old}` is already called that")));
        }
        for f in [old, new] {
            if is_meta_field(f) {
                return Err(DatumError::Other(format!(
                    "`{f}` is reserved for datum's own documents"
                )));
            }
            if datum_parse::field_template::is_template(f) {
                return Err(DatumError::Other(format!(
                    "`{f}` is a field template, not a field"
                )));
            }
        }
        crate::slug::validate_field_name(new)?;
        crate::slug::validate_new_field_name(new)?;

        // Every row to save, the spec's too, is made before the lock; a dry
        // run reads it and saves nothing.
        let compute = |_: &super::View| {
            let mut stmt = self
                .conn
                .prepare("SELECT * FROM entries WHERE field = ?1 AND deleted_at IS NULL")?;
            let rows = stmt
                .query_map([old], row_to_entry)?
                .collect::<rusqlite::Result<Vec<Entry>>>()?;
            let spec = self.spec_entry(old)?;
            if rows.is_empty() && spec.is_none() {
                return Err(DatumError::NotFound(format!("no field `{old}`")));
            }
            if self.field_has_entries(new)? || self.spec_entry(new)?.is_some() {
                return Err(DatumError::Other(format!(
                    "`{new}` already has entries or a spec (deleted ones count): \
                     rename does not merge two fields"
                )));
            }
            if let Some(tmpl) = rows.iter().find_map(|e| e.field_structure.as_deref()) {
                return Err(DatumError::Other(format!(
                    "`{old}`'s entries take their field from the template `{tmpl}`: \
                     change the key it reads with `datum update` instead"
                )));
            }
            let mut renamed = FieldRenamed {
                entries: rows.len(),
                spec: spec.is_some(),
                derived: Vec::new(),
                naming_old: self
                    .list_user_aliases()?
                    .into_iter()
                    .filter(|a| names_field(&a.expands_to, old))
                    .collect(),
            };
            let mut edits = Vec::with_capacity(rows.len() + 1);
            for mut e in rows {
                let slug = e.slug.clone();
                e.field = new.to_string();
                // A row with no id template keeps its slug's body as a literal
                // one (`%` escaped, the template's one token), so its slug moves
                // with its field. The field holds no ':', so the first is the
                // boundary. A body the template would not give back verbatim
                // (interpolation collapses `__` runs) is refused, not changed.
                if e.id_structure.is_none() {
                    let body = slug.as_str().split_once(':').map_or("", |(_, b)| b);
                    let literal = body.replace('%', "\\%");
                    let ctx = InterpolationContext::from_entry(&e);
                    if interpolate(&literal, &ctx, DEFAULT_ID_DELIMITER) != body {
                        return Err(DatumError::Other(format!(
                            "entry {} ({slug}) has no id template, and its id would not \
                             survive the rename unchanged",
                            e.hid
                        )));
                    }
                    e.id_structure = Some(literal);
                }
                edits.push((slug, e));
            }
            if let Some(mut row) = spec {
                let slug = row.slug.clone();
                let data = row.data.as_mut().and_then(|d| d.as_object_mut());
                let data =
                    data.ok_or_else(|| DatumError::Other(format!("`{slug}` holds no spec")))?;
                let mut parsed = self.parse_spec(&serde_json::to_string(data)?)?;
                parsed.field = new.to_string();
                parsed.regenerate_derived_aliases();
                let derived = &parsed.derived_aliases;
                renamed.derived = derived.iter().map(|a| a.name.clone()).collect();
                data.insert("field".into(), new.into());
                data.insert("derived_aliases".into(), serde_json::to_value(derived)?);
                row.id_structure = Some(new.replace('%', "\\%"));
                edits.push((slug, row));
            }
            Ok((edits, renamed))
        };
        if dry_run {
            Ok(self.read(compute)?.1)
        } else {
            Ok(self.save_edits(compute, ChangeKind::UserEdit)?.1)
        }
    }
}

/// Does an alias expansion name `field`: as a word, in a comma list of
/// fields, or with a state (`mood:happy`)?
fn names_field(expansion: &str, field: &str) -> bool {
    datum_parse::shell_split(expansion).iter().any(|word| {
        word.split(',')
            .any(|w| w == field || w.strip_prefix(field).is_some_and(|r| r.starts_with(':')))
    })
}

#[cfg(test)]
mod tests {
    use super::names_field;
    use crate::db::Db;
    use crate::entry::{Entry, Verb};
    use crate::ids::Slug;

    /// A row with no id template moves its slug's body verbatim, `%` and
    /// all; one interpolation would not give back (a `__` run) is refused
    /// and nothing moves (canary review of da-ftls7).
    #[test]
    fn a_row_with_no_id_template_keeps_its_body_or_is_refused() {
        for (body, moves) in [("x%y", true), ("a____b", false)] {
            let db = Db::open_in_memory().unwrap();
            let mut e = Entry::new("mood", Verb::Point, None, None, None, None, "d1");
            e.slug = Slug(format!("mood:{body}"));
            assert!(e.id_structure.is_none());
            db.insert_entry(&e).unwrap();
            let renamed = db.rename_field("mood", "feeling", false);
            let slug = db.get_entry(&e.hid.0).unwrap().slug;
            if moves {
                renamed.unwrap();
                assert_eq!(slug.as_str(), format!("feeling:{body}"));
            } else {
                let err = renamed.unwrap_err().to_string();
                assert!(err.contains("would not survive the rename"), "{err}");
                assert_eq!(slug.as_str(), format!("mood:{body}"));
            }
        }
    }

    #[test]
    fn an_expansion_names_a_field_as_a_word_a_list_member_or_with_a_state() {
        for (expansion, names) in [
            ("switch mood happy", true),
            ("tail body,mood", true),
            ("tail mood:happy", true),
            ("switch moody happy", false),
            ("note moodlog mood=1", false),
        ] {
            assert_eq!(names_field(expansion, "mood"), names, "{expansion}");
        }
    }
}
