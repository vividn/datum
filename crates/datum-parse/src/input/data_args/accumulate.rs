//! Accumulation phase: take a coerced [`Value`] and write it into the
//! growing [`crate::input::data_args::DatumData`] payload at a given
//! dotted path. Owns:
//! - dotted-path traversal primitives (`get_path`, `set_path`,
//!   `remove_path`) with lodash.set-like overwrite semantics
//! - [`alter_datum_data`] — the single seam the orchestration loop uses
//!   to either set or append a value, dispatching coercion through
//!   [`super::coerce::infer_typed_value`]
//! - append fan-out (`create_or_append`) for repeated comments and
//!   remainder buckets

use serde_json::{Map, Value};

use crate::error::{ParseError, Result};
use crate::input::data_args::Compat;

use super::classify::{datum_path, path_segs};
use super::coerce::infer_typed_value;

/// Walk a dotted path through a `Map` and return a reference to the leaf.
pub(super) fn get_path<'a>(obj: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let segs: Vec<&str> = path_segs(path);
    if segs.is_empty() {
        return None;
    }
    let mut cur = obj.get(segs[0])?;
    for seg in &segs[1..] {
        match cur {
            Value::Object(m) => {
                cur = m.get(*seg)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

/// Remove a dotted path from `obj`. No-op if the path doesn't exist.
pub(super) fn remove_path(obj: &mut Map<String, Value>, path: &str) {
    let segs: Vec<&str> = path_segs(path);
    if segs.is_empty() {
        return;
    }
    if segs.len() == 1 {
        obj.remove(segs[0]);
        return;
    }
    let (last, parents) = segs.split_last().unwrap();
    let mut cur: &mut Map<String, Value> = obj;
    for seg in parents {
        match cur.get_mut(*seg) {
            Some(Value::Object(m)) => {
                cur = m;
            }
            _ => return,
        }
    }
    cur.remove(*last);
}

/// Set a dotted path to `value`. Scalars in the path are overwritten
/// with fresh objects to match lodash.set's semantics.
pub(super) fn set_path(obj: &mut Map<String, Value>, path: &str, value: Value) {
    let segs: Vec<&str> = path_segs(path);
    if segs.is_empty() {
        return;
    }
    if segs.len() == 1 {
        obj.insert(segs[0].to_string(), value);
        return;
    }
    let (last, parents) = segs.split_last().unwrap();
    let mut cur: &mut Map<String, Value> = obj;
    for seg in parents {
        let entry = cur
            .entry((*seg).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        cur = entry.as_object_mut().unwrap();
    }
    cur.insert(last.to_string(), value);
}

/// Port of datum's `alterDatumData`: infer the right JSON shape for
/// `value` (delegating to [`super::coerce::infer_typed_value`]) and set
/// or append it at `path` inside `obj`.
pub(super) fn alter_datum_data(
    obj: &mut Map<String, Value>,
    path: &str,
    value: Option<&str>,
    default_value: Option<&str>,
    append: bool,
    is_required: bool,
    compat: Compat,
) -> Result<()> {
    let state_aware = datum_path(path);

    // `.` → recurse with the default.
    if matches!(value, Some(".")) {
        return alter_datum_data(obj, path, default_value, None, append, is_required, compat);
    }

    let inferred = match value {
        Some(v) => infer_typed_value(&state_aware, v, is_required, compat)?,
        None => {
            if is_required {
                return Err(ParseError::MissingRequiredKey(state_aware.clone()));
            }
            None
        }
    };

    if append {
        let Some(new_val) = inferred else {
            return Ok(());
        };
        let existing = get_path(obj, &state_aware).cloned();
        let combined = create_or_append(existing, new_val);
        set_path(obj, &state_aware, combined);
    } else {
        match inferred {
            Some(v) => set_path(obj, &state_aware, v),
            None => remove_path(obj, &state_aware),
        }
    }
    Ok(())
}

/// Append-or-promote: the first value at a path stays scalar; the
/// second wraps both into an array; subsequent values push onto that
/// array. Used by the comment / string-remainder fan-out.
fn create_or_append(existing: Option<Value>, to_append: Value) -> Value {
    match existing {
        None => to_append,
        Some(Value::Array(mut arr)) => {
            arr.push(to_append);
            Value::Array(arr)
        }
        Some(other) => Value::Array(vec![other, to_append]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn set_and_get_paths_round_trip() {
        let mut m = Map::new();
        set_path(&mut m, "a.b.c", json!(1));
        assert_eq!(get_path(&m, "a.b.c"), Some(&json!(1)));
        set_path(&mut m, "a.b.c", json!(2));
        assert_eq!(get_path(&m, "a.b.c"), Some(&json!(2)));
        set_path(&mut m, "a.b.d", json!("x"));
        assert_eq!(get_path(&m, "a.b.d"), Some(&json!("x")));
    }

    #[test]
    fn set_path_overwrites_scalar_with_object() {
        let mut m = Map::new();
        set_path(&mut m, "top", json!("primitive"));
        set_path(&mut m, "top.nested", json!("abc"));
        assert_eq!(get_path(&m, "top.nested"), Some(&json!("abc")));
    }

    #[test]
    fn remove_path_removes_leaf() {
        let mut m = Map::new();
        set_path(&mut m, "a.b.c", json!(1));
        remove_path(&mut m, "a.b.c");
        assert_eq!(get_path(&m, "a.b.c"), None);
    }
}
