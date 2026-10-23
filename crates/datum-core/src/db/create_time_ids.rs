//! A create-time id puts its `c` after the time (da-06jxa): `field:<create_time>c`,
//! so a field's ids sort in time order and a timeless entry sits right after
//! an occur entry of its instant. The v68→v69 migration marks a file holding
//! ids from before (`_meta` `create_time_ids`); the open that can write then
//! rewrites them here.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde_json::Value;

use crate::doc_ref::state_refs;
use crate::field_spec::{
    interpolate, FieldSpec, InterpolationContext, SubfieldType, DEFAULT_ID_DELIMITER,
};
use crate::shape::REF_MARK;

use super::row_mapper::row_to_entry;
use super::{busy, Db, WriteBatch};
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::ids::Slug;

impl Db {
    /// Each live entry whose id is its default or its template's with the `c`
    /// before the time ([`moved_id`]) takes the id with the `c` after it, as
    /// `rename` moves a row (da-ftls7): an edit in place, its hid kept, signed
    /// as an op, so a copy that has not upgraded folds to the same id; every
    /// copy upgrading signs the same value, and `slug` is no key a conflict is
    /// read on. Then each live entry whose state or data names an id so moved
    /// names its new one ([`carry_refs`]). It opens no undo action, and an id
    /// set by `--id`, `-K` or a literal is left as it is.
    ///
    /// Each row is its own write: one whose new id is taken keeps its old id,
    /// and one whose carried references will not save keeps them, each named
    /// in [`Db::create_time_ids_left`] for the CLI to say and by `datum check`
    /// (slug drift, a dangling reference), and the rest move. With the lock another's, the
    /// marker stays and the next open goes on from what is left.
    pub(crate) fn rewrite_create_time_ids(&self) -> Result<()> {
        let marked = self
            .conn
            .prepare_cached("SELECT 1 FROM _meta WHERE key = 'create_time_ids'")?
            .exists([])?;
        if !marked || !self.can_write(Duration::from_millis(100))? {
            return Ok(());
        }
        for e in self.live_rows()? {
            let Some(new) = moved_id(&e) else { continue };
            let mut moved = e.clone();
            moved.slug = Slug(new);
            match self.save_existing_entry_with_rekey(&e.slug, &moved) {
                Err(err) if busy(&err) => return Ok(()),
                Err(_) => self.create_time_ids_left.borrow_mut().push(e.slug.0),
                Ok(()) => {}
            }
        }
        // Read against every id in its new form, so an open that goes on from
        // one the lock stopped carries the references to the ids moved then.
        let rows = self.live_rows()?;
        let mut was: HashMap<String, String> = rows
            .iter()
            .filter_map(|e| {
                let old = c_before(e.slug.as_str(), &e.create_time);
                (old != e.slug.as_str()).then(|| (old, e.slug.0.clone()))
            })
            .collect();
        let mut specs: HashMap<String, Option<FieldSpec>> = HashMap::new();
        let mut seen: HashSet<String> = was.keys().cloned().collect();
        let mut rows = Some(rows);
        // A carried entry is saved as an edit is, so an id its template
        // derives from the reference moves too, and the references to that
        // are carried in the next pass, until a pass renames nothing new.
        while let Some(pass) = rows.take().filter(|_| !was.is_empty()) {
            let mut renamed = Vec::new();
            for mut e in pass {
                if !specs.contains_key(&e.field) {
                    specs.insert(e.field.clone(), self.load_spec(&e.field)?);
                }
                if !carry_refs(specs[&e.field].as_ref(), &mut e, &was) {
                    continue;
                }
                let slug = e.slug.clone();
                match self.save_existing_entry_with_auto_reinterp(&slug, e, DEFAULT_ID_DELIMITER) {
                    Err(err) if busy(&err) => return Ok(()),
                    Err(_) => self.create_time_ids_left.borrow_mut().push(slug.0),
                    Ok((fresh, Some(old))) => renamed.push((old.0, fresh.slug.0)),
                    Ok(_) => {}
                }
            }
            // Each old id once, so two entries whose ids derive from each
            // other's references stop.
            was = renamed
                .into_iter()
                .filter(|(old, _)| seen.insert(old.clone()))
                .collect();
            if !was.is_empty() {
                rows = Some(self.live_rows()?);
            }
        }
        let mut done = WriteBatch::default();
        done.push("DELETE FROM _meta WHERE key = 'create_time_ids'", vec![]);
        self.write(&done).map(drop)
    }

