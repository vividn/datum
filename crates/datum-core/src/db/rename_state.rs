//! `datum rename FIELD --state OLD NEW` (da-zbkqb): a state of one field
//! takes another name, in its entries, its spec and the aliases the spec
//! derives from it.
//!
//! Each entry holding OLD — as its state, in a multi-state list, as the
//! state it leaves (`following_state`), the one `-l` said it left or the
//! one an `end -l` closed (`data.last_state`) — is
//! edited in place, its hid kept, signed as an op like any edit. The spec's
//! row is edited as JSON, so a key this build does not read stays. The
//! whole rename is one transaction and one action on the undo stack.

use serde_json::{Map, Value};

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::field_spec::Alias;
use crate::history::ChangeKind;

use super::rename::names_field;
use super::row_mapper::row_to_entry;
use super::spec::is_meta_field;
use super::Db;

/// The spec's lists of states, which keep their order.
const LISTS: [&str; 2] = ["allowed_states", "archived"];
/// The spec's maps keyed by state (`state_symbols` is presentation's,
/// flattened into the same object).
const MAPS: [&str; 5] = [
    "state_colors",
    "state_descriptions",
    "state_keys",
    "state_assign",
    "state_symbols",
];

/// What a state rename changed, or with `dry_run` would change.
#[derive(Debug, Default)]
pub struct StateRenamed {
    /// The live entries that held OLD and now hold NEW.
    pub entries: usize,
    /// The spec keys that named OLD: the lists, then the state-keyed maps.
    pub spec_keys: Vec<String>,
    /// The alias the spec derived from OLD, now derived from NEW.
    pub alias: bool,
    /// The aliases you wrote whose expansion names OLD: text, so they are
    /// listed and left as they are.
    pub naming_old: Vec<Alias>,
    /// The field the states name documents of (`state_lookup_field`): its
    /// `OLD` document is not renamed.
    pub lookup: Option<String>,
    /// `--merge`: the keys of a state map where OLD and NEW both said
    /// something, NEW's kept: the map, OLD's value, NEW's.
    pub differs: Vec<(String, String, String)>,
}

impl Db {
    /// Rename `field`'s state `old` to `new`; with `dry_run`, say what would
    /// change. Refuses `true` or `false` on either side, a `new` a log would
    /// not read back as this state, an `old` the field does not know, and a
    /// `new` the field already knows (a deleted entry's too) unless `merge`.
    pub fn rename_state(
        &self,
        field: &str,
        old: &str,
        new: &str,
        dry_run: bool,
        merge: bool,
    ) -> Result<StateRenamed> {
        let refuse = |s: String| Err(DatumError::Other(s));
        if is_meta_field(field) {
            return refuse(format!("`{field}` is reserved for datum's own documents"));
        }
        if datum_parse::field_template::is_template(field) {
            return refuse(format!("`{field}` is a field template, not a field"));
        }
        for s in [old, new] {
            if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("false") {
                return refuse(format!(
                    "`{s}` is on or off, not a name: only a named state is renamed"
                ));
            }
        }
        if old == new {
            return refuse(format!("`{old}` is already called that"));
        }
        if let Some(why) = not_read_as_a_state(new) {
            return refuse(format!(
                "`{new}` would not read back as a state: a log takes it as {why}"
            ));
        }

        let compute = |_: &super::View| {
            let rows = self
                .conn
                .prepare("SELECT * FROM entries WHERE field = ?1")?
                .query_map([field], row_to_entry)?
                .collect::<rusqlite::Result<Vec<Entry>>>()?;
            let spec = self.spec_entry(field)?;
            if rows.is_empty() && spec.is_none() {
                return Err(DatumError::NotFound(format!("no field `{field}`")));
            }
            let spec_data = spec.as_ref().and_then(|e| e.data.as_ref());
            let spec_has = |s: &str| spec_data.is_some_and(|d| spec_names(d, s));
            let live = |e: &&Entry| e.deleted_at.is_none();
            if !spec_has(old) && !rows.iter().filter(live).any(|e| entry_holds(e, old)) {
                return Err(DatumError::NotFound(format!(
                    "`{field}` has no state `{old}`"
                )));
            }
            if !merge && (spec_has(new) || rows.iter().any(|e| entry_holds(e, new))) {
                return Err(DatumError::Other(format!(
                    "`{new}` is already a state of `{field}` (deleted entries count): \
                     to merge `{old}` into it, say --merge"
                )));
            }
            let mut renamed = StateRenamed {
                naming_old: self
                    .list_user_aliases()?
                    .into_iter()
                    .filter(|a| names_state(&a.expands_to, field, old))
                    .collect(),
                lookup: self.load_spec(field)?.and_then(|s| s.state_lookup_field),
                ..StateRenamed::default()
            };
            let mut edits = Vec::new();
            for mut e in rows.into_iter().filter(|e| e.deleted_at.is_none()) {
                let mut moved = false;
                for v in places_mut(&mut e) {
                    moved |= rename_value(v, old, new);
                }
                if moved {
                    renamed.entries += 1;
                    edits.push((e.slug.clone(), e));
                }
            }
            if let Some(mut row) = spec {
                let slug = row.slug.clone();
                let data = row.data.as_mut().and_then(Value::as_object_mut);
                let data =
                    data.ok_or_else(|| DatumError::Other(format!("`{slug}` holds no spec")))?;
                if rename_in_spec(data, old, new, &mut renamed) {
                    let mut parsed = self.parse_spec(&serde_json::to_string(data)?)?;
                    let had = parsed.derived_aliases.iter().any(|a| a.name == old);
                    parsed.regenerate_derived_aliases();
                    let derived = &parsed.derived_aliases;
                    renamed.alias = had && derived.iter().any(|a| a.name == new);
                    data.insert("derived_aliases".into(), serde_json::to_value(derived)?);
                    edits.push((slug, row));
                }
            }
            Ok((edits, renamed))
        };
        let (_, renamed) = self.save_edits_around(
            !dry_run,
            compute,
            ChangeKind::UserEdit,
            |_, _| Ok(()),
            |_, _| Ok(()),
        )?;
        Ok(renamed)
    }
}

