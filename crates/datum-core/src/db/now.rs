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
use crate::state::{resolve_effective, Effective, StateRow, RESUME_LOOKBACK};

use super::Db;
use crate::shape::is_inactive_state;

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
            // `following_state` that takes effect at `occur_time`.
            // da-phoe: a plain block instead HANDS THE FIELD BACK there,
            // so the row that decides the state may be behind it.
            // [`resolve_effective`] applies both rules.
            let (newest, mut entries, eff) = self.resolved_now(&tf.field, filter, now)?;

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

            let Some(newest) = newest else {
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
            };

            // `since` is when the CURRENT state began — the resume
            // boundary when a block handed the field back — while
            // `last_event` stays the newest row's own time. That is the
            // split these two keys were declared separately for.
            let last_event = entry_event_time(&newest);
            // The governing row established the state: the row a block
            // interrupted, not the block. With nothing behind the block
            // the field holds no state and the newest row is still the
            // one worth showing.
            let e = match eff.governing {
                Some(i) => entries.swap_remove(i),
                None => newest,
            };
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

    /// The field's newest row, the rows behind its state at `now` newest
    /// first, and the resolution over them. Reads ONE row — the common
    /// case, and the whole of the pre-da-phoe cost — and looks further
    /// back, occur dots aside, only when that row is a dot (which never
    /// decides a state) or a block that has already handed the field back.
    /// An answer an occur moves takes the first dot after it too, as
    /// [`Db::batch_governing_at`] does.
    fn resolved_now(
        &self,
        field: &str,
        filter: &NowFilter,
        now: DateTime<Utc>,
    ) -> Result<(Option<Entry>, Vec<Entry>, Effective)> {
        let mut entries = self.entries_for_now(field, filter.snapshot_time, 1, false)?;
        let newest = entries.first().cloned();
        if newest.as_ref().is_some_and(|e| {
            let row = StateRow::from_entry(e);
            row.is_occur_dot() || row.hands_field_back(now)
        }) {
            entries = self.entries_for_now(field, filter.snapshot_time, RESUME_LOOKBACK, true)?;
        }
        let resolve = |entries: &[Entry]| {
            let rows: Vec<StateRow> = entries.iter().map(StateRow::from_entry).collect();
            resolve_effective(&rows, now)
        };
        let mut effective = resolve(&entries);
        if effective.an_occur_tracks() {
            if let Some(dot) = self.first_dot_after(field, effective.since, now)? {
                entries.push(dot);
                effective = resolve(&entries);
            }
        }
        Ok((newest, entries, effective))
    }
}

fn entry_event_time(e: &Entry) -> Option<DateTime<Utc>> {
    e.occur_time.as_ref().map(|t| t.utc)
}

/// Truthiness in the `datum now` sense: not inactive
/// ([`is_inactive_state`]). Arrays are truthy iff any element is truthy.
fn is_truthy(v: &JsonValue) -> bool {
    match v {
        JsonValue::Array(items) => items.iter().any(is_truthy),
        v => !is_inactive_state(Some(v)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::{IGNORED_STATE_VALUE, TRACKED_STATE_VALUE};

    #[test]
    fn truthy_rules() {
        assert!(!is_truthy(&JsonValue::Null));
        assert!(!is_truthy(&JsonValue::Bool(false)));
        assert!(is_truthy(&JsonValue::Bool(true)));
        assert!(!is_truthy(&JsonValue::String(IGNORED_STATE_VALUE.into())));
        assert!(!is_truthy(&JsonValue::String(TRACKED_STATE_VALUE.into())));
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
