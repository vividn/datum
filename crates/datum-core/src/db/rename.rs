//! `datum rename OLD NEW` (da-ftls7): a field takes another name, its
//! entries, its spec and the aliases the spec derives with it.
//!
//! Each row moves as `update field=NEW` moves one: an edit of the row in
//! place, its hid kept and its slug re-derived as `NEW:<body>`, signed as an
//! op like any edit, so every copy folds to the same rows. The slug is not an
//! identity (the hid is), and an `OLD` entry logged later on a copy that has
//! seen the rename names what it saw in its `deps`, so it joins no renamed
//! row. The whole rename is one transaction and one action on the undo stack.
//!
//! `--merge` (da-icna7) lets NEW exist. An entry landing where a NEW entry
//! is, the same in all but when each was written, becomes one with it: the
//! younger is deleted. One that differs is a `restore` op's row
//! ([`sync::restore_row`]), a new hid, and its old row deleted: the fold's
//! `superseded_by` stands once set, so a row moved in by an edit could never
//! be moved out by an `undo`, while a restored row is taken back whole. The
//! fold then merges the two and names their conflicts, on every copy.

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::field_spec::{interpolate, Alias, InterpolationContext, DEFAULT_ID_DELIMITER};
use crate::history::ChangeKind;
use crate::ids::Slug;

use super::lookup::fetch_entry_any;
use super::row_mapper::row_to_entry;
use super::spec::is_meta_field;
use super::tombstone::{move_aside, soft_delete_in_tx};
use super::{sync, Db};

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
    /// `--merge`: the entries that were NEW's already in all but when each
    /// was written, the younger of each pair deleted.
    pub became_one: usize,
    /// `--merge`: the entries landing on a NEW one and saying otherwise.
    pub clashes: Vec<Clash>,
    /// `--merge`: `spec:OLD` went, NEW's staying, and these keys differed:
    /// each with OLD's value, then NEW's.
    pub spec_went: Option<Vec<(String, String, String)>>,
}

/// An entry of OLD that landed on one of NEW's and says otherwise.
#[derive(Debug)]
pub struct Clash {
    /// Its hid in OLD, deleted, which names `now`.
    pub hid: String,
    /// The row it is in NEW.
    pub now: String,
    /// NEW's entry, and where the two meet.
    pub with: String,
    pub slug: Slug,
    /// What the two say differently.
    pub keys: Vec<String>,
}

/// The writes a merge is, in their order.
#[derive(Default)]
struct Merge {
    /// Hids to delete, then those to move aside for a row that lands there.
    delete: Vec<String>,
    aside: Vec<String>,
    /// Each with the row it becomes, restored after it is deleted.
    clash: Vec<(Entry, Entry)>,
}

