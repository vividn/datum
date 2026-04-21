//! State compilation — answers "what is the active state of field F at time T?"
//!
//! Ports datum's `normalizeState` and `getActiveState` semantics into pure Rust.
//!
//! A "state" is a JSON value that represents the current status of a field:
//! - `Null` → the field is explicitly untracked at this moment
//! - `Bool(true)` → the field is "on" / started (shorthand from `start`)
//! - `Bool(false)` → the field is "off" / stopped (shorthand from `stop`, or after `occur`)
//! - `String(s)` → a named state (from `switch field s`)
//! - `Array(...)` / `Object(...)` → structured states
//!
//! State changes come from entries with verbs: switch, start, stop, occur.
//! `note` entries never affect state.

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use crate::entry::{Entry, Verb};

/// Active state type. `Value::Null` represents the "untracked" state.
pub type ActiveState = Value;

/// Normalize a raw JSON state value into its canonical form, matching
/// datum's `normalizeState` behavior.
///
/// Rules (in order):
/// - `null` or `"null"` → `Null`
/// - numbers → stringified (e.g. `123` → `"123"`)
/// - `"start"` → `true`; `"end"` → `false`
/// - strings and booleans pass through
/// - `[]` (empty array) → `false`
/// - `[single]` → recurse on the single element
/// - longer arrays → flattened array of normalized elements
/// - objects with only an `id` → unfold to the normalized id
/// - objects without `id` → treated as `{id: true, ...}`
/// - objects with id+other → id is normalized; other fields merged in
pub fn normalize_state(value: &Value) -> Value {
    match value {
        Value::Null => Value::Null,
        Value::String(s) if s == "null" => Value::Null,
        Value::String(s) if s == "start" => Value::Bool(true),
        Value::String(s) if s == "end" => Value::Bool(false),
        Value::String(_) | Value::Bool(_) => value.clone(),
        Value::Number(n) => Value::String(n.to_string()),
        Value::Array(arr) => normalize_array(arr),
        Value::Object(obj) => normalize_object(obj),
    }
}

fn normalize_array(arr: &[Value]) -> Value {
    if arr.is_empty() {
        return Value::Bool(false);
    }
    if arr.len() == 1 {
        return normalize_state(&arr[0]);
    }
    let mut out: Vec<Value> = Vec::with_capacity(arr.len());
    for item in arr {
        let normed = normalize_state(item);
        flatten_into(normed, &mut out);
    }
    Value::Array(out)
}

fn flatten_into(value: Value, out: &mut Vec<Value>) {
    match value {
        Value::Array(inner) => {
            for v in inner {
                flatten_into(v, out);
            }
        }
        other => out.push(other),
    }
}

