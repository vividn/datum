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
use std::fmt;

use crate::db::IGNORED_STATE_VALUE;
use crate::dur::Dur;
use crate::entry::{Entry, Verb};
use crate::error::{DatumError, Result};

/// Active state type. `Value::Null` represents the "untracked" state.
pub type ActiveState = Value;

/// Typed representation of a `state` value. Mirrors the JSON shapes
/// `state` can take on disk:
///
/// - `Null` — undefined; entry exists but state is genuinely unknown.
/// - `Ignored` — explicit untrack (round-trips to/from the magic-string
///   `"IGNORED"` so existing rows deserialize cleanly).
/// - `Bool(b)` — start (`true`) / end (`false`) shorthand.
/// - `Simple(s)` — bare named state, e.g. `"vertical"`, `"sit"`, `"car"`.
/// - `Structured(o)` — object with an `id` plus arbitrary extras,
///   e.g. `{id: "car", role: "driver"}`.
/// - `Parallel(v)` — multiple simultaneous states.
///
/// Storage stays JSON via [`StateValue::from_json`] / [`StateValue::to_json`];
/// the on-disk shape is unchanged. `PartialEq` is structural so callers can
/// compare two states for full equality (e.g. for repeated-state checks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateValue {
    Null,
    Ignored,
    Bool(bool),
    Simple(String),
    Structured(Map<String, Value>),
    Parallel(Vec<StateValue>),
}

impl StateValue {
    /// Map a raw JSON value into a `StateValue` without applying any
    /// of the `normalize_state` rewrite rules (no `"start"` → `true`
    /// translation, no number-to-string promotion). The string sentinel
    /// `"IGNORED"` is the only special-case — it maps to
    /// [`StateValue::Ignored`] so the on-disk magic-string keeps its
    /// meaning. Numbers stringify into `Simple` so the converter is
    /// total over `Value`.
    pub fn from_json(v: &Value) -> Result<Self> {
        Ok(match v {
            Value::Null => StateValue::Null,
            Value::String(s) if s == IGNORED_STATE_VALUE => StateValue::Ignored,
            Value::String(s) => StateValue::Simple(s.clone()),
            Value::Bool(b) => StateValue::Bool(*b),
            Value::Number(n) => StateValue::Simple(n.to_string()),
            Value::Array(arr) => StateValue::Parallel(
                arr.iter()
                    .map(StateValue::from_json)
                    .collect::<Result<Vec<_>>>()?,
            ),
            Value::Object(obj) => StateValue::Structured(obj.clone()),
        })
    }

    /// Serialize back to JSON. Round-trip with [`StateValue::from_json`]
    /// preserves shape (modulo the explicit normalizations done by
    /// [`normalize_state`]).
    pub fn to_json(&self) -> Value {
        match self {
            StateValue::Null => Value::Null,
            StateValue::Ignored => Value::String(IGNORED_STATE_VALUE.to_string()),
            StateValue::Bool(b) => Value::Bool(*b),
            StateValue::Simple(s) => Value::String(s.clone()),
            StateValue::Structured(obj) => Value::Object(obj.clone()),
            StateValue::Parallel(items) => {
                Value::Array(items.iter().map(StateValue::to_json).collect())
            }
        }
    }

    /// The "name" of this state, when one exists. `Simple` returns its
    /// string; `Structured` returns its `id` field if it's a string.
    /// Everything else returns `None`.
    pub fn id(&self) -> Option<&str> {
        match self {
            StateValue::Simple(s) => Some(s.as_str()),
            StateValue::Structured(obj) => obj.get("id").and_then(Value::as_str),
            _ => None,
        }
    }

