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

use crate::dur::Dur;
use crate::entry::{Entry, Verb};
use crate::error::DatumError;
use crate::error::Result;
use crate::shape::IGNORED_STATE_VALUE;

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

// ─── The state oracle (da-u5l1o) ─────────────────────────────────────────
//
// ONE function decides what state a field holds at an instant:
// [`resolve_effective`]. Every reader hands it rows — the windowed SQL
// read behind [`crate::db::Db::state_at`], or the per-field cache
// [`FieldStateRows`] for callers that probe many times — and nothing
// else computes a state from rows.

/// How many dur-bearing rows stored AFTER the probe instant a state read
/// pulls in so a block still running at that instant is seen. da-zwct: a
/// block's stored `occur` is its END, so the row that covers `at` can sit
/// in `at`'s future; the nearest few forward blocks are the only ones
/// whose span can reach back over it (overlapping blocks are an integrity
/// warning, not a shape the reader has to unravel).
pub const SPAN_LOOKAHEAD: usize = 8;

/// How many rows a state read may walk back through to find the state a
/// block handed the field back to. Each completed block immediately
/// preceding the read consumes one; a real ledger never stacks anywhere
/// near this many back to back, and past the limit the read reports "no
/// state" rather than inventing one.
pub const RESUME_LOOKBACK: usize = 64;

/// SQL for [`StateRow::is_occur_dot`]. The depth-limited state reads leave
/// dots out, since an occur never decides a state and a field logged by
/// occurs would otherwise fill the whole lookback with them. They fetch
/// the one dot that matters, the first after an IGNORED, on its own.
/// The partial index `entries_field_state` spells it too, and SQLite uses
/// that index only while the two spellings match (da-asgt4).
pub(crate) const OCCUR_DOT_SQL: &str = "(verb = 'point' AND (state IS NULL OR state = 'true'))";

/// One state-bearing row, in as much of it as the oracle reads. The
/// input of [`resolve_effective`], so `now` (which holds whole
/// [`Entry`]s), [`crate::db::Db::batch_state_at`] (which selects seven
/// columns) and [`FieldStateRows`] resolve through the SAME code.
#[derive(Debug, Clone)]
pub struct StateRow {
    /// The row's whole hid, so a caller that wants the ENTRY behind a
    /// resolved state can name it (da-6hg6v).
    pub id: String,
    pub verb: Verb,
    pub state: Option<Value>,
    pub occur: Option<DateTime<Utc>>,
    pub dur: Option<Dur>,
    pub following_state: Option<Value>,
}

impl StateRow {
    pub fn from_entry(e: &Entry) -> Self {
        StateRow {
            id: e.hid.to_string(),
            verb: e.verb,
            state: e.state.clone(),
            occur: e.occur_time.as_ref().map(|t| t.utc),
            dur: e.dur.clone(),
            following_state: e.following_state.clone(),
        }
    }

    /// Where a row that carries a duration ends — its `occur`, which IS
    /// the block's trailing edge (da-zwct: the duration precedes it). A
    /// MOMENT is a span of zero, so it ends where it stands: that is where
    /// a first log's point-switch hands over to the `following_state` it
    /// declares (da-gvcen). `None` for no `dur` or an unknown one.
    fn trailing_edge(&self) -> Option<DateTime<Utc>> {
        let occur = self.occur?;
        let dur = self.dur.as_ref()?;
        (dur.is_moment() || dur.as_seconds().is_some()).then_some(occur)
    }

    /// Where this row's influence begins: `occur - dur` for a block,
    /// `occur` for a point or an open block.
    pub fn leading_edge(&self) -> Option<DateTime<Utc>> {
        let occur = self.occur?;
        let secs = self.dur.as_ref().and_then(|d| d.as_seconds()).unwrap_or(0);
        Some(occur - chrono::Duration::seconds(secs))
    }

    /// Does this row hand the field back once its trailing edge is past?
    ///
    /// da-611d: a block is an INTERRUPTION, not a switch. `end work 12`
    /// writes `following_state IS NULL`, and NULL there is a *specified
    /// default* meaning "resume whatever state preceded this block" —
    /// the common case, because the state to come back to is simply the
    /// prior one and does not need restating. So past its trailing edge
    /// this row is no longer the one that decides the state: the row
    /// before it is.
    ///
    /// [`Verb::BlockSwitch`] / [`Verb::PointSwitch`] carry a real
    /// `following_state` and switch to it instead ([`effective_state`]),
    /// so they never hand back — [`crate::shape::hands_field_back`] owns
    /// that verb split, shared with `stats` (da-mkqo).
    ///
    /// da-zwct: the trailing edge is `occur` itself, so a completed block
    /// has already handed the field back by the instant it stores.
    ///
    /// A moment ([`StateRow::is_moment_state`]) hands the field back at
    /// any instant: it never decides a state, before itself or after.
    /// Wall-clock `now` reads rows dated ahead of it (da-e47ap).
    pub fn hands_field_back(&self, at: DateTime<Utc>) -> bool {
        if self.is_moment_state() {
            return true;
        }
        if !crate::shape::hands_field_back(self.verb) || self.following_state.is_some() {
            return false;
        }
        match (self.occur, self.dur.as_ref().and_then(|d| d.as_seconds())) {
            // A zero-span row occupies no time, so it interrupts nothing.
            (Some(occur), Some(secs)) if secs > 0 => at >= occur,
            _ => false,
        }
    }

