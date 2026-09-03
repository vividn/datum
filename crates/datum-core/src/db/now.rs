//! `current_now`: queryable snapshot of currently-active / tracked fields.
//!
//! Single source of truth for the `datum now` surface. Both the terminal
//! renderer and the JSON emitter consume a [`NowSnapshot`]; the wire format
//! is versioned via [`SCHEMA_ID`] so downstream consumers (web, mobile,
//! voice) can lock against it.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value as JsonValue;

use crate::entry::Entry;
use crate::error::Result;

use super::types::IGNORED_STATE_VALUE;
use super::Db;

/// Wire-format identifier for the JSON shape of [`NowSnapshot`].
pub const SCHEMA_ID: &str = "datum_now_v1";

/// Concentric scopes a [`NowSnapshot`] can include. Active fields are
/// always present; the flags extend outward.
#[derive(Debug, Clone, Copy, Default)]
pub struct NowFilter {
    /// Include tracked fields whose current state is falsy.
    pub include_tracked_false: bool,
    /// Include fields whose latest state is `IGNORED`.
    pub include_ignored: bool,
    /// Time-travel anchor for the snapshot (da-tx8j). `None` →
    /// wall-clock now (legacy behaviour). `Some(t)` → return the state
    /// each field was in at or before `t`. The returned snapshot's
    /// `now` field carries the anchor so renderers and `--json`
    /// consumers see a single stable timestamp.
    pub snapshot_time: Option<DateTime<Utc>>,
}

/// Active row: the field is currently in a truthy state.
#[derive(Debug, Clone, Serialize)]
pub struct NowFieldActive {
    pub field: String,
    pub state: JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    pub verb: String,
    #[serde(skip)]
    pub color: Option<String>,
    #[serde(skip)]
    pub entry: Entry,
}

/// Tracked-but-currently-false row. `since` is when the field entered its
/// current state; `last_event` is the newest state-bearing entry's own
/// time. They usually coincide — that entry both established the state
/// and is the last event — and da-phoe is the case they split: a block
/// that has handed the field back is the last event, but the state it
/// handed back to began at the block's trailing edge.
#[derive(Debug, Clone, Serialize)]
pub struct NowFieldTrackedFalse {
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub color: Option<String>,
    #[serde(skip)]
    pub entry: Option<Entry>,
}

/// Explicitly-ignored row. `since` is the time of the IGNORED-state entry
/// that pinned the field out of the tracked set.
#[derive(Debug, Clone, Serialize)]
pub struct NowFieldIgnored {
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub color: Option<String>,
}

/// Top-level wire shape. `schema = "datum_now_v1"` lets consumers detect
/// breaking changes; bumps to v2+ accompany any incompatible field rename
/// or semantic change.
#[derive(Debug, Clone, Serialize)]
pub struct NowSnapshot {
    pub schema: &'static str,
    pub now: DateTime<Utc>,
    pub active: Vec<NowFieldActive>,
    pub tracked_false: Vec<NowFieldTrackedFalse>,
    pub ignored: Vec<NowFieldIgnored>,
}

impl Db {
    /// Compute a [`NowSnapshot`] of the currently-tracked fields.
    ///
    /// `active` is always populated; the other two scopes are gated by
    /// `filter`. Within each scope, rows are sorted alphabetically by
    /// field name.
    pub fn current_now(&self, filter: &NowFilter) -> Result<NowSnapshot> {
        // da-tx8j: snapshot_time pins the anchor for both the query
        // (`latest_entry_for_now` filters `<= snapshot_time`) and the
        // wire `now` field. Defaults to wall-clock now when None.
        // da-n0dr: the same anchor scopes the field list, so tracked/
        // ignored classification matches the per-field states at `now`
        // rather than mixing epochs with wall-clock membership.
        let now = filter.snapshot_time.unwrap_or_else(Utc::now);
        let tracked = self.get_tracked_fields(now)?;

        let mut active = Vec::new();
        let mut tracked_false = Vec::new();
        let mut ignored = Vec::new();

        for tf in tracked {
            // da-2i8g: a block-switch / point-switch carries a
            // `following_state` that takes effect at `occur_time + dur`.
            // da-phoe: a plain block instead HANDS THE FIELD BACK there,
            // so the row that decides the state may be behind it.
            // [`resolve_effective`] applies both rules.
            let (mut entries, eff) = self.resolved_now(&tf.field, filter, now)?;

            if tf.ignored {
                if filter.include_ignored {
                    ignored.push(NowFieldIgnored {
                        field: tf.field,
                        since: eff.since,
                        color: tf.color,
                    });
                }
                continue;
            }

            if entries.is_empty() {
                if filter.include_tracked_false {
                    tracked_false.push(NowFieldTrackedFalse {
                        field: tf.field,
                        since: None,
                        last_event: None,
                        color: tf.color,
                        entry: None,
                    });
                }
                continue;
            }

            // `since` is when the CURRENT state began — the resume
            // boundary when a block handed the field back — while
            // `last_event` stays the newest row's own time. That is the
            // split these two keys were declared separately for.
            let last_event = entries.first().and_then(entry_event_time);
            // The governing row established the state: the row a block
            // interrupted, not the block. With nothing behind the block
            // the field holds no state and the newest row is still the
            // one worth showing.
            let e = entries.swap_remove(eff.governing.unwrap_or(0));
            if is_truthy(&eff.state) {
                active.push(NowFieldActive {
                    field: tf.field,
                    state: eff.state,
                    since: eff.since,
                    verb: e.verb.to_string(),
                    color: tf.color,
                    entry: e,
                });
            } else if filter.include_tracked_false {
                tracked_false.push(NowFieldTrackedFalse {
                    field: tf.field,
                    since: eff.since,
                    last_event,
                    color: tf.color,
                    entry: Some(e),
                });
            }
        }

        active.sort_by(|a, b| a.field.cmp(&b.field));
        tracked_false.sort_by(|a, b| a.field.cmp(&b.field));
        ignored.sort_by(|a, b| a.field.cmp(&b.field));

        Ok(NowSnapshot {
            schema: SCHEMA_ID,
            now,
            active,
            tracked_false,
            ignored,
        })
    }

