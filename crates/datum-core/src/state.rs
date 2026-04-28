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
use rusqlite::{params, Connection};
use serde_json::{Map, Value};

use crate::entry::{Entry, Verb};
use crate::error::{DatumError, Result};

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

/// Compute the active state of `field` strictly before (or at) `time`,
/// ignoring an entry identified by `exclude_hid` (so detection run on a
/// just-inserted row can see "the state as it was without this entry").
///
/// Returns `None` when the field has no state-affecting history at all
/// at-or-before `time` (state compiles to `Null`); otherwise the concrete
/// value — including `Bool(false)` for an explicit stop.
pub fn last_state_at(
    conn: &Connection,
    field: &str,
    time: DateTime<Utc>,
    exclude_hid: Option<&str>,
) -> Result<Option<Value>> {
    let at_str = time.to_rfc3339();
    let exclude = exclude_hid.unwrap_or("");
    let mut stmt = conn.prepare(
        r#"SELECT * FROM entries
           WHERE field = ?1
             AND deleted_at IS NULL
             AND verb IN ('switch', 'start', 'stop', 'occur', 'block')
             AND hid != ?3
             AND (
                  (start_time IS NOT NULL AND start_time <= ?2)
               OR (end_time   IS NOT NULL AND end_time   <= ?2)
             )
           ORDER BY COALESCE(start_time, end_time, create_time) ASC"#,
    )?;
    let rows = stmt.query_map(params![field, at_str, exclude], crate::db::row_to_entry_public)?;
    let entries: Vec<Entry> = rows
        .map(|r| r.map_err(DatumError::Db))
        .collect::<Result<Vec<_>>>()?;
    let compiled = compile_state(&entries, time);
    if compiled.is_null() {
        Ok(None)
    } else {
        Ok(Some(compiled))
    }
}

/// Pre-derived state events for a single field, loaded once per render
/// batch so `tail` / `head` answer many lead/trail/ongoing state probes
/// without re-issuing one (multi-row, all-columns) `last_state_at` query
/// per output row. The events are the same ones [`compile_state`] would
/// derive, tagged with the originating `hid` so probes can honour
/// `exclude_hid` (matching `last_state_at`'s contract).
#[derive(Default, Debug, Clone)]
pub struct FieldStateEvents {
    /// `(event_time, create_time, value, origin_hid)`, sorted ascending by
    /// `event_time` then `create_time`. Probes scan from the tail and stop
    /// at the first event ≤ probe time.
    events: Vec<(DateTime<Utc>, DateTime<Utc>, Value, String)>,
}

impl FieldStateEvents {
    /// Load every state-affecting event for `field`. See
    /// [`Self::load_for_field_up_to`] for the bounded variant.
    pub fn load_for_field(conn: &Connection, field: &str) -> Result<Self> {
        Self::load_for_field_up_to(conn, field, None)
    }