    /// An occur dot: a point with no state, or the literal `true` legacy
    /// occur wrote. vividn, 2026-09-29: «an occur should not modify the
    /// state (except in the case that state is IGNORED, in which case it
    /// transitions to TRACKED)» — [`resolve_effective`] passes over it.
    pub fn is_occur_dot(&self) -> bool {
        self.verb == Verb::Point && matches!(self.state, None | Some(Value::Bool(true)))
    }

    /// A moment: a point that names a state (dur MOMENT, or a zero one,
    /// which vividn rules the same). vividn, 2026-10-08: the state before it
    /// resumes — it does not switch the field (da-22750).
    pub fn is_moment_state(&self) -> bool {
        self.verb == Verb::Point && !self.is_occur_dot()
    }

    /// An occur-shaped row: an occur dot, or a row with no state at all.
    /// It moves the field's state like any row, but it is not a state
    /// anyone switched TO, so the `_` ordinals
    /// ([`crate::db::Db::resolve_state_back`], da-a0n3) pass over what it
    /// sets. A compound switch is never one: its `following_state` is a
    /// deliberate state.
    pub fn is_annotation(&self) -> bool {
        match self.verb {
            Verb::Point => self.is_occur_dot(),
            Verb::Switch | Verb::Block => self.state.is_none(),
            Verb::BlockSwitch | Verb::PointSwitch => false,
        }
    }

    /// When this row's effective state at `at` took effect: its trailing
    /// edge once a compound switch has crossed it, otherwise its leading
    /// edge — `occur - dur` for a block, `occur` for anything else.
    fn effective_since(&self, at: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if crate::shape::opens_secondary_switch_region(self.verb) && self.following_state.is_some()
        {
            if let Some(trailing) = self.trailing_edge() {
                if at >= trailing {
                    return Some(trailing);
                }
            }
        }
        self.leading_edge()
    }
}

/// What a field's rows say it holds at one instant.
pub struct Effective {
    pub state: Value,
    /// When the field entered `state` — the resume boundary when a block
    /// handed it back, otherwise the governing row's own event time.
    pub since: Option<DateTime<Utc>>,
    /// Index into the input rows of the row that established `state`.
    /// `None` when every row handed the field back and nothing preceded
    /// them: the field holds no state at all.
    pub governing: Option<usize>,
}

/// THE rule: the state a field holds at `at`, from its rows at or before
/// `at` plus any block spanning it, NEWEST FIRST (the order
/// [`crate::db::STATE_ORDER_NEWEST_FIRST`] reads them in).
///
/// Walks back past every block that has already handed the field back
/// ([`StateRow::hands_field_back`]), and every occur dot
/// ([`StateRow::is_occur_dot`]), to the row that actually decides the
/// state, then reads that row with [`effective_state`]. An occur does not
/// modify the state, except IGNORED or no state at all: then the first dot
/// after that took effect makes the field TRACKED from that dot on (vividn,
/// 2026-09-29: «if the state is IGNORED (or nothing because no data exists
/// which is functionally equivalent), then it goes to TRACKED on an
/// occur» — v2's behaviour too).
///
/// "Newest" is when a row's state at `at` TOOK EFFECT
/// ([`StateRow::effective_since`]), not its stored instant: a block's
/// duration precedes `occur` (da-zwct), so a block still running at `at`
/// stores an instant in `at`'s future while its influence began before
/// it; and a compound switch past its trailing edge said its last word
/// there, so a switch logged inside its span does not outrank it. The
/// walk re-orders on that instant (stably, so equal instants keep the
/// query's own newest-first tiebreak). Keyed this way, the newest stored
/// row at or before `at` always outranks every older one, which is what
/// lets a reader fetch one row and look deeper only when it hands back.
/// `governing` still indexes `rows` as given.
pub fn resolve_effective<R: std::borrow::Borrow<StateRow>>(
    rows: &[R],
    at: DateTime<Utc>,
) -> Effective {
    let row = |i: usize| rows[i].borrow();
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|a, b| {
        row(*b)
            .effective_since(at)
            .cmp(&row(*a).effective_since(at))
            .then(a.cmp(b))
    });
    // The NEWEST completed block fixes when the resumed span began;
    // older ones ended before it and are already covered.
    let mut resumed_at: Option<DateTime<Utc>> = None;
    let mut dots: Vec<usize> = Vec::new();
    for i in order {
        let r = row(i);
        if r.is_occur_dot() {
            dots.push(i);
            continue;
        }
        if r.hands_field_back(at) {
            // A moment interrupted nothing, so the state it passes over
            // holds from where it began.
            if !r.is_moment_state() {
                resumed_at = resumed_at.or_else(|| r.trailing_edge());
            }
            continue;
        }
        let eff = Effective {
            state: effective_state(r, at),
            since: resumed_at.max(r.effective_since(at)),
            governing: Some(i),
        };
        return eff.tracked_by(&dots, row);
    }
    // Nothing preceded the block(s): there is no state to resume to.
    // `Null` is "tracked, no current state" — the honest answer, and the
    // one a timeline resolver derives for the same rows.
    Effective {
        state: Value::Null,
        since: resumed_at,
        governing: None,
    }
    .tracked_by(&dots, row)
}