    /// Returns the bare-string state if this is [`StateValue::Simple`],
    /// otherwise `None`. Distinct from [`Self::id`] which also peels
    /// `Structured.id`.
    pub fn as_simple(&self) -> Option<&str> {
        match self {
            StateValue::Simple(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn is_structured(&self) -> bool {
        matches!(self, StateValue::Structured(_))
    }

    pub fn is_parallel(&self) -> bool {
        matches!(self, StateValue::Parallel(_))
    }
}

/// Render a `StateValue` for human display. Peels back common shapes:
/// `Null` → `""`; `Ignored` → `"IGNORED"`; `Bool` → `"true"` / `"false"`;
/// `Simple("v")` → `"v"`; `Structured({id: "car"})` → `"car"`;
/// `Structured({id: "car", role: "driver"})` → `"car {role: driver}"`;
/// `Parallel([a, b])` → `"a, b"`.
impl fmt::Display for StateValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateValue::Null => Ok(()),
            StateValue::Ignored => f.write_str(IGNORED_STATE_VALUE),
            StateValue::Bool(b) => write!(f, "{}", b),
            StateValue::Simple(s) => f.write_str(s),
            StateValue::Structured(obj) => {
                let id_str = obj.get("id").and_then(Value::as_str);
                let extras: Vec<(&String, &Value)> =
                    obj.iter().filter(|(k, _)| k.as_str() != "id").collect();
                if extras.is_empty() {
                    match id_str {
                        Some(id) => f.write_str(id),
                        None => f.write_str("{}"),
                    }
                } else {
                    let mut parts: Vec<String> = Vec::with_capacity(extras.len());
                    for (k, v) in &extras {
                        let rendered = match v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        parts.push(format!("{}: {}", k, rendered));
                    }
                    let body = parts.join(", ");
                    match id_str {
                        Some(id) => write!(f, "{} {{{}}}", id, body),
                        None => write!(f, "{{{}}}", body),
                    }
                }
            }
            StateValue::Parallel(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                f.write_str(&parts.join(", "))
            }
        }
    }
}

/// Normalize a raw JSON state value into its canonical form, matching
/// datum's `normalizeState` behavior, then classify it as a typed
/// [`StateValue`].
///
/// Rules (in order, applied in JSON space before classification):
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
pub fn normalize_state(value: &Value) -> StateValue {
    let normalized = normalize_to_value(value);
    // `from_json` is total over the values produced by `normalize_to_value`.
    StateValue::from_json(&normalized).expect("normalize_to_value output is always convertible")
}

fn normalize_to_value(value: &Value) -> Value {
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
        return normalize_to_value(&arr[0]);
    }
    let mut out: Vec<Value> = Vec::with_capacity(arr.len());
    for item in arr {
        let normed = normalize_to_value(item);
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
        return normalize_to_value(&id_value);
    }

    let normalized_id = normalize_to_value(&id_value);

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
    let sql = format!(
        "SELECT * FROM entries
         WHERE field = ?1
           AND {participation}
           AND hid != ?3
           AND occur_time <= ?2
         ORDER BY {order}",
        participation = crate::db::STATE_PARTICIPATION_SQL,
        order = crate::db::STATE_ORDER_OLDEST_FIRST,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![field, at_str, exclude],
        crate::db::row_to_entry_public,
    )?;
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
            let state: Option<Value> = row
                .get::<_, Option<String>>("state")?
                .and_then(|s| serde_json::from_str(&s).ok());
            let occur_time: Option<DateTime<Utc>> = row
                .get::<_, Option<String>>("occur_time")?
                .and_then(|s| s.parse().ok());
            let create_time: DateTime<Utc> =
                row.get::<_, String>("create_time")?
                    .parse()
                    .map_err(|e: chrono::ParseError| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Text,
                            e.into(),
                        )
                    })?;
            // da-2i8g sub-bead 5: block-switch / point-switch emit a
            // trailing event at `occur_time + dur` carrying the
            // `following_state`. Pull the columns so collect_raw_events
            // can decide.
            let dur: Option<Dur> = row
                .get::<_, Option<String>>("dur")?
                .as_deref()
                .and_then(Dur::parse);
            let following_state: Option<Value> = row
                .get::<_, Option<String>>("following_state")?
                .and_then(|s| serde_json::from_str(&s).ok());
            collect_raw_events(
                &hid,
                verb,
                state.as_ref(),
                occur_time,
                None,
                create_time,
                dur.as_ref(),
                following_state.as_ref(),
                out,
            );
            Ok(())
        };

        let participation = crate::db::STATE_PARTICIPATION_SQL;
        match max_probe {
            Some(t) => {
                let sql = format!(
                    "SELECT hid, verb, state, occur_time, create_time, dur, following_state
                     FROM entries
                     WHERE field = ?1
                       AND {participation}
                       AND occur_time <= ?2
                     ORDER BY {order}",
                    order = crate::db::STATE_ORDER_OLDEST_FIRST,
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params![field, t.to_rfc3339()])?;
                while let Some(row) = rows.next()? {
                    push(row, &mut events)?;
                }
            }
            None => {
                let sql = format!(
                    "SELECT hid, verb, state, occur_time, create_time, dur, following_state
                     FROM entries
                     WHERE field = ?1
                       AND {participation}
                     ORDER BY {order}",
                    order = crate::db::STATE_ORDER_OLDEST_FIRST,
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut rows = stmt.query(params![field])?;
                while let Some(row) = rows.next()? {
                    push(row, &mut events)?;
                }
            }
        }

        // Stable sort: rows tied on (event_time, create_time) keep the SQL
        // order, which is why both arms above ask for the canonical
        // oldest-first ordering. Same-instant live rows are schema-impossible
        // on one device but arrive routinely from a sync merge (da-ylet), and
        // this is the door integrity detection reads through (da-j8cu) — it
        // must resolve such a tie to the same row `last_state_at` picks.
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

/// Whether an emitted state event is a genuine state transition or an
/// occur-shaped annotation. Both kinds participate in state compilation
/// (`state_at` / `now` probes); only [`EventKind::Transition`] events are
/// eligible as `_` ordinal candidates ([`crate::db::Db::resolve_state_back`]).
/// da-a0n3: this classification lives in the canonical compiler so the
/// ordinal walk needs no local row predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventKind {
    /// A deliberate state change — counts in the distinct-ordinals walk.
    Transition,
    /// Occur-shaped noise (a bare `occur` dot, a stateless block): affects
    /// compiled state but is invisible to `_` ordinals.
    Annotation,
}