    /// The rows behind a field's state at `now`, newest first, together
    /// with the resolution over them. Reads ONE row — the common case,
    /// and the whole of the pre-da-phoe cost — and looks further back
    /// only when that row is a block that has already handed the field
    /// back, so the state to resume lies behind it.
    fn resolved_now(
        &self,
        field: &str,
        filter: &NowFilter,
        now: DateTime<Utc>,
    ) -> Result<(Vec<Entry>, Effective)> {
        let mut entries = self.entries_for_now(field, filter.snapshot_time, 1)?;
        if entries
            .first()
            .is_some_and(|e| StateRow::from_entry(e).hands_field_back(now))
        {
            entries = self.entries_for_now(field, filter.snapshot_time, RESUME_LOOKBACK)?;
        }
        let rows: Vec<StateRow> = entries.iter().map(StateRow::from_entry).collect();
        let effective = resolve_effective(&rows, now);
        Ok((entries, effective))
    }
}

/// How many rows a state read may walk back through to find the state a
/// block handed the field back to. Each completed block immediately
/// preceding the read consumes one; a real ledger never stacks anywhere
/// near this many back to back, and past the limit the read reports "no
/// state" rather than inventing one.
pub(super) const RESUME_LOOKBACK: usize = 64;

/// One state-bearing row, in as much of it as the state oracles read.
/// The shared input of [`resolve_effective`] so `now` (which holds whole
/// [`Entry`]s) and [`Db::batch_state_at`] (which selects six columns)
/// resolve through the SAME code and cannot drift (da-phoe).
pub(super) struct StateRow {
    pub verb: crate::entry::Verb,
    pub state: Option<JsonValue>,
    pub occur: Option<DateTime<Utc>>,
    pub dur: Option<crate::dur::Dur>,
    pub following_state: Option<JsonValue>,
}

impl StateRow {
    pub(super) fn from_entry(e: &Entry) -> Self {
        StateRow {
            verb: e.verb,
            state: e.state.clone(),
            occur: e.occur_time.as_ref().map(|t| t.utc),
            dur: e.dur.clone(),
            following_state: e.following_state.clone(),
        }
    }

    /// `occur + dur` for a row that spans one, i.e. where its own region
    /// ends. `None` for a point or an unparseable `dur`.
    fn trailing_edge(&self) -> Option<DateTime<Utc>> {
        let occur = self.occur?;
        let secs = self.dur.as_ref()?.as_seconds()?;
        Some(occur + chrono::Duration::seconds(secs))
    }

    /// Does this row hand the field back once its trailing edge is past?
    ///
    /// da-611d: a block is an INTERRUPTION, not a switch. `end work 12`
    /// writes `following_state IS NULL`, and NULL there is a *specified
    /// default* meaning "resume whatever state preceded this block" —
    /// the common case, because the state to come back to is simply the
    /// prior one and does not need restating. So past `occur + dur` this
    /// row is no longer the one that decides the state: the row before
    /// it is. Reading NULL as "the field is now `false` forever" is what
    /// left every minute after vividn's break attributed to nothing.
    ///
    /// [`Verb::BlockSwitch`] / [`Verb::PointSwitch`] carry a real
    /// `following_state` and switch to it instead ([`effective_state`]),
    /// so they never hand back — [`crate::shape::hands_field_back`] owns
    /// that verb split, shared with `stats` (da-mkqo).
    pub(super) fn hands_field_back(&self, at: DateTime<Utc>) -> bool {
        if !crate::shape::hands_field_back(self.verb) || self.following_state.is_some() {
            return false;
        }
        match (self.occur, self.dur.as_ref().and_then(|d| d.as_seconds())) {
            // A zero-span row occupies no time, so it interrupts nothing.
            (Some(occur), Some(secs)) if secs > 0 => at >= occur + chrono::Duration::seconds(secs),
            _ => false,
        }
    }