/// Why a log would not read `word` as the named state it is, if it would not.
fn not_read_as_a_state(word: &str) -> Option<String> {
    use datum_parse::alias::expand::SWITCH_PROMOTE_VERBS;
    if word.is_empty() {
        return Some("nothing".into());
    }
    if SWITCH_PROMOTE_VERBS.contains(&word) {
        return Some(format!("the command `{word}`"));
    }
    if word.starts_with('-') {
        return Some("a flag".into());
    }
    if word.contains('=') {
        return Some("a key and its value".into());
    }
    if word == crate::shape::IGNORED_STATE_VALUE {
        return Some("the IGNORED mark, which ignores the field".into());
    }
    match datum_parse::input::infer_type(word) {
        Some(Value::String(s)) if s == word => None,
        Some(Value::Array(_)) => Some("a list of states".into()),
        Some(Value::Number(_)) => Some("a number".into()),
        Some(Value::Null) | None => Some("no state".into()),
        Some(v) => Some(v.to_string()),
    }
}

/// Does a state value hold `name`: itself, in a list, or as an object's id?
fn holds(v: &Value, name: &str) -> bool {
    match v {
        Value::String(s) => s == name,
        Value::Number(n) => n.to_string() == name,
        Value::Array(items) => items.iter().any(|i| holds(i, name)),
        Value::Object(o) => o.get("id").is_some_and(|id| holds(id, name)),
        _ => false,
    }
}

/// Where an entry stores a state: its own, the one it leaves, the one `-l`
/// said it left, and the closed block's an `end -l` keeps as `last_state`.
fn places(e: &Entry) -> impl Iterator<Item = &Value> {
    let last = e.data.as_ref().and_then(|d| d.get("last_state"));
    [
        e.state.as_ref(),
        e.following_state.as_ref(),
        e.user_asserted_last_state.as_ref(),
        last,
    ]
    .into_iter()
    .flatten()
}

fn places_mut(e: &mut Entry) -> impl Iterator<Item = &mut Value> {
    let last = e.data.as_mut().and_then(|d| d.get_mut("last_state"));
    [
        e.state.as_mut(),
        e.following_state.as_mut(),
        e.user_asserted_last_state.as_mut(),
        last,
    ]
    .into_iter()
    .flatten()
}

fn entry_holds(e: &Entry, name: &str) -> bool {
    places(e).any(|v| holds(v, name))
}