/// THE verb → state-events match, shared by [`entry_state_events`],
/// [`entry_transition_events`] and [`collect_raw_events`] so the three
/// doors cannot drift (da-a0n3).
///
/// da-5ns sub-bead 1: each state-affecting verb emits an event at
/// `occur` (the row's occur_time). da-2i8g sub-bead 5: block-switch and
/// point-switch ALSO emit a trailing event at `occur + dur` carrying the
/// `following_state` value. That trailing event is what makes a
/// post-block probe see the next state (e.g. after a `sit 30 → vertical`
/// block, `now` 31 min later reports `vertical`, not `sit`).
fn verb_state_events(
    verb: Verb,
    state: Option<&Value>,
    occur: Option<DateTime<Utc>>,
    dur: Option<&Dur>,
    following_state: Option<&Value>,
    out: &mut Vec<(DateTime<Utc>, Value, EventKind)>,
) {
    // Legacy `add` rows (state=null, no time) wrote nothing — they
    // classify as Point, but the missing occur_time short-circuits here.
    let Some(ts) = occur else { return };
    // Exhaustive on purpose (no wildcard): the state compiler is the
    // storage-side counterpart of shape.rs — a new verb must decide its
    // event emission here, enforced at compile time.
    match verb {
        Verb::Switch => {
            // A SQL-NULL state carries no state at all (legacy rows only —
            // derive_verb never mints a stateless Switch): the compiled
            // `Null` means "untracked", not a state the user switched to.
            let (v, kind) = match state {
                Some(v) => (normalize_state(v).to_json(), EventKind::Transition),
                None => (Value::Null, EventKind::Annotation),
            };
            out.push((ts, v, kind));
        }
        Verb::BlockSwitch | Verb::PointSwitch => {
            let (primary, kind) = match state {
                Some(v) => (normalize_state(v).to_json(), EventKind::Transition),
                None => (Value::Null, EventKind::Annotation),
            };
            out.push((ts, primary, kind));
            if let (Some(d), Some(fs)) = (dur, following_state) {
                // PointSwitch: dur is a zero span; the trailing
                // event lands at the same occur instant. The
                // stable sort preserves the push order so the
                // trailing event still wins on a `<= t` probe.
                // BlockSwitch: trailing lands at occur + dur.
                let delta = d.as_seconds().unwrap_or(0);
                let trailing_ts = ts + chrono::Duration::seconds(delta);
                let trailing = normalize_state(fs).to_json();
                out.push((trailing_ts, trailing, EventKind::Transition));
            }
        }
        Verb::Point => {
            // Occur-shaped rows (state absent, or the literal `true` that
            // legacy occur wrote): the dot ends any active block —
            // state=false annotation. Point with any OTHER state is
            // `switch --moment`: a genuine zero-length transition that
            // carries (and prevails with) its real state, matching the
            // SQL oracle ([`crate::db::Db::state_at`]).
            match state {
                None | Some(Value::Bool(true)) => {
                    out.push((ts, Value::Bool(false), EventKind::Annotation));
                }
                Some(v) => {
                    out.push((ts, normalize_state(v).to_json(), EventKind::Transition));
                }
            }
        }
        Verb::Block => {
            // A stateless block (`occur --dur`, legacy start/stop spans)
            // means "active": state=true, but it isn't a state the user
            // named — annotation for ordinal purposes.
            let (v, kind) = match state {
                Some(v) => (normalize_state(v).to_json(), EventKind::Transition),
                None => (Value::Bool(true), EventKind::Annotation),
            };
            out.push((ts, v, kind));
        }
    }
}