    fn live_rows(&self) -> Result<Vec<Entry>> {
        self.read(|view| {
            let mut stmt = view
                .conn()
                .prepare("SELECT * FROM entries WHERE deleted_at IS NULL")?;
            let rows = stmt.query_map([], row_to_entry)?;
            Ok::<_, DatumError>(rows.collect::<rusqlite::Result<Vec<Entry>>>()?)
        })
    }

    /// The entries this open's rewrite of create-time ids could not save
    /// (da-06jxa): their id, or a reference in them, keeps the `c` before the
    /// time.
    pub fn create_time_ids_left(&self) -> Vec<String> {
        self.create_time_ids_left.borrow().clone()
    }
}

/// `id` with the `c` before `create_time` its stamp: what `id` was, if it is
/// one with the `c` after.
fn c_before(id: &str, create_time: &chrono::DateTime<chrono::Utc>) -> String {
    let stamp = datum_parse::entry_id::render_stamp(create_time);
    id.replacen(&format!("{stamp}c"), &format!("c{stamp}"), 1)
}

/// The id `e` takes with its `c` after the time, if its id is the one its
/// template derives to now but for where the `c` stands — or, with no
/// template, the old default `field:c<create_time>` exactly (a v2 import's).
fn moved_id(e: &Entry) -> Option<String> {
    let stamp = datum_parse::entry_id::render_stamp(&e.create_time);
    let new = match e.id_structure.as_deref() {
        Some(tmpl) => {
            let ctx = InterpolationContext::from_entry(e);
            format!(
                "{}:{}",
                e.field,
                interpolate(tmpl, &ctx, DEFAULT_ID_DELIMITER)
            )
        }
        None => format!("{}:{stamp}c", e.field),
    };
    (new != e.slug.as_str() && c_before(&new, &e.create_time) == e.slug.as_str()).then_some(new)
}

/// Point `e`'s references to a moved id at its new one: a state's slugs
/// ([`state_refs`]: a string holding `:`), and in its data a `ref://` value
/// or a bare one under a key the spec types `ref`, as
/// [`crate::doc_ref::data_refs`] reads them. Whether anything moved.
fn carry_refs(spec: Option<&FieldSpec>, e: &mut Entry, was: &HashMap<String, String>) -> bool {
    let mut moved = false;
    for state in [&mut e.state, &mut e.following_state].into_iter().flatten() {
        if !state_refs(spec, state).iter().any(|r| was.contains_key(r)) {
            continue;
        }
        moved |= each_string(state, &mut |s| {
            was.get(s.as_str()).map(|n| *s = n.clone()).is_some()
        });
    }
    if let Some(Value::Object(data)) = &mut e.data {
        for (key, value) in data.iter_mut() {
            moved |= carry_data_refs(spec, key, value, was);
        }
    }
    moved
}

fn carry_data_refs(
    spec: Option<&FieldSpec>,
    key: &str,
    value: &mut Value,
    was: &HashMap<String, String>,
) -> bool {
    match value {
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |m, v| carry_data_refs(spec, key, v, was) | m),
        Value::Object(map) => map.iter_mut().fold(false, |m, (k, v)| {
            carry_data_refs(spec, &format!("{key}.{k}"), v, was) | m
        }),
        Value::String(s) => {
            let typed = spec.and_then(|sp| sp.subfields.get(key)).is_some_and(|sf| {
                matches!(sf.value_type, SubfieldType::Ref | SubfieldType::RefList)
            });
            let new = match s.strip_prefix(REF_MARK) {
                Some(slug) => was.get(slug).map(|n| format!("{REF_MARK}{n}")),
                None if typed => was.get(s.as_str()).cloned(),
                None => None,
            };
            new.map(|n| *s = n).is_some()
        }
        _ => false,
    }
}