impl Db {
    /// Rename `old` to `new`; with `dry_run`, say what would move.
    /// Refuses a `new` that holds any row or a spec unless `merge` (no silent
    /// merge), a meta field, a field template, and a field its entries derive
    /// from a template (`issue__%id%`), which re-deriving would undo.
    pub fn rename_field(
        &self,
        old: &str,
        new: &str,
        dry_run: bool,
        merge: bool,
    ) -> Result<FieldRenamed> {
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
        // run does it all and rolls it back.
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
            let new_spec = self.spec_entry(new)?;
            if !merge && (self.field_has_entries(new)? || new_spec.is_some()) {
                return Err(DatumError::Other(format!(
                    "`{new}` already has entries or a spec (deleted ones count): \
                     to merge `{old}` into it, say --merge"
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
                became_one: 0,
                clashes: Vec::new(),
                spec_went: None,
            };
            let mut edits = Vec::with_capacity(rows.len() + 1);
            let mut plan = Merge::default();
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
                if merge
                    && e.superseded_by.is_none()
                    && self.lands(&mut e, &slug, &mut plan, &mut renamed)?
                {
                    continue;
                }
                edits.push((slug, e));
            }
            if let (Some(gone), Some(kept)) = (&spec, &new_spec) {
                renamed.spec_went = Some(spec_differences(gone, kept));
                plan.delete.push(gone.hid.to_string());
            } else if let Some(mut row) = spec {
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
                if merge {
                    let mut landed = row.clone();
                    landed.slug = Slug(super::spec::spec_slug(new));
                    tombstones_at(self, &landed, &mut plan)?;
                }
                edits.push((slug, row));
            }
            Ok((edits, plan, renamed))
        };
        let kind = ChangeKind::UserEdit;
        let (_, (_, renamed)) = self.save_edits_around(
            !dry_run,
            |view: &super::View| {
                let (edits, plan, renamed) = compute(view)?;
                Ok((edits, (plan, renamed)))
            },
            kind,
            |tx, (plan, _)| {
                for hid in &plan.delete {
                    soft_delete_in_tx(tx, hid, kind, None)?;
                }
                for hid in &plan.aside {
                    move_aside(tx, &fetch_entry_any(tx, hid)?, kind)?;
                }
                Ok(())
            },
            |tx, (plan, _)| {
                for (was, row) in &plan.clash {
                    let said = format!("renamed into {new} as {}", row.hid);
                    soft_delete_in_tx(tx, was.hid.as_str(), kind, Some(&said))?;
                    sync::restore_row(tx, row)?;
                }
                Ok(())
            },
        )?;
        Ok(renamed)
    }

    /// Where `e`, renamed, lands in a merge: `true` when that is settled and
    /// it takes no plain edit. A deleted row there moves aside.
    fn lands(
        &self,
        e: &mut Entry,
        was: &Slug,
        plan: &mut Merge,
        renamed: &mut FieldRenamed,
    ) -> Result<bool> {
        if let Some(tmpl) = e.v2_id_template_repaired() {
            e.id_structure = Some(tmpl.to_string());
        }
        let tmpl = e.id_structure.clone().unwrap_or_default();
        let ctx = InterpolationContext::from_entry(e);
        let body = interpolate(&tmpl, &ctx, DEFAULT_ID_DELIMITER);
        e.slug = Slug(format!("{}:{body}", e.field));
        tombstones_at(self, e, plan)?;
        let held = sync::blockers(&self.conn, e)?;
        let Some(onto) = held
            .iter()
            .filter(|h| h.deleted_at.is_none())
            .min_by_key(|h| h.slug != e.slug)
        else {
            return Ok(false);
        };
        // A row under it would stay behind, superseded by a deleted one.
        if self.rows_under(e)? {
            return Err(DatumError::Other(format!(
                "entry {} ({was}) has another row merged into it, and {} holds {} in `{}`: \
                 nothing was changed",
                e.hid, onto.hid, e.slug, e.field
            )));
        }
        let same_slug = onto.slug == e.slug;
        // One slug, one template: what it says next is what the entry it joins
        // would, and a side of no key. A row with none keeps its own, or its
        // save would leave the slug on OLD.
        if let Some(tmpl) = onto.id_structure.as_ref().filter(|_| same_slug) {
            e.id_structure = Some(tmpl.clone());
        }
        let keys = sync::differences(e, onto);
        if keys.is_empty() && same_slug && onto.conflicts.is_none() && !self.rows_under(onto)? {
            renamed.became_one += 1;
            if onto.create_time <= e.create_time {
                plan.delete.push(e.hid.to_string());
                return Ok(true);
            }
            plan.delete.push(onto.hid.to_string());
            plan.aside.push(onto.hid.to_string());
            return Ok(false);
        }
        let mut row = e.clone();
        row.hid = crate::entry::new_hid();
        row.content_hash = row.expected_content_hash();
        renamed.clashes.push(Clash {
            hid: e.hid.to_string(),
            now: row.hid.to_string(),
            with: onto.hid.to_string(),
            slug: e.slug.clone(),
            keys,
        });
        plan.clash.push((e.clone(), row));
        Ok(true)
    }

    fn rows_under(&self, e: &Entry) -> Result<bool> {
        Ok(self
            .conn
            .prepare_cached("SELECT 1 FROM entries WHERE superseded_by = ?1")?
            .exists([&e.hid])?)
    }
}

/// Plan moving aside each deleted row on the identity `e` is landing on.
fn tombstones_at(db: &Db, e: &Entry, plan: &mut Merge) -> Result<()> {
    for held in sync::blockers(&db.conn, e)? {
        if held.deleted_at.is_some() && !plan.aside.contains(&held.hid.0) {
            plan.aside.push(held.hid.0);
        }
    }
    Ok(())
}

/// The keys two specs give differently, each with `gone`'s value then
/// `kept`'s; `field` and what follows from it are no difference.
fn spec_differences(gone: &Entry, kept: &Entry) -> Vec<(String, String, String)> {
    let none = serde_json::Map::new();
    let of = |e: &Entry| e.data.as_ref().and_then(|d| d.as_object()).cloned();
    let (a, b) = (
        of(gone).unwrap_or_else(|| none.clone()),
        of(kept).unwrap_or(none),
    );
    let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    let say = |v: Option<&serde_json::Value>| v.map_or("nothing".to_string(), |v| v.to_string());
    keys.into_iter()
        .filter(|k| !matches!(k.as_str(), "field" | "derived_aliases") && a.get(*k) != b.get(*k))
        .map(|k| (k.clone(), say(a.get(k)), say(b.get(k))))
        .collect()
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
            let renamed = db.rename_field("mood", "feeling", false, false);
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