/// Variant of [`collect_events`] that operates on already-extracted columns,
/// tagging each emitted event with its origin `hid` so [`FieldStateEvents`]
/// can honour `exclude_hid` probes. Delegates to [`verb_state_events`].
#[allow(clippy::too_many_arguments)]
fn collect_raw_events(
    hid: &str,
    verb: Verb,
    value: Option<&Value>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    create_time: DateTime<Utc>,
    dur: Option<&Dur>,
    following_state: Option<&Value>,
    out: &mut Vec<(DateTime<Utc>, DateTime<Utc>, Value, String)>,
) {
    let _ = end_time;
    let mut events = Vec::new();
    verb_state_events(verb, value, start_time, dur, following_state, &mut events);
    for (ts, state, _) in events {
        out.push((ts, create_time, state, hid.to_string()));
    }
}

/// State-transition events asserted by a single entry, in emission order:
/// `(event_time, state)` pairs.
///
/// da-5ns sub-bead 1: each state-affecting verb emits an event at
/// occur_time. da-2i8g sub-bead 5: block-switch / point-switch
/// additionally emit a trailing `following_state` event at
/// occur_time + dur, so a `state_at(t)` probe past the block sees
/// the next state, not the block's primary.
///
/// Timeless rows (bare `add`/`note`, or `-T`/`--no-timestamp`): the
/// `occur_time` column is NULL, which marks the row as deliberately
/// timeless (its slug is `field:c<create_time>`). State compilation
/// skips these — they shouldn't emit on the timeline.
///
/// This is the per-`Entry` form of the compiler's event extraction.
/// Callers that must attribute each transition to its originating row
/// (e.g. tail's block mode, da-eybr) consume this instead of
/// re-mirroring the verb match.
pub fn entry_state_events(entry: &Entry) -> Vec<(DateTime<Utc>, Value)> {
    entry_events_kinded(entry)
        .into_iter()
        .map(|(ts, state, _)| (ts, state))
        .collect()
}

/// The ordinal-eligible subset of [`entry_state_events`]: only genuine
/// state transitions (deliberate switches, including the trailing
/// `following_state` event of a compound switch), with occur-shaped
/// annotations filtered out. This is the stream the `_` / `__` / `_n`
/// distinct-state walk samples ([`crate::db::Db::resolve_state_back`],
/// da-a0n3) — the transition/annotation distinction falls out of the
/// canonical compiler ([`verb_state_events`]), not a caller-side row
/// predicate.
pub fn entry_transition_events(entry: &Entry) -> Vec<(DateTime<Utc>, Value)> {
    entry_events_kinded(entry)
        .into_iter()
        .filter(|(_, _, kind)| *kind == EventKind::Transition)
        .map(|(ts, state, _)| (ts, state))
        .collect()
}