    /// When this row's effective state at `at` took effect: its trailing
    /// edge once a compound switch has crossed it, otherwise `occur`.
    fn effective_since(&self, at: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if crate::shape::opens_secondary_switch_region(self.verb) && self.following_state.is_some()
        {
            if let Some(trailing) = self.trailing_edge() {
                if at >= trailing {
                    return Some(trailing);
                }
            }
        }
        self.occur
    }
}

/// What a field's rows say it holds at one instant.
pub(super) struct Effective {
    pub state: JsonValue,
    /// When the field entered `state` — the resume boundary when a block
    /// handed it back, otherwise the governing row's own event time.
    pub since: Option<DateTime<Utc>>,
    /// Index into the input rows of the row that established `state`.
    /// `None` when every row handed the field back and nothing preceded
    /// them: the field holds no state at all.
    pub governing: Option<usize>,
}

/// Resolve the state a field holds at `at` from its rows, NEWEST FIRST.
///
/// Walks back past every block that has already handed the field back
/// ([`StateRow::hands_field_back`]) to the row that actually decides the
/// state, then reads that row with [`effective_state`]. One row in is
/// the common case and behaves exactly as before da-phoe.
pub(super) fn resolve_effective(rows: &[StateRow], at: DateTime<Utc>) -> Effective {
    // The NEWEST completed block fixes when the resumed span began;
    // older ones ended before it and are already covered.
    let mut resumed_at: Option<DateTime<Utc>> = None;
    for (i, row) in rows.iter().enumerate() {
        if row.hands_field_back(at) {
            resumed_at = resumed_at.or_else(|| row.trailing_edge());
            continue;
        }
        return Effective {
            state: effective_state(
                row.verb,
                row.state.as_ref(),
                row.occur,
                row.dur.as_ref(),
                row.following_state.as_ref(),
                at,
            ),
            since: resumed_at.max(row.effective_since(at)),
            governing: Some(i),
        };
    }
    // Nothing preceded the block(s): there is no state to resume to.
    // `Null` is "tracked, no current state" — the honest answer, and the
    // one a timeline resolver derives for the same rows.
    Effective {
        state: JsonValue::Null,
        since: resumed_at,
        governing: None,
    }
}

/// Read ONE row's state at `at`, honouring `following_state` for
/// block-switch / point-switch when the moment has crossed the trailing
/// edge (da-2i8g). For all other verbs (and for block-switch rows whose
/// trailing edge is still in the future), the primary `value` column is
/// the state.
fn effective_state(
    verb: crate::entry::Verb,
    state: Option<&JsonValue>,
    occur_utc: Option<DateTime<Utc>>,
    dur: Option<&crate::dur::Dur>,
    following_state: Option<&JsonValue>,
    at: DateTime<Utc>,
) -> JsonValue {
    // `following_state` takes over past the trailing edge for exactly the
    // verbs that open a secondary switch region — shape.rs owns the set.
    if crate::shape::opens_secondary_switch_region(verb) {
        if let (Some(occur), Some(dur), Some(fs)) = (occur_utc, dur, following_state) {
            let delta = dur.as_seconds().unwrap_or(0);
            let trailing = occur + chrono::Duration::seconds(delta);
            if at >= trailing {
                return fs.clone();
            }
        }
    }
    state.cloned().unwrap_or(JsonValue::Null)
}

fn entry_event_time(e: &Entry) -> Option<DateTime<Utc>> {
    e.occur_time.as_ref().map(|t| t.utc)
}

/// Truthiness in the `datum now` sense: not `false`, not `null`, not the
/// IGNORED sentinel. Arrays are truthy iff any element is truthy.
fn is_truthy(v: &JsonValue) -> bool {
    match v {
        JsonValue::Null => false,
        JsonValue::Bool(b) => *b,
        JsonValue::String(s) if s == IGNORED_STATE_VALUE => false,
        JsonValue::Array(items) => items.iter().any(is_truthy),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthy_rules() {
        assert!(!is_truthy(&JsonValue::Null));
        assert!(!is_truthy(&JsonValue::Bool(false)));
        assert!(is_truthy(&JsonValue::Bool(true)));
        assert!(!is_truthy(&JsonValue::String(IGNORED_STATE_VALUE.into())));
        assert!(is_truthy(&JsonValue::String("happy".into())));
        // Empty string is still a state value — truthy.
        assert!(is_truthy(&JsonValue::String(String::new())));
        // Arrays: truthy iff any element is truthy.
        assert!(!is_truthy(&serde_json::json!([])));
        assert!(!is_truthy(&serde_json::json!([false, null])));
        assert!(is_truthy(&serde_json::json!([false, "x"])));
    }

    #[test]
    fn schema_id_is_v1() {
        assert_eq!(SCHEMA_ID, "datum_now_v1");
    }
}