/// Rename `old` to `new` wherever `v` holds it; a list holding both keeps
/// one, and a list left with one member is that member. Whether anything
/// changed.
fn rename_value(v: &mut Value, old: &str, new: &str) -> bool {
    match v {
        Value::String(_) | Value::Number(_) if holds(v, old) => {
            *v = Value::String(new.into());
            true
        }
        Value::Array(items) => {
            let mut moved = false;
            for i in items.iter_mut() {
                moved |= rename_value(i, old, new);
            }
            if moved {
                let mut seen = Vec::new();
                items.retain(|i| {
                    !seen.contains(i) && {
                        seen.push(i.clone());
                        true
                    }
                });
                // A list of one is that state, as a log stores it.
                if let [one] = &mut items[..] {
                    *v = one.take();
                }
            }
            moved
        }
        Value::Object(o) => o.get_mut("id").is_some_and(|id| rename_value(id, old, new)),
        _ => false,
    }
}

/// Does the spec's JSON name the state `name` in a list or a map?
fn spec_names(data: &Value, name: &str) -> bool {
    let listed = LISTS.iter().filter_map(|k| data.get(k)?.as_array());
    let mapped = MAPS.iter().filter_map(|k| data.get(k)?.as_object());
    listed.flatten().any(|s| s.as_str() == Some(name))
        || mapped.into_iter().any(|m| m.contains_key(name))
}

/// Rename `old` to `new` in the spec's lists and maps, NEW's word winning
/// in a map that has both. Whether anything changed.
fn rename_in_spec(
    data: &mut Map<String, Value>,
    old: &str,
    new: &str,
    renamed: &mut StateRenamed,
) -> bool {
    for key in LISTS {
        let Some(list) = data.get_mut(key).and_then(Value::as_array_mut) else {
            continue;
        };
        let Some(at) = list.iter().position(|s| s.as_str() == Some(old)) else {
            continue;
        };
        if list.iter().any(|s| s.as_str() == Some(new)) {
            list.remove(at);
        } else {
            list[at] = new.into();
        }
        renamed.spec_keys.push(key.into());
    }
    for key in MAPS {
        let Some(map) = data.get_mut(key).and_then(Value::as_object_mut) else {
            continue;
        };
        let Some(was) = map.remove(old) else {
            continue;
        };
        match map.get(new) {
            Some(kept) if *kept != was => {
                renamed
                    .differs
                    .push((key.into(), was.to_string(), kept.to_string()));
            }
            Some(_) => {}
            None => {
                map.insert(new.into(), was);
            }
        }
        renamed.spec_keys.push(key.into());
    }
    !renamed.spec_keys.is_empty()
}

/// Does an alias expansion name `field` and its state `state`: as a word
/// beside the field, in a comma list, or as `field:state`?
fn names_state(expansion: &str, field: &str, state: &str) -> bool {
    let words = datum_parse::shell_split(expansion);
    let paired = format!("{field}:{state}");
    names_field(expansion, field)
        && words
            .iter()
            .any(|w| w == &paired || w.split(',').any(|s| s == state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_value_renames_in_a_list_and_an_id_and_a_list_keeps_one() {
        for (before, after) in [
            (json!("a"), json!("c")),
            (json!(["good", "a"]), json!(["good", "c"])),
            (json!(["c", "a"]), json!("c")),
            (json!(["c", "a", "b"]), json!(["c", "b"])),
            (
                json!({"id": "a", "role": "x"}),
                json!({"id": "c", "role": "x"}),
            ),
        ] {
            let mut v = before.clone();
            assert!(rename_value(&mut v, "a", "c"), "{before}");
            assert_eq!(v, after, "{before}");
        }
        let mut v = json!(["b", true]);
        assert!(!rename_value(&mut v, "a", "c"));
    }

    #[test]
    fn a_new_name_must_read_back_as_the_state() {
        for ok in ["okay", "very-good", "a b", "str"] {
            assert_eq!(not_read_as_a_state(ok), None, "{ok}");
        }
        for bad in [
            "", "5", "a,b", "[a]", "null", "end", "off", "on", "-x", "-", "k=v", "IGNORED",
        ] {
            assert!(not_read_as_a_state(bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn an_expansion_names_a_state_beside_its_field() {
        for (expansion, names) in [
            ("switch mood meh", true),
            ("tail mood:meh", true),
            ("switch mood meh,good", true),
            ("switch mood good", false),
            ("switch feeling meh", false),
        ] {
            assert_eq!(names_state(expansion, "mood", "meh"), names, "{expansion}");
        }
    }
}