fn entry_events_kinded(entry: &Entry) -> Vec<(DateTime<Utc>, Value, EventKind)> {
    let mut out = Vec::new();
    // da-r17h: a deliberately timeless row carries `occur_time IS NULL`
    // (its slug is now `field:c<create_time>`, no longer the `:unset`
    // sentinel). It has no place on the timeline, so emit no events.
    if entry.occur_time.is_none() {
        return out;
    }
    verb_state_events(
        entry.verb,
        entry.state.as_ref(),
        entry.occur_time.as_ref().map(|t| t.utc),
        entry.dur.as_ref(),
        entry.following_state.as_ref(),
        &mut out,
    );
    out
}

fn collect_events(entry: &Entry, out: &mut Vec<(DateTime<Utc>, DateTime<Utc>, Value)>) {
    for (ts, state) in entry_state_events(entry) {
        out.push((ts, entry.create_time, state));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_state_matches_datum_behavior() {
        // null / "null" → null
        assert_eq!(normalize_state(&Value::Null).to_json(), Value::Null);
        assert_eq!(normalize_state(&json!("null")).to_json(), Value::Null);
        assert_eq!(normalize_state(&json!({"id": null})).to_json(), Value::Null);

        // Plain values pass through
        assert_eq!(normalize_state(&json!("abcd")).to_json(), json!("abcd"));
        assert_eq!(normalize_state(&json!(true)).to_json(), json!(true));
        assert_eq!(normalize_state(&json!(false)).to_json(), json!(false));

        // Numbers stringified
        assert_eq!(normalize_state(&json!(123)).to_json(), json!("123"));

        // "start" → true, "end" → false
        assert_eq!(normalize_state(&json!("start")).to_json(), json!(true));
        assert_eq!(normalize_state(&json!("end")).to_json(), json!(false));

        // Unfold {id: x} to x
        assert_eq!(normalize_state(&json!({"id": true})).to_json(), json!(true));
        assert_eq!(
            normalize_state(&json!({"id": false})).to_json(),
            json!(false)
        );
        assert_eq!(normalize_state(&json!({"id": 123})).to_json(), json!("123"));

        // Preserve object with id + other keys
        assert_eq!(
            normalize_state(&json!({"id": "id", "abc": 123, "key": "value"})).to_json(),
            json!({"id": "id", "abc": 123, "key": "value"}),
        );

        // Default id to true when absent
        assert_eq!(
            normalize_state(&json!({"abc": 123, "key": "value"})).to_json(),
            json!({"id": true, "abc": 123, "key": "value"}),
        );
        assert_eq!(normalize_state(&json!({})).to_json(), json!(true));

        // Empty array → false
        assert_eq!(normalize_state(&json!([])).to_json(), json!(false));
        assert_eq!(normalize_state(&json!({"id": []})).to_json(), json!(false));

        // Single-element array unfolds
        assert_eq!(normalize_state(&json!(["abcd"])).to_json(), json!("abcd"));
        assert_eq!(normalize_state(&json!([null])).to_json(), Value::Null);

        // Longer array, normalized per-element, string-like pass through
        assert_eq!(
            normalize_state(&json!(["a", "start", "end"])).to_json(),
            json!(["a", true, false]),
        );

        // "start" inside {id: ...} becomes true
        assert_eq!(
            normalize_state(&json!({"id": "start", "extra": "key"})).to_json(),
            json!({"id": true, "extra": "key"}),
        );

        // Unfold id-as-object into outer object
        assert_eq!(
            normalize_state(&json!({"id": {"id": "abc"}})).to_json(),
            json!("abc"),
        );
    }

    #[test]
    fn normalize_state_handles_arrays_of_ids() {
        // {id: [a, b, c]} → [a, b, c]
        assert_eq!(
            normalize_state(&json!({"id": ["s1", "s2", "s3"]})).to_json(),
            json!(["s1", "s2", "s3"]),
        );
        // {id: [a, b], name: "x"} → [{id:a,name:x}, {id:b,name:x}]
        assert_eq!(
            normalize_state(&json!({"id": ["s1", true], "name": "n"})).to_json(),
            json!([
                {"id": "s1", "name": "n"},
                {"id": true, "name": "n"},
            ]),
        );
    }

    #[test]
    fn state_value_round_trips_through_json() {
        // Each shape: from_json(to_json(sv)) == sv.
        let cases = vec![
            StateValue::Null,
            StateValue::Ignored,
            StateValue::Bool(true),
            StateValue::Bool(false),
            StateValue::Simple("vertical".into()),
            StateValue::Simple("car".into()),
            StateValue::Structured({
                let mut m = Map::new();
                m.insert("id".into(), json!("car"));
                m.insert("role".into(), json!("driver"));
                m
            }),
            StateValue::Structured({
                // No `id` field — round-trips as a plain object. Note the
                // normalize_state pipeline would inject `id: true`, but
                // `from_json` is the raw mapping.
                let mut m = Map::new();
                m.insert("type".into(), json!("vehicle"));
                m
            }),
            StateValue::Parallel(vec![
                StateValue::Simple("vertical".into()),
                StateValue::Simple("sit".into()),
            ]),
            StateValue::Parallel(vec![
                StateValue::Bool(true),
                StateValue::Structured({
                    let mut m = Map::new();
                    m.insert("id".into(), json!("car"));
                    m.insert("role".into(), json!("driver"));
                    m
                }),
            ]),
        ];
        for sv in cases {
            let json = sv.to_json();
            let back = StateValue::from_json(&json).expect("from_json should succeed");
            assert_eq!(back, sv, "round-trip failed for {:?}", json);
        }
    }

    #[test]
    fn state_value_ignored_round_trips_via_string_sentinel() {
        let sv = StateValue::Ignored;
        let json = sv.to_json();
        assert_eq!(json, json!("IGNORED"));
        assert_eq!(StateValue::from_json(&json).unwrap(), StateValue::Ignored);
    }

    #[test]
    fn state_value_helpers() {
        // .id() — Simple returns its string; Structured returns id when present.
        assert_eq!(StateValue::Simple("car".into()).id(), Some("car"));
        let structured = StateValue::Structured({
            let mut m = Map::new();
            m.insert("id".into(), json!("car"));
            m.insert("role".into(), json!("driver"));
            m
        });
        assert_eq!(structured.id(), Some("car"));
        // No id field: None.
        let no_id = StateValue::Structured({
            let mut m = Map::new();
            m.insert("role".into(), json!("driver"));
            m
        });
        assert_eq!(no_id.id(), None);
        // Non-string id: None (per spec — only strings count).
        let bool_id = StateValue::Structured({
            let mut m = Map::new();
            m.insert("id".into(), json!(true));
            m
        });
        assert_eq!(bool_id.id(), None);
        assert_eq!(StateValue::Bool(true).id(), None);
        assert_eq!(StateValue::Null.id(), None);
        assert_eq!(StateValue::Ignored.id(), None);

        // .as_simple() — only Simple.
        assert_eq!(StateValue::Simple("x".into()).as_simple(), Some("x"));
        assert_eq!(structured.as_simple(), None);

        // .is_structured() / .is_parallel().
        assert!(structured.is_structured());
        assert!(!structured.is_parallel());
        let parallel = StateValue::Parallel(vec![StateValue::Bool(true)]);
        assert!(parallel.is_parallel());
        assert!(!parallel.is_structured());
        assert!(!StateValue::Simple("x".into()).is_structured());
    }

    #[test]
    fn state_value_display_peels_back() {
        assert_eq!(StateValue::Null.to_string(), "");
        assert_eq!(StateValue::Ignored.to_string(), "IGNORED");
        assert_eq!(StateValue::Bool(true).to_string(), "true");
        assert_eq!(StateValue::Bool(false).to_string(), "false");
        assert_eq!(
            StateValue::Simple("vertical".into()).to_string(),
            "vertical"
        );

        // Structured-with-only-id renders as the id alone.
        let only_id = StateValue::Structured({
            let mut m = Map::new();
            m.insert("id".into(), json!("car"));
            m
        });
        assert_eq!(only_id.to_string(), "car");

        // Structured-with-extras renders id followed by {k: v, ...}.
        let mut obj = Map::new();
        obj.insert("id".into(), json!("car"));
        obj.insert("role".into(), json!("driver"));
        let with_extras = StateValue::Structured(obj);
        // Map iteration is insertion-order; `role` is the only extra here.
        assert_eq!(with_extras.to_string(), "car {role: driver}");

        // Parallel renders as comma-separated peeled children.
        let parallel = StateValue::Parallel(vec![
            StateValue::Simple("vertical".into()),
            StateValue::Simple("sit".into()),
        ]);
        assert_eq!(parallel.to_string(), "vertical, sit");
    }

    #[test]
    fn state_value_partial_eq_is_structural() {
        // Repeated-state check semantics: drive car == drive car, but
        // {id:"car",role:"driver"} != {id:"car",role:"passenger"}.
        let drive_car = StateValue::Structured({
            let mut m = Map::new();
            m.insert("id".into(), json!("car"));
            m.insert("role".into(), json!("driver"));
            m
        });
        let drive_car_again = StateValue::Structured({
            let mut m = Map::new();
            m.insert("id".into(), json!("car"));
            m.insert("role".into(), json!("driver"));
            m
        });
        let ride_car = StateValue::Structured({
            let mut m = Map::new();
            m.insert("id".into(), json!("car"));
            m.insert("role".into(), json!("passenger"));
            m
        });
        assert_eq!(drive_car, drive_car_again);
        assert_ne!(drive_car, ride_car);
    }

    // ─── da-2i8g: following_state-aware events ──────────────────────

    fn block_switch_entry(
        occur: DateTime<Utc>,
        primary: &str,
        dur_secs: i64,
        following: &str,
    ) -> Entry {
        use crate::dur::Dur;
        use crate::entry::Verb;
        use crate::time::DatumTime;
        let mut e = Entry::new(
            "body",
            Verb::BlockSwitch,
            Some(json!(primary)),
            None,
            Some(DatumTime {
                utc: occur,
                offset_minutes: 0,
                timezone: Some("UTC".into()),
            }),
            Some(Dur::Of(chrono::Duration::seconds(dur_secs))),
            "dev",
        );
        e.following_state = Some(json!(following));
        e
    }

    #[test]
    fn collect_events_emits_following_state_at_end_of_block_switch() {
        let occur: DateTime<Utc> = "2026-04-01T09:00:00Z".parse().unwrap();
        let entry = block_switch_entry(occur, "sit", 30 * 60, "vertical");

        let mut events: Vec<(DateTime<Utc>, DateTime<Utc>, Value)> = Vec::new();
        collect_events(&entry, &mut events);

        assert_eq!(events.len(), 2, "block-switch emits primary + trailing");
        assert_eq!(events[0].0, occur, "first event at occur_time");
        assert_eq!(events[0].2, json!("sit"));
        assert_eq!(
            events[1].0,
            occur + chrono::Duration::seconds(30 * 60),
            "trailing event at occur_time + dur"
        );
        assert_eq!(events[1].2, json!("vertical"));
    }

    #[test]
    fn collect_events_point_switch_emits_both_at_same_instant() {
        use crate::dur::Dur;
        use crate::entry::Verb;
        use crate::time::DatumTime;
        let occur: DateTime<Utc> = "2026-04-01T09:00:00Z".parse().unwrap();
        let mut e = Entry::new(
            "mood",
            Verb::PointSwitch,
            Some(json!("happy")),
            None,
            Some(DatumTime {
                utc: occur,
                offset_minutes: 0,
                timezone: Some("UTC".into()),
            }),
            Some(Dur::Of(chrono::Duration::zero())),
            "dev",
        );
        e.following_state = Some(json!("sad"));

        let mut events: Vec<(DateTime<Utc>, DateTime<Utc>, Value)> = Vec::new();
        collect_events(&e, &mut events);

        assert_eq!(events.len(), 2, "point-switch emits primary + trailing");
        assert_eq!(events[0].0, occur);
        assert_eq!(events[1].0, occur, "point-switch trailing at same instant");
        assert_eq!(events[0].2, json!("happy"));
        assert_eq!(events[1].2, json!("sad"));
    }

    #[test]
    fn collect_events_block_switch_without_following_state_emits_only_primary() {
        use crate::dur::Dur;
        use crate::entry::Verb;
        use crate::time::DatumTime;
        let occur: DateTime<Utc> = "2026-04-01T09:00:00Z".parse().unwrap();
        // Verb-derivation rejects this shape today (block-switch requires
        // following_state), but the row-level defensive guard means a
        // hand-rolled or migrated row without following_state still
        // emits the primary cleanly instead of panicking.
        let mut e = Entry::new(
            "body",
            Verb::BlockSwitch,
            Some(json!("sit")),
            None,
            Some(DatumTime {
                utc: occur,
                offset_minutes: 0,
                timezone: Some("UTC".into()),
            }),
            Some(Dur::Of(chrono::Duration::seconds(30 * 60))),
            "dev",
        );
        e.following_state = None;

        let mut events: Vec<(DateTime<Utc>, DateTime<Utc>, Value)> = Vec::new();
        collect_events(&e, &mut events);

        assert_eq!(events.len(), 1, "no trailing emitted when fs missing");
        assert_eq!(events[0].2, json!("sit"));
    }

    // ─── da-a0n3: transition stream for `_` ordinals ─────────────────

    fn point_entry(occur: DateTime<Utc>, state: Option<Value>) -> Entry {
        use crate::dur::Dur;
        use crate::entry::Verb;
        use crate::time::DatumTime;
        Entry::new(
            "mood",
            Verb::Point,
            state,
            None,
            Some(DatumTime {
                utc: occur,
                offset_minutes: 0,
                timezone: Some("UTC".into()),
            }),
            Some(Dur::Moment),
            "dev",
        )
    }

    #[test]
    fn occur_points_are_annotations_not_transitions() {
        // Occur-shaped rows (Point + no state, or the literal `true`
        // legacy occur wrote) emit a state=false compile event but are
        // invisible to the transition stream `_` ordinals sample.
        let occur: DateTime<Utc> = "2026-04-01T09:00:00Z".parse().unwrap();
        for state in [None, Some(json!(true))] {
            let e = point_entry(occur, state);
            assert_eq!(entry_state_events(&e), vec![(occur, json!(false))]);
            assert!(entry_transition_events(&e).is_empty());
        }
    }

    #[test]
    fn moment_switch_emits_its_real_state_as_a_transition() {
        // `switch mood happy --moment` derives Point with a real state:
        // a genuine zero-length transition. The compile event carries the
        // state itself (not `false`), matching the SQL oracle's answer
        // for the same row.
        let occur: DateTime<Utc> = "2026-04-01T09:00:00Z".parse().unwrap();
        let e = point_entry(occur, Some(json!("happy")));
        assert_eq!(entry_state_events(&e), vec![(occur, json!("happy"))]);
        assert_eq!(entry_transition_events(&e), vec![(occur, json!("happy"))]);
    }

    #[test]
    fn compound_switch_trailing_event_is_a_transition() {
        let occur: DateTime<Utc> = "2026-04-01T09:00:00Z".parse().unwrap();
        let entry = block_switch_entry(occur, "sit", 30 * 60, "vertical");
        assert_eq!(
            entry_transition_events(&entry),
            vec![
                (occur, json!("sit")),
                (
                    occur + chrono::Duration::seconds(30 * 60),
                    json!("vertical")
                ),
            ],
            "primary AND following_state are both ordinal candidates"
        );
    }

    #[test]
    fn state_at_after_block_switch_returns_following_state() {
        // End-to-end through FieldStateEvents::at: a probe inside the
        // block sees the primary; a probe after the block sees the
        // trailing.
        let occur: DateTime<Utc> = "2026-04-01T09:00:00Z".parse().unwrap();
        let mid = occur + chrono::Duration::minutes(15);
        let after = occur + chrono::Duration::minutes(31);
        let entry = block_switch_entry(occur, "sit", 30 * 60, "vertical");

        let mut events = Vec::new();
        collect_events(&entry, &mut events);
        // Mirror what FieldStateEvents::at does: scan from the tail and
        // pick the latest event at or before the probe.
        let pick = |t: DateTime<Utc>| -> Option<&Value> {
            events
                .iter()
                .rev()
                .find(|(et, _, _)| *et <= t)
                .map(|(_, _, v)| v)
        };

        assert_eq!(pick(mid), Some(&json!("sit")));
        assert_eq!(pick(after), Some(&json!("vertical")));
    }
}