impl Effective {
    /// Is this a state an occur dot moves — IGNORED, or no state at all
    /// (nothing decided it)? A reader that leaves dots out fetches the
    /// first one after [`Effective::since`] when it is.
    pub fn an_occur_tracks(&self) -> bool {
        self.governing.is_none() || crate::shape::is_ignored_state(&self.state)
    }

    /// TRACKED from the first of `dots` after `since`, when an occur moves
    /// this state ([`Effective::an_occur_tracks`]); otherwise unchanged.
    fn tracked_by<'a>(self, dots: &[usize], row: impl Fn(usize) -> &'a StateRow) -> Effective {
        if !self.an_occur_tracks() {
            return self;
        }
        let first = dots
            .iter()
            .copied()
            .filter(|d| row(*d).occur > self.since)
            .min_by_key(|d| row(*d).occur);
        match first {
            Some(d) => Effective {
                state: Value::String(crate::shape::TRACKED_STATE_VALUE.into()),
                since: row(d).occur,
                governing: Some(d),
            },
            None => self,
        }
    }
}

/// Read ONE row's state at `at`, honouring `following_state` for
/// block-switch / point-switch once the moment has crossed the trailing
/// edge (da-2i8g). Otherwise the row's own `state` is the state.
fn effective_state(row: &StateRow, at: DateTime<Utc>) -> Value {
    // `following_state` takes over past the trailing edge for exactly the
    // verbs that open a secondary switch region — shape.rs owns the set.
    if crate::shape::opens_secondary_switch_region(row.verb) {
        if let (Some(trailing), Some(fs)) = (row.trailing_edge(), row.following_state.as_ref()) {
            // da-zwct: the trailing edge is the stored `occur` — the
            // block's duration runs backward from it, not forward.
            if at >= trailing {
                return fs.clone();
            }
        }
    }
    row.state.clone().unwrap_or(Value::Null)
}

/// One field's state-bearing rows, loaded once so a caller that probes
/// many instants — the tail and log render, integrity detection, backup
/// restore, `tail field:state`, the `_` ordinals — pays one read instead
/// of one per probe. It decides nothing: every probe hands
/// [`resolve_effective`] the SAME window of rows the SQL read behind
/// [`crate::db::Db::state_at`] selects (the newest row at or before the
/// probe, [`RESUME_LOOKBACK`] of them when that one hands the field back,
/// and the [`SPAN_LOOKAHEAD`] oldest blocks stored after it, occur dots
/// aside; then, if that says IGNORED, the first dot after it), so the two
/// fetches cannot answer differently.
#[derive(Default, Debug, Clone)]
pub struct FieldStateRows {
    /// Newest first: `occur` descending, slug descending on a tie —
    /// [`crate::db::STATE_ORDER_NEWEST_FIRST`]. Every row has an `occur`,
    /// and none is an occur dot.
    rows: Vec<StateRow>,
    /// The occur dots ([`StateRow::is_occur_dot`]), oldest first.
    dots: Vec<StateRow>,
    /// Where in `rows` the blocks are, the rows with a `dur`, so a read
    /// looks ahead through those alone (da-u65pa).
    blocks: Vec<usize>,
}

#[cfg(test)]
thread_local! {
    /// The rows [`FieldStateRows::window`] has looked at, for the test that
    /// holds a read's cost to the rows it returns (da-u65pa).
    static WALKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One change in a field's state, as [`FieldStateRows::changes`] lists it.
pub struct StateChange<'a> {
    /// When the new state took effect.
    pub at: DateTime<Utc>,
    /// The state from `at` on — [`resolve_effective`]'s answer there.
    pub state: Value,
    /// The row whose edge this is: the one that set the state, or the
    /// block that handed the field back.
    pub row: &'a StateRow,
    /// The row that decided `state`: `row` itself, except where a block
    /// handed the field back — then the row it resumed, `None` when
    /// nothing preceded the block.
    pub governing: Option<&'a StateRow>,
}

impl FieldStateRows {
    /// The field's rows from entries already in hand — the browser's
    /// door, which has the field's entries and no connection. Pass the
    /// field's live rows in any order; timeless rows are skipped, as the
    /// SQL loaders skip them.
    pub fn from_entries<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Self {
        let mut timed: Vec<&Entry> = entries
            .into_iter()
            .filter(|e| e.occur_time.is_some())
            .collect();
        timed.sort_by_cached_key(|e| {
            std::cmp::Reverse((e.occur_time.as_ref().map(|t| t.utc), e.slug.to_string()))
        });
        Self::new(timed.into_iter().map(StateRow::from_entry).collect())
    }