/// Run `f` on each string in `v`, itself or an array's; whether any changed.
fn each_string(v: &mut Value, f: &mut impl FnMut(&mut String) -> bool) -> bool {
    match v {
        Value::String(s) => f(s),
        Value::Array(items) => items.iter_mut().fold(false, |m, i| each_string(i, f) | m),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::db::Db;
    use crate::entry::{Entry, Verb};
    use crate::ids::Slug;

    const MARK: &str = "INSERT INTO _meta (key, value) VALUES ('create_time_ids', 'c_before')";

    /// Log `field` with `tmpl` as its id template and the id `body` makes of
    /// its create time's stamp; its hid and stamp.
    fn mk(
        db: &Db,
        field: &str,
        tmpl: Option<&str>,
        body: &dyn Fn(&str) -> String,
    ) -> (String, String) {
        let mut e = Entry::new(field, Verb::Point, None, None, None, None, "d1");
        let stamp = datum_parse::entry_id::render_stamp(&e.create_time);
        e.id_structure = tmpl.map(Into::into);
        e.slug = Slug(format!("{field}:{}", body(&stamp)));
        db.insert_entry(&e).unwrap();
        (e.hid.0, stamp)
    }

    fn marked(db: &Db) -> bool {
        db.conn()
            .prepare("SELECT 1 FROM _meta WHERE key = 'create_time_ids'")
            .unwrap()
            .exists([])
            .unwrap()
    }

    /// The default id, a templated one and a v2 import's with no template
    /// move their `c`; an `--id`, and an id that drifted otherwise, stay; one
    /// whose new id is taken stays, is named, and holds nothing back; the
    /// references to a moved id follow it; the rows are what their ops fold
    /// to, and the marker goes.
    #[test]
    fn an_old_create_time_id_moves_its_c_after_the_time() {
        let db = Db::open_in_memory().unwrap();
        let (note, s1) = mk(&db, "y", Some("%create_time%"), &|s| format!("c{s}"));
        let (kept, _) = mk(&db, "book", Some("anon"), &|_| "anon".into());
        let (drifted, _) = mk(&db, "w", Some("%create_time%"), &|_| "other".into());
        let (blocked, s2) = mk(&db, "z", Some("%create_time%"), &|s| format!("c{s}"));
        mk(&db, "z", Some("taken"), &|_| format!("{s2}c"));
        let (tmpl, s3) = mk(&db, "issue", Some("x__%create_time%"), &|s| {
            format!("x__c{s}")
        });
        let (v2, s4) = mk(&db, "pc", None, &|s| format!("c{s}"));
        let mut by_state = Entry::new(
            "media",
            Verb::Point,
            Some(json!(format!("y:c{s1}"))),
            None,
            None,
            None,
            "d1",
        );
        by_state.data = Some(json!({"r": format!("ref://pc:c{s4}"), "s": format!("pc:c{s4}")}));
        db.insert_entry(&by_state).unwrap();
        db.conn().execute(MARK, []).unwrap();

        db.rewrite_create_time_ids().unwrap();

        let slug = |hid: &str| db.get_entry(hid).unwrap().slug.0;
        assert_eq!(slug(&note), format!("y:{s1}c"));
        assert_eq!(slug(&kept), "book:anon");
        assert_eq!(slug(&drifted), "w:other");
        assert_eq!(slug(&blocked), format!("z:c{s2}"));
        assert_eq!(slug(&tmpl), format!("issue:x__{s3}c"));
        assert_eq!(slug(&v2), format!("pc:{s4}c"));
        assert_eq!(db.create_time_ids_left(), [format!("z:c{s2}")]);
        let by_state = db.get_entry(&by_state.hid.0).unwrap();
        assert_eq!(by_state.state, Some(json!(format!("y:{s1}c"))));
        // A bare value under a key no spec types `ref` is no reference.
        assert_eq!(
            by_state.data,
            Some(json!({"r": format!("ref://pc:{s4}c"), "s": format!("pc:c{s4}")}))
        );
        let check = db.sync_fold_check().unwrap();
        assert!(check.stale.is_empty(), "{:?}", check.stale);
        assert!(!marked(&db));
    }

    /// Every form a reference takes follows the moved id: an array state, a
    /// key the spec types `ref` and `ref[]`, a nested `ref://`. An entry
    /// whose id its template derives from a reference moves with it, and a
    /// reference to that entry follows in turn; one whose carried id is
    /// taken keeps its reference and is named.
    #[test]
    fn references_follow_a_moved_id() {
        use crate::action::Action;
        use crate::field_spec::{FieldSpec, SubfieldSpec, SubfieldType};
        let db = Db::open_in_memory().unwrap();
        for (field, keys) in [
            ("task", &["b", "list"][..]),
            ("review", &["b"]),
            ("r2", &["b"]),
        ] {
            let mut spec = FieldSpec::new(field, Action::Occur);
            for k in keys {
                let value_type = if *k == "list" {
                    SubfieldType::RefList
                } else {
                    SubfieldType::Ref
                };
                spec.subfields.insert(
                    (*k).into(),
                    SubfieldSpec {
                        required: false,
                        value_type,
                        in_slug: false,
                        default: None,
                    },
                );
            }
            db.save_field_spec(&spec).unwrap();
        }
        let (_, s) = mk(&db, "y", Some("%create_time%"), &|s| format!("c{s}"));
        let (old, new) = (format!("y:c{s}"), format!("y:{s}c"));
        let log = |field: &str,
                   state: Option<serde_json::Value>,
                   data: serde_json::Value,
                   tmpl: &str,
                   body: String| {
            let mut e = Entry::new(field, Verb::Point, state, None, None, None, "d1");
            e.data = Some(data);
            e.id_structure = Some(tmpl.into());
            e.slug = Slug(format!("{field}:{body}"));
            db.insert_entry(&e).unwrap();
            e.hid.0
        };
        let task = log(
            "task",
            Some(json!([old, "plain"])),
            json!({"b": old, "list": [old, "x"], "n": {"deep": format!("ref://{old}")}}),
            "t1",
            "t1".into(),
        );
        let review = log(
            "review",
            None,
            json!({"b": old}),
            "rev_%b%",
            format!("rev_{old}"),
        );
        let watch = log(
            "watch",
            Some(json!(format!("review:rev_{old}"))),
            json!({}),
            "w1",
            "w1".into(),
        );
        let r2 = log("r2", None, json!({"b": old}), "%b%", old.clone());
        log("r2", None, json!({}), "taken", new.clone());
        db.conn().execute(MARK, []).unwrap();

        db.rewrite_create_time_ids().unwrap();

        let task = db.get_entry(&task).unwrap();
        assert_eq!(task.state, Some(json!([new, "plain"])));
        assert_eq!(
            task.data,
            Some(json!({"b": new, "list": [new, "x"], "n": {"deep": format!("ref://{new}")}}))
        );
        assert_eq!(
            db.get_entry(&review).unwrap().slug.0,
            format!("review:rev_{new}")
        );
        assert_eq!(
            db.get_entry(&watch).unwrap().state,
            Some(json!(format!("review:rev_{new}")))
        );
        assert_eq!(db.get_entry(&r2).unwrap().data, Some(json!({"b": old})));
        assert_eq!(db.create_time_ids_left(), [format!("r2:{old}")]);
        assert!(db.sync_fold_check().unwrap().stale.is_empty());
    }

    /// A file from before v69 migrates, rewrites as it opens, and a copy that
    /// held its old ids takes the new ones by syncing: the two hash alike.
    #[test]
    fn an_old_file_migrates_rewrites_and_syncs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.db");
        let path = path.to_str().unwrap();
        let other = Db::open_in_memory().unwrap();
        let (note, stamp, by) = {
            let db = Db::open(path).unwrap();
            let (note, stamp) = mk(&db, "y", Some("%create_time%"), &|s| format!("c{s}"));
            let by = Entry::new(
                "media",
                Verb::Point,
                Some(json!(format!("y:c{stamp}"))),
                None,
                None,
                None,
                "d1",
            );
            db.insert_entry(&by).unwrap();
            let ops = db.sync_ops_since(&Default::default(), false, None).unwrap();
            other.sync_apply(&ops.join("\n")).unwrap();
            db.conn()
                .execute(
                    "UPDATE _meta SET value = '68' WHERE key = 'schema_version'",
                    [],
                )
                .unwrap();
            (note, stamp, by.hid.0)
        };
        let db = Db::open(path).unwrap();
        assert_eq!(db.last_migration().unwrap().to_version, 69);
        assert!(!marked(&db));
        assert_eq!(db.get_entry(&note).unwrap().slug.0, format!("y:{stamp}c"));

        let ops = db
            .sync_ops_since(&other.sync_vector().unwrap(), false, None)
            .unwrap();
        other.sync_apply(&ops.join("\n")).unwrap();
        assert_eq!(
            other.get_entry(&note).unwrap().slug.0,
            format!("y:{stamp}c")
        );
        assert_eq!(
            other.get_entry(&by).unwrap().state,
            Some(json!(format!("y:{stamp}c")))
        );
        for copy in [&db, &other] {
            assert!(copy.sync_fold_check().unwrap().stale.is_empty());
        }
        assert_eq!(
            db.sync_state_hash(None).unwrap().0,
            other.sync_state_hash(None).unwrap().0
        );
    }
}