fn normalize_object(obj: &Map<String, Value>) -> Value {
    // Default id to `true` if absent.
    let id_value = obj.get("id").cloned().unwrap_or(Value::Bool(true));

    let mut other_keys: Map<String, Value> = obj.clone();
    other_keys.remove("id");

    // If only `id` is present (or no keys at all), unfold to the normalized id.
    if other_keys.is_empty() {
        return normalize_state(&id_value);
    }

    let normalized_id = normalize_state(&id_value);

    match normalized_id {
        Value::Null => Value::Null,
        Value::String(_) | Value::Bool(_) => {
            let mut out = Map::new();
            out.insert("id".to_string(), normalized_id);
            for (k, v) in other_keys {
                out.insert(k, v);
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            let mapped: Vec<Value> = items
                .into_iter()
                .map(|item| match item {
                    Value::String(_) | Value::Bool(_) => {
                        let mut out = Map::new();
                        out.insert("id".to_string(), item);
                        for (k, v) in other_keys.iter() {
                            out.insert(k.clone(), v.clone());
                        }
                        Value::Object(out)
                    }
                    Value::Object(mut inner) => {
                        for (k, v) in other_keys.iter() {
                            inner.insert(k.clone(), v.clone());
                        }
                        Value::Object(inner)
                    }
                    other => other,
                })
                .collect();
            Value::Array(mapped)
        }
        Value::Object(mut inner) => {
            // Merge other_keys on top of the unfolded id object.
            for (k, v) in other_keys {
                inner.insert(k, v);
            }
            Value::Object(inner)
        }
        Value::Number(_) => Value::Null, // unreachable after normalize
    }
}

/// Compile the active state at time `at` from a set of entries for a single field.
///
/// State-change events are derived from each entry:
/// - `switch`: emits (time_start, value). If time_end is set (e.g. --dur), also
///   emits (time_end, false) — the block ends, state reverts to "off".
/// - `start`: emits (time_start, true). time_end analogous to switch.
/// - `stop`: emits (time_end or time_start, false).
/// - `occur`: emits (time_start, false) — datum treats occurrences as
///   instantaneous events that end the active block.
/// - `note`: no state effect.
///
/// The active state is the most recent event with time <= at, or `Null` if
/// none. Ties on time are broken by created_at descending so the latest write
/// wins — this mirrors datum's LWW semantics for same-instant events.
pub fn compile_state(entries: &[Entry], at: DateTime<Utc>) -> ActiveState {
    let mut events: Vec<(DateTime<Utc>, DateTime<Utc>, Value)> = Vec::new();
    for entry in entries {
        collect_events(entry, &mut events);
    }
    // Sort by event time, then created_at, both ascending. We want the last
    // event at or before `at`, with LWW tie-break on created_at.
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    events
        .into_iter()
        .rev()
        .find(|(t, _, _)| *t <= at)
        .map(|(_, _, v)| v)
        .unwrap_or(Value::Null)
}

fn collect_events(
    entry: &Entry,
    out: &mut Vec<(DateTime<Utc>, DateTime<Utc>, Value)>,
) {
    match entry.verb {
        Verb::Switch => {
            if let Some(ts) = &entry.start_time {
                let state = entry
                    .value
                    .as_ref()
                    .map(normalize_state)
                    .unwrap_or(Value::Null);
                out.push((ts.utc, entry.create_time, state));
                if let Some(te) = &entry.end_time {
                    out.push((te.utc, entry.create_time, Value::Bool(false)));
                }
            }
        }
        Verb::Start => {
            if let Some(ts) = &entry.start_time {
                out.push((ts.utc, entry.create_time, Value::Bool(true)));
                if let Some(te) = &entry.end_time {
                    out.push((te.utc, entry.create_time, Value::Bool(false)));
                }
            }
        }
        Verb::Stop => {
            let at = entry
                .end_time
                .as_ref()
                .or(entry.start_time.as_ref());
            if let Some(t) = at {
                out.push((t.utc, entry.create_time, Value::Bool(false)));
            }
        }
        Verb::Occur => {
            if let Some(ts) = &entry.start_time {
                out.push((ts.utc, entry.create_time, Value::Bool(false)));
            }
        }
        Verb::Block => {
            // A closed block: emit an "on" event at start_time (value when
            // present, true otherwise) and a terminal "off" at end_time.
            if let Some(ts) = &entry.start_time {
                let state = entry
                    .value
                    .as_ref()
                    .map(normalize_state)
                    .unwrap_or(Value::Bool(true));
                out.push((ts.utc, entry.create_time, state));
                if let Some(te) = &entry.end_time {
                    out.push((te.utc, entry.create_time, Value::Bool(false)));
                }
            }
        }
        Verb::Note | Verb::Add => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_state_matches_datum_behavior() {
        // null / "null" → null
        assert_eq!(normalize_state(&Value::Null), Value::Null);
        assert_eq!(normalize_state(&json!("null")), Value::Null);
        assert_eq!(normalize_state(&json!({"id": null})), Value::Null);

        // Plain values pass through
        assert_eq!(normalize_state(&json!("abcd")), json!("abcd"));
        assert_eq!(normalize_state(&json!(true)), json!(true));
        assert_eq!(normalize_state(&json!(false)), json!(false));

        // Numbers stringified
        assert_eq!(normalize_state(&json!(123)), json!("123"));

        // "start" → true, "end" → false
        assert_eq!(normalize_state(&json!("start")), json!(true));
        assert_eq!(normalize_state(&json!("end")), json!(false));

        // Unfold {id: x} to x
        assert_eq!(normalize_state(&json!({"id": true})), json!(true));
        assert_eq!(normalize_state(&json!({"id": false})), json!(false));
        assert_eq!(normalize_state(&json!({"id": 123})), json!("123"));

        // Preserve object with id + other keys
        assert_eq!(
            normalize_state(&json!({"id": "id", "abc": 123, "key": "value"})),
            json!({"id": "id", "abc": 123, "key": "value"}),
        );

        // Default id to true when absent
        assert_eq!(
            normalize_state(&json!({"abc": 123, "key": "value"})),
            json!({"id": true, "abc": 123, "key": "value"}),
        );
        assert_eq!(normalize_state(&json!({})), json!(true));

        // Empty array → false
        assert_eq!(normalize_state(&json!([])), json!(false));
        assert_eq!(normalize_state(&json!({"id": []})), json!(false));

        // Single-element array unfolds
        assert_eq!(normalize_state(&json!(["abcd"])), json!("abcd"));
        assert_eq!(normalize_state(&json!([null])), Value::Null);

        // Longer array, normalized per-element, string-like pass through
        assert_eq!(
            normalize_state(&json!(["a", "start", "end"])),
            json!(["a", true, false]),
        );

        // "start" inside {id: ...} becomes true
        assert_eq!(
            normalize_state(&json!({"id": "start", "extra": "key"})),
            json!({"id": true, "extra": "key"}),
        );

        // Unfold id-as-object into outer object
        assert_eq!(
            normalize_state(&json!({"id": {"id": "abc"}})),
            json!("abc"),
        );
    }

    #[test]
    fn normalize_state_handles_arrays_of_ids() {
        // {id: [a, b, c]} → [a, b, c]
        assert_eq!(
            normalize_state(&json!({"id": ["s1", "s2", "s3"]})),
            json!(["s1", "s2", "s3"]),
        );
        // {id: [a, b], name: "x"} → [{id:a,name:x}, {id:b,name:x}]
        assert_eq!(
            normalize_state(&json!({"id": ["s1", true], "name": "n"})),
            json!([
                {"id": "s1", "name": "n"},
                {"id": true, "name": "n"},
            ]),
        );
    }
}