    /// From rows newest first: the occur dots set aside, oldest first.
    fn new(newest_first: Vec<StateRow>) -> Self {
        let (mut dots, rows): (Vec<_>, Vec<_>) =
            newest_first.into_iter().partition(StateRow::is_occur_dot);
        dots.reverse();
        let blocks = (0..rows.len()).filter(|&i| rows[i].dur.is_some()).collect();
        FieldStateRows { rows, dots, blocks }
    }

    /// Every state-participating row of `field`.
    pub fn load(conn: &Connection, field: &str) -> Result<Self> {
        Self::load_rows(conn, field, false)
    }

    /// [`FieldStateRows::load`] over the field's TRANSITIONS only: the
    /// TRACK document that declares a field is not one (da-c8mh), so a
    /// reader asking "what was this field doing just before this row" is
    /// not answered with "it exists".
    pub fn load_transitions(conn: &Connection, field: &str) -> Result<Self> {
        Self::load_rows(conn, field, true)
    }

    fn load_rows(conn: &Connection, field: &str, transitions_only: bool) -> Result<Self> {
        let declarations = if transitions_only {
            format!("AND {}", crate::track::NOT_A_DECLARATION_SQL)
        } else {
            String::new()
        };
        let mut stmt = conn.prepare(&Self::select(
            &declarations,
            crate::db::STATE_ORDER_NEWEST_FIRST,
        ))?;
        let rows = stmt
            .query_map(params![field], crate::db::read_state_columns)?
            .map(Self::decode)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self::new(rows))
    }

    /// [`Self::load`] for a reader that asks about `[from, until]` and
    /// nothing else ([`Self::changes_between`]): the rows stored in it;
    /// after it, the [`SPAN_LOOKAHEAD`] oldest blocks; before it, the
    /// newest row that is not an occur dot ([`RESUME_LOOKBACK`] of them
    /// when a block among the rows can hand the field back), the dots
    /// after the earliest instant any of those rows can have taken effect,
    /// and — when that is all its history — the field's first dot. That
    /// is every row [`Self::resolve`] reaches in the window, so the
    /// answers there are [`Self::load`]'s, at the cost of the window
    /// instead of the field's whole history.
    pub fn load_between(
        conn: &Connection,
        field: &str,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Self> {
        Self::load_rows_between(conn, field, from, until, false)
    }

    /// [`Self::load_between`] over the field's TRANSITIONS only, as
    /// [`Self::load_transitions`] is [`Self::load`]'s.
    pub fn load_transitions_between(
        conn: &Connection,
        field: &str,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Self> {
        Self::load_rows_between(conn, field, from, until, true)
    }

    fn load_rows_between(
        conn: &Connection,
        field: &str,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
        transitions_only: bool,
    ) -> Result<Self> {
        let declarations = match transitions_only {
            true => format!("AND {}", crate::track::NOT_A_DECLARATION_SQL),
            false => String::new(),
        };
        let read = |and: &str, order: &str, bounds: &[&str]| -> Result<Vec<StateRow>> {
            let and = format!("{declarations} {and}");
            let mut stmt = conn.prepare_cached(&Self::select(&and, order))?;
            let params = std::iter::once(field).chain(bounds.iter().copied());
            let rows = stmt
                .query_map(
                    rusqlite::params_from_iter(params),
                    crate::db::read_state_columns,
                )?
                .map(Self::decode)
                .collect();
            rows
        };
        let newest = crate::db::STATE_ORDER_NEWEST_FIRST;
        let first = |n: usize| format!("{} LIMIT {n}", crate::db::STATE_ORDER_OLDEST_FIRST);
        let last = |n: usize| format!("{newest} LIMIT {n}");
        let (lo, hi) = (from.to_rfc3339(), until.to_rfc3339());

        // Blocks, as `window` reads them: an occur's MOMENT is not one.
        let after = format!("AND occur_time > ?2 AND dur IS NOT NULL AND NOT {OCCUR_DOT_SQL}");
        let mut rows = read(&after, &first(SPAN_LOOKAHEAD), &[&hi])?;
        rows.reverse();
        let within = "AND occur_time >= ?2 AND occur_time <= ?3";
        rows.extend(read(within, newest, &[&lo, &hi])?);
        // As deep as the SQL read goes: one row, or RESUME_LOOKBACK when
        // the row a probe lands on can be a block handing the field back.
        let before = format!("AND occur_time < ?2 AND NOT {OCCUR_DOT_SQL}");
        let mut depth = 1;
        let mut older = read(&before, &last(depth), &[&lo])?;
        let deep = |r: &StateRow| r.hands_field_back(DateTime::<Utc>::MAX_UTC);
        if rows.iter().chain(&older).any(deep) {
            depth = RESUME_LOOKBACK;
            older = read(&before, &last(depth), &[&lo])?;
        }
        // Fewer: the field's history before `from` is all here, and so is
        // the "no state yet" its first dot moves.
        let whole = older.len() < depth;
        rows.extend(older);
        // An occur dot moves only a state it comes after: IGNORED, or none
        // at all, which a hand-back can leave (a short history's is the
        // first dot's, below).
        let ignored = |v: &Option<Value>| v.as_ref().is_some_and(crate::shape::is_ignored_state);
        let dotted = depth > 1
            || rows
                .iter()
                .any(|r| ignored(&r.state) || ignored(&r.following_state));
        let earliest = rows
            .iter()
            .filter_map(StateRow::leading_edge)
            .fold(from, DateTime::min)
            .to_rfc3339();
        if dotted {
            let dots = format!("AND occur_time < ?2 AND occur_time >= ?3 AND {OCCUR_DOT_SQL}");
            rows.extend(read(&dots, newest, &[&lo, &earliest])?);
        }
        if whole {
            let first_dot = format!("AND occur_time < ?2 AND {OCCUR_DOT_SQL}");
            rows.extend(read(&first_dot, &first(1), &[&earliest])?);
        }
        Ok(Self::new(rows))
    }

    /// A field's state-participating rows, narrowed by `and`, in `order`.
    fn select(and: &str, order: &str) -> String {
        format!(
            "SELECT field, state, verb, occur_time, dur, following_state, hid
               FROM entries
              WHERE field = ?1
                AND {participation}
                {and}
              ORDER BY {order}",
            participation = crate::db::STATE_PARTICIPATION_SQL,
        )
    }

    fn decode(cols: rusqlite::Result<crate::db::StateColumns>) -> Result<StateRow> {
        let (_, row) = crate::db::decode_state_row(cols.map_err(DatumError::Db)?)?;
        Ok(row)
    }

    /// The window [`resolve_effective`] reads at `at`, with `exclude`
    /// left out as though it were not in the table. Mirrors
    /// [`crate::db::Db::batch_governing_at`]'s two SQL reads row for row;
    /// [`Self::resolve`] adds the dot an IGNORED needs.
    fn window(&self, at: DateTime<Utc>, exclude: Option<&str>) -> Vec<&StateRow> {
        let kept = |r: &&StateRow| exclude != Some(r.id.as_str());
        let walked = |_: &&StateRow| {
            #[cfg(test)]
            WALKED.with(|n| n.set(n.get() + 1));
        };
        let split = self.rows.partition_point(|r| r.occur > Some(at));
        let past = self.rows[split..].iter().inspect(walked).filter(kept);
        let deep = past.clone().next().is_some_and(|r| r.hands_field_back(at));
        let mut out: Vec<&StateRow> = past.take(if deep { RESUME_LOOKBACK } else { 1 }).collect();
        let newer = self.blocks.partition_point(|&i| i < split);
        out.extend(
            self.blocks[..newer]
                .iter()
                .rev()
                .map(|&i| &self.rows[i])
                .inspect(walked)
                .filter(kept)
                .take(SPAN_LOOKAHEAD)
                .filter(|r| r.leading_edge().is_some_and(|edge| edge <= at)),
        );
        out
    }

    /// The field's state at `at`, as though `exclude` had never been
    /// written (so detection run on a just-inserted row sees the state as
    /// it was without it). `None` when the field holds no state there —
    /// no row yet, or the rows say `Null`.
    pub fn at(&self, at: DateTime<Utc>, exclude: Option<&str>) -> Option<Value> {
        Some(self.resolve(at, exclude).1.state).filter(|s| !s.is_null())
    }

    /// [`resolve_effective`] over [`Self::window`], and over it plus the
    /// first occur dot after the answer took effect when an occur moves
    /// that answer ([`Effective::an_occur_tracks`]). `governing` indexes
    /// the returned rows.
    fn resolve(&self, at: DateTime<Utc>, exclude: Option<&str>) -> (Vec<&StateRow>, Effective) {
        let mut window = self.window(at, exclude);
        let eff = resolve_effective(&window, at);
        if !eff.an_occur_tracks() {
            return (window, eff);
        }
        let after = self.dots.partition_point(|d| d.occur <= eff.since);
        let dot = self.dots[after..]
            .iter()
            .take_while(|d| d.occur <= Some(at))
            .find(|d| exclude != Some(d.id.as_str()));
        match dot {
            Some(dot) => {
                window.push(dot);
                let eff = resolve_effective(&window, at);
                (window, eff)
            }
            None => (window, eff),
        }
    }

    /// Every change in the field's state, oldest first: the oracle read at
    /// each row's edges (a leading edge, and a block's trailing edge),
    /// kept where the answer differs from the one before it — in state, or
    /// in whether a row decides it.
    pub fn changes(&self) -> Vec<StateChange<'_>> {
        self.changes_within(None, None)
    }

    /// [`Self::changes`] in `[from, until]`, for a reader drawing a window
    /// that need not read the field's history around it
    /// ([`Self::load_between`]): the state in effect at `from`, as a
    /// change AT `from` (none when no row decides one), then every change
    /// after it up to `until`.
    pub fn changes_between(
        &self,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Vec<StateChange<'_>> {
        self.changes_within(Some(from), Some(until))
    }

    fn changes_within(
        &self,
        from: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Vec<StateChange<'_>> {
        let mut out: Vec<StateChange<'_>> = Vec::new();
        if let Some(from) = from {
            let (window, eff) = self.resolve(from, None);
            if let Some(row) = eff.governing.map(|i| window[i]) {
                out.push(StateChange {
                    at: from,
                    state: eff.state,
                    row,
                    governing: Some(row),
                });
            }
        }
        let mut edges: Vec<(DateTime<Utc>, &StateRow)> = Vec::new();
        for r in self.rows.iter().rev().chain(&self.dots) {
            edges.extend(r.leading_edge().map(|t| (t, r)));
            edges.extend(
                r.trailing_edge()
                    .filter(|t| Some(*t) != r.leading_edge())
                    .map(|t| (t, r)),
            );
        }
        edges.retain(|(t, _)| from.is_none_or(|f| *t > f) && until.is_none_or(|u| *t <= u));
        edges.sort_by_key(|(t, _)| *t);
        for group in edges.chunk_by(|a, b| a.0 == b.0) {
            let at = group[0].0;
            let (window, eff) = self.resolve(at, None);
            // A Null a row decides and one none does are two changes
            // (da-3k9k1): a reader keyed on governance tells them apart.
            if out.last().is_some_and(|c| {
                c.state == eff.state && c.governing.is_some() == eff.governing.is_some()
            }) {
                continue;
            }
            let governing = eff.governing.map(|i| window[i]);
            let row = group
                .iter()
                .map(|(_, r)| *r)
                .find(|r| governing.is_some_and(|g| std::ptr::eq(*r, g)))
                .unwrap_or(group[group.len() - 1].1);
            out.push(StateChange {
                at,
                state: eff.state,
                row,
                governing,
            });
        }
        out
    }
}