    /// Load state-affecting events for `field`, optionally pruned to entries
    /// that could contribute an event ≤ `max_probe`. Reads only the columns
    /// [`compile_state`] needs.
    ///
    /// `max_probe` is an upper bound on every probe time the caller will
    /// issue (e.g. `tail -yt 15` → max probe is yesterday-15:00). Events
    /// strictly later than that never affect the answer, so the WHERE clause
    /// matches `last_state_at`'s `(start_time <= ?) OR (end_time <= ?)`
    /// predicate. Pass `None` for an unbounded load (used when probes can be
    /// `now` and bounding wouldn't prune anything).
    pub fn load_for_field_up_to(
        conn: &Connection,
        field: &str,
        max_probe: Option<DateTime<Utc>>,
    ) -> Result<Self> {
        let mut events: Vec<(DateTime<Utc>, DateTime<Utc>, Value, String)> = Vec::new();
        let push = |row: &rusqlite::Row<'_>,
                    out: &mut Vec<(DateTime<Utc>, DateTime<Utc>, Value, String)>|
         -> rusqlite::Result<()> {
            let hid: String = row.get("hid")?;
            let verb_str: String = row.get("verb")?;
            let verb: Verb = verb_str.parse().map_err(|e: String| {
                rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
            })?;
            let value: Option<Value> = row
                .get::<_, Option<String>>("value")?
                .and_then(|s| serde_json::from_str(&s).ok());
            let start_time: Option<DateTime<Utc>> = row
                .get::<_, Option<String>>("start_time")?
                .and_then(|s| s.parse().ok());
            let end_time: Option<DateTime<Utc>> = row
                .get::<_, Option<String>>("end_time")?
                .and_then(|s| s.parse().ok());
            let create_time: DateTime<Utc> = row
                .get::<_, String>("create_time")?
                .parse()
                .map_err(|e: chrono::ParseError| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        e.into(),
                    )
                })?;
            collect_raw_events(
                &hid, verb, value.as_ref(), start_time, end_time, create_time, out,
            );
            Ok(())
        };

        match max_probe {
            Some(t) => {
                let mut stmt = conn.prepare(
                    r#"SELECT hid, verb, value, start_time, end_time, create_time
                       FROM entries
                       WHERE field = ?1
                         AND deleted_at IS NULL
                         AND verb IN ('switch', 'start', 'stop', 'occur', 'block')
                         AND (
                              (start_time IS NOT NULL AND start_time <= ?2)
                           OR (end_time   IS NOT NULL AND end_time   <= ?2)
                         )"#,
                )?;
                let mut rows = stmt.query(params![field, t.to_rfc3339()])?;
                while let Some(row) = rows.next()? {
                    push(row, &mut events)?;
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    r#"SELECT hid, verb, value, start_time, end_time, create_time
                       FROM entries
                       WHERE field = ?1
                         AND deleted_at IS NULL
                         AND verb IN ('switch', 'start', 'stop', 'occur', 'block')"#,
                )?;
                let mut rows = stmt.query(params![field])?;
                while let Some(row) = rows.next()? {
                    push(row, &mut events)?;
                }
            }
        }

        events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        Ok(FieldStateEvents { events })
    }

    /// Compile the active state at `time`, optionally ignoring events that
    /// originated on `exclude_hid`. Mirrors [`last_state_at`]: a `Null`
    /// result collapses to `None`.
    pub fn at(&self, time: DateTime<Utc>, exclude_hid: Option<&str>) -> Option<Value> {
        let exclude = exclude_hid.unwrap_or("");
        for (t, _, value, hid) in self.events.iter().rev() {
            if *t > time {
                continue;
            }
            if !exclude.is_empty() && hid == exclude {
                continue;
            }
            return if value.is_null() {
                None
            } else {
                Some(value.clone())
            };
        }
        None
    }
}

/// Variant of [`collect_events`] that operates on already-extracted columns,
/// tagging each emitted event with its origin `hid` so [`FieldStateEvents`]
/// can honour `exclude_hid` probes.
fn collect_raw_events(
    hid: &str,
    verb: Verb,
    value: Option<&Value>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    create_time: DateTime<Utc>,
    out: &mut Vec<(DateTime<Utc>, DateTime<Utc>, Value, String)>,
) {
    match verb {
        Verb::Switch => {
            if let Some(ts) = start_time {
                let state = value.map(normalize_state).unwrap_or(Value::Null);
                out.push((ts, create_time, state, hid.to_string()));
                if let Some(te) = end_time {
                    out.push((te, create_time, Value::Bool(false), hid.to_string()));
                }
            }
        }
        Verb::Start => {
            if let Some(ts) = start_time {
                out.push((ts, create_time, Value::Bool(true), hid.to_string()));
                if let Some(te) = end_time {
                    out.push((te, create_time, Value::Bool(false), hid.to_string()));
                }
            }
        }
        Verb::Stop => {
            if let Some(t) = end_time.or(start_time) {
                out.push((t, create_time, Value::Bool(false), hid.to_string()));
            }
        }
        Verb::Occur => {
            if let Some(ts) = start_time {
                out.push((ts, create_time, Value::Bool(false), hid.to_string()));
            }
        }
        Verb::Block => {
            if let Some(ts) = start_time {
                let state = value.map(normalize_state).unwrap_or(Value::Bool(true));
                out.push((ts, create_time, state, hid.to_string()));
                if let Some(te) = end_time {
                    out.push((te, create_time, Value::Bool(false), hid.to_string()));
                }
            }
        }
        Verb::Add => {}
    }
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
        Verb::Add => {}
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