/// S12: true when two (normalized) states share at least one element. Scalar
/// values are treated as one-element sets; `Parallel` values are treated as
/// sets of their elements. Elements compare via [`StateValue`] structural
/// equality, so a `Structured({id: "car", role: "driver"})` matches another
/// `Structured` only when every field agrees — `.id` alone is not enough.
///
/// Exported so the renderer can flag a repeated-state switch (`da-qfv0`)
/// using the same predicate that drives `SameStateRepeat` warning records.
pub fn states_share_element(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let sa = state_as_set(a);
    let sb = state_as_set(b);
    sa.iter().any(|x| sb.iter().any(|y| x == y))
}

/// S12: set-aware equality on top of [`StateValue`] PartialEq — `Parallel`
/// values compare order-insensitive (same multiset of elements), everything
/// else compares structurally.
///
/// Exported so the renderer's `!` marker uses the same predicate as
/// [`IntegrityKind::SameStateRepeat`](crate::integrity::IntegrityKind::SameStateRepeat) (da-g1b8).
pub fn states_equal_set_aware(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let Ok(sva) = StateValue::from_json(a) else {
        return false;
    };
    let Ok(svb) = StateValue::from_json(b) else {
        return false;
    };
    match (&sva, &svb) {
        (StateValue::Parallel(va), StateValue::Parallel(vb)) => state_multiset_eq(va, vb),
        _ => sva == svb,
    }
}

/// Decompose a state into its element multiset for set-aware comparisons.
/// `Parallel` flattens to its members; everything else is a one-element set.
/// On a malformed value (none should reach here in practice — the upstream
/// shape is already a valid JSON state) the fallback is an empty set so the
/// element-share / equality predicates short-circuit to "no match".
fn state_as_set(v: &serde_json::Value) -> Vec<StateValue> {
    match StateValue::from_json(v) {
        Ok(StateValue::Parallel(items)) => items,
        Ok(other) => vec![other],
        Err(_) => Vec::new(),
    }
}

/// Multiset equality for two `Parallel` element vectors. Ours are tiny
/// (a handful of states), so the O(n²) match-and-mark walk is fine and
/// avoids needing `Hash` on `StateValue`.
fn state_multiset_eq(va: &[StateValue], vb: &[StateValue]) -> bool {
    if va.len() != vb.len() {
        return false;
    }
    let mut used = vec![false; vb.len()];
    'outer: for x in va {
        for (i, y) in vb.iter().enumerate() {
            if !used[i] && x == y {
                used[i] = true;
                continue 'outer;
            }
        }
        return false;
    }
    true
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

    // ─── The oracle and its row cache (da-u5l1o) ─────────────────────

    fn row(
        field: &str,
        verb: Verb,
        state: Option<Value>,
        occur: &str,
        dur_mins: Option<i64>,
        following: Option<Value>,
    ) -> Entry {
        use crate::time::DatumTime;
        let mut e = Entry::new(
            field,
            verb,
            state,
            None,
            Some(DatumTime {
                utc: occur.parse().unwrap(),
                offset_minutes: 0,
                timezone: Some("UTC".into()),
                uncertainty: None,
            }),
            dur_mins.map(|m| Dur::of(chrono::Duration::minutes(m))),
            "dev",
        );
        e.following_state = following;
        e
    }

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    /// da-zwct: `switch body sit 30 , vertical` logged at 09:00 held `sit`
    /// from 08:30 and went `vertical` AT 09:00.
    #[test]
    fn block_switch_holds_its_state_then_its_following_state() {
        let e = row(
            "body",
            Verb::BlockSwitch,
            Some(json!("sit")),
            "2026-04-01T09:00:00Z",
            Some(30),
            Some(json!("vertical")),
        );
        let rows = FieldStateRows::from_entries([&e]);
        assert_eq!(rows.at(t("2026-04-01T08:29:00Z"), None), None);
        assert_eq!(rows.at(t("2026-04-01T08:45:00Z"), None), Some(json!("sit")));
        assert_eq!(
            rows.at(t("2026-04-01T09:00:00Z"), None),
            Some(json!("vertical"))
        );
    }

    /// vividn's report: a timed switch with no `following_state` ends, and
    /// the field goes back to the state it interrupted — it does not stay
    /// in the block's state forever.
    #[test]
    fn a_finished_block_hands_the_field_back() {
        let before = row(
            "f",
            Verb::Switch,
            Some(json!("state1")),
            "2026-04-01T09:00:00Z",
            None,
            None,
        );
        let block = row(
            "f",
            Verb::Block,
            Some(json!("state2")),
            "2026-04-01T09:10:00Z",
            Some(5),
            None,
        );
        let rows = FieldStateRows::from_entries([&before, &block]);
        assert_eq!(
            rows.at(t("2026-04-01T09:07:00Z"), None),
            Some(json!("state2"))
        );
        assert_eq!(
            rows.at(t("2026-04-01T09:10:00Z"), None),
            Some(json!("state1"))
        );
        let changes: Vec<_> = rows
            .changes()
            .into_iter()
            .map(|c| (c.at, c.state, c.row.id.clone()))
            .collect();
        assert_eq!(
            changes,
            vec![
                (
                    t("2026-04-01T09:00:00Z"),
                    json!("state1"),
                    before.hid.to_string()
                ),
                (
                    t("2026-04-01T09:05:00Z"),
                    json!("state2"),
                    block.hid.to_string()
                ),
                (
                    t("2026-04-01T09:10:00Z"),
                    json!("state1"),
                    block.hid.to_string()
                ),
            ],
            "the block's end is a change, and it is the block's"
        );
    }

    /// A compound switch past its trailing edge said its last word there,
    /// so a switch logged inside its span does not outrank it — however
    /// many rows the reader happened to fetch.
    #[test]
    fn a_switch_inside_a_finished_block_switch_does_not_outrank_its_following_state() {
        let outer = row(
            "body",
            Verb::BlockSwitch,
            Some(json!("sit")),
            "2026-04-01T10:00:00Z",
            Some(60),
            Some(json!("vertical")),
        );
        let inner = row(
            "body",
            Verb::Switch,
            Some(json!("stand")),
            "2026-04-01T09:30:00Z",
            None,
            None,
        );
        let rows = FieldStateRows::from_entries([&outer, &inner]);
        assert_eq!(
            rows.at(t("2026-04-01T09:45:00Z"), None),
            Some(json!("stand"))
        );
        let at = t("2026-04-01T10:30:00Z");
        assert_eq!(rows.at(at, None), Some(json!("vertical")));
        let both = [StateRow::from_entry(&inner), StateRow::from_entry(&outer)];
        assert_eq!(
            resolve_effective(&both, at).state,
            json!("vertical"),
            "every window agrees"
        );
    }

    /// da-gvcen: a first log is a point-switch with a MOMENT dur that
    /// declares its field in `following_state`; the declaration holds from
    /// that instant on, as it did when the render compiled its own events.
    #[test]
    fn a_moment_point_switch_hands_over_to_its_following_state() {
        let mut e = row(
            "coffee",
            Verb::PointSwitch,
            None,
            "2026-04-01T09:00:00Z",
            None,
            Some(json!("TRACKED")),
        );
        e.dur = Some(Dur::moment());
        let rows = FieldStateRows::from_entries([&e]);
        assert_eq!(rows.at(t("2026-04-01T08:59:00Z"), None), None);
        assert_eq!(
            rows.at(t("2026-04-01T09:00:00Z"), None),
            Some(json!("TRACKED"))
        );
    }

    /// da-22750: a moment (dur MOMENT, or zero) names a state for an
    /// instant and switches nothing — the state before it holds on, from
    /// where it began, and the field changes only at the next switch.
    #[test]
    fn a_moment_state_leaves_the_state_before_it() {
        let sw = |s: &str, at: &str| row("mood", Verb::Switch, Some(json!(s)), at, None, None);
        let moment = |s: &str, at: &str, dur: &str| {
            let mut e = row("mood", Verb::Point, Some(json!(s)), at, None, None);
            e.dur = Some(Dur::parse(dur).unwrap());
            e
        };
        let happy = sw("happy", "2025-01-08T09:00:00Z");
        let startled = moment("startled", "2025-01-08T11:00:00Z", "MOMENT");
        let startled2 = moment("startled2", "2025-01-08T12:00:00Z", "PT0S");
        let tired = sw("tired", "2025-01-08T15:00:00Z");
        let rows = FieldStateRows::from_entries([&happy, &startled, &startled2, &tired]);
        for at in ["11:00", "11:30", "12:00", "14:59"] {
            let at = t(&format!("2025-01-08T{at}:00Z"));
            assert_eq!(rows.at(at, None), Some(json!("happy")), "at {at}");
        }
        let changes: Vec<_> = rows
            .changes()
            .iter()
            .map(|c| (c.at, c.state.clone()))
            .collect();
        assert_eq!(
            changes,
            [
                (t("2025-01-08T09:00:00Z"), json!("happy")),
                (t("2025-01-08T15:00:00Z"), json!("tired")),
            ]
        );
        // `now`'s read: the newest row is the moment, the one it resumes
        // began at 09:00.
        let newest_first = [startled2.clone(), startled, happy].map(|e| StateRow::from_entry(&e));
        let eff = resolve_effective(&newest_first, t("2025-01-08T13:00:00Z"));
        assert_eq!(eff.state, json!("happy"));
        assert_eq!(eff.since, Some(t("2025-01-08T09:00:00Z")));
    }

    /// vividn, 2026-09-29: an occur leaves a state alone, unless it is
    /// IGNORED or there is none — then the field is TRACKED from the dot.
    #[test]
    fn an_occur_moves_only_an_ignored_or_absent_state() {
        let dot = |at: &str| row("f", Verb::Point, None, at, None, None);
        let sw = |s: &str, at: &str| row("f", Verb::Switch, Some(json!(s)), at, None, None);
        let (d1, d2) = (dot("2026-04-01T09:10:00Z"), dot("2026-04-01T09:20:00Z"));

        let on = sw("drinking", "2026-04-01T09:00:00Z");
        let rows = FieldStateRows::from_entries([&on, &d1, &d2]);
        assert_eq!(
            rows.at(t("2026-04-01T09:10:00Z"), None),
            Some(json!("drinking"))
        );
        assert_eq!(
            rows.at(t("2026-04-01T09:30:00Z"), None),
            Some(json!("drinking"))
        );

        let ignored = sw("IGNORED", "2026-04-01T09:00:00Z");
        let rows = FieldStateRows::from_entries([&ignored, &d1, &d2]);
        assert_eq!(
            rows.at(t("2026-04-01T09:05:00Z"), None),
            Some(json!("IGNORED"))
        );
        assert_eq!(
            rows.at(t("2026-04-01T09:10:00Z"), None),
            Some(json!("TRACKED"))
        );
        assert_eq!(
            rows.at(t("2026-04-01T09:10:00Z"), Some(d1.hid.as_str())),
            Some(json!("IGNORED")),
            "excluding a row reads the field as though it were never written"
        );
        let changes: Vec<_> = rows
            .changes()
            .into_iter()
            .map(|c| (c.at, c.state, c.row.id.clone()))
            .collect();
        assert_eq!(
            changes[1],
            (
                t("2026-04-01T09:10:00Z"),
                json!("TRACKED"),
                d1.hid.to_string()
            ),
            "TRACKED from the FIRST dot after the IGNORED, and the change is that dot's"
        );

        let rows = FieldStateRows::from_entries([&d1, &d2]);
        assert_eq!(rows.at(t("2026-04-01T09:05:00Z"), None), None);
        assert_eq!(
            rows.at(t("2026-04-01T09:15:00Z"), None),
            Some(json!("TRACKED")),
            "no state at all"
        );
    }

    /// da-u65pa: `changes` reads the oracle once per edge, and a field of
    /// plain switches has no block for a read to look ahead to, so what
    /// the reads walk grows with the rows and not with their square.
    #[test]
    fn changes_over_blockless_switches_walk_rows_in_proportion() {
        let walked = |n: usize| {
            let base = t("2026-04-01T00:00:00Z");
            let entries: Vec<Entry> = (0..n)
                .map(|i| {
                    let at = (base + chrono::Duration::minutes(i as i64)).to_rfc3339();
                    let state = json!(if i % 2 == 0 { "on" } else { "off" });
                    row("f", Verb::Switch, Some(state), &at, None, None)
                })
                .collect();
            let rows = FieldStateRows::from_entries(&entries);
            WALKED.set(0);
            assert_eq!(rows.changes().len(), n);
            WALKED.get()
        };
        let (small, large) = (walked(2_000), walked(8_000));
        assert!(
            large <= 4 * small,
            "4x the rows walked {large} against {small}"
        );
    }

    /// A read that excludes a row reads as though it were never written,
    /// so an excluded block takes none of the [`SPAN_LOOKAHEAD`] and the
    /// block one further out, which spans the probe, is still reached
    /// (da-47v7z).
    #[test]
    fn an_excluded_block_takes_no_place_in_the_look_ahead() {
        let block = |s: &str, end: &str, mins: i64| {
            row("f", Verb::Block, Some(json!(s)), end, Some(mins), None)
        };
        let mut blocks: Vec<Entry> = (1..=SPAN_LOOKAHEAD)
            .map(|i| block("short", &format!("2026-04-01T10:{:02}:00Z", 5 * i), 2))
            .collect();
        blocks.push(block("long", "2026-04-01T11:00:00Z", 90));
        let rows = FieldStateRows::from_entries(&blocks);
        let probe = t("2026-04-01T10:00:00Z");
        assert_eq!(
            rows.at(probe, None),
            None,
            "the ninth block is out of reach"
        );
        assert_eq!(
            rows.at(probe, Some(blocks[0].hid.as_str())),
            Some(json!("long"))
        );
    }
}
