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
/// current state and equals `last_event` here — the most recent
/// state-bearing entry both established the false state and is the last
/// event. They are exposed as distinct keys so future versions can
/// distinguish the two without a schema break.
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
        let now = Utc::now();
        let tracked = self.get_tracked_fields()?;

        let mut active = Vec::new();
        let mut tracked_false = Vec::new();
        let mut ignored = Vec::new();

        for tf in tracked {
            if tf.ignored {
                if !filter.include_ignored {
                    continue;
                }
                let entry = self.latest_entry_for_now(&tf.field)?;
                let since = entry.as_ref().and_then(entry_event_time);
                ignored.push(NowFieldIgnored {
                    field: tf.field,
                    since,
                    color: tf.color,
                });
                continue;
            }

            let entry = self.latest_entry_for_now(&tf.field)?;
            match entry {
                None => {
                    if filter.include_tracked_false {
                        tracked_false.push(NowFieldTrackedFalse {
                            field: tf.field,
                            since: None,
                            last_event: None,
                            color: tf.color,
                            entry: None,
                        });
                    }
                }
                Some(e) => {
                    let state = e.value.clone().unwrap_or(JsonValue::Null);
                    let since = entry_event_time(&e);
                    if is_truthy(&state) {
                        active.push(NowFieldActive {
                            field: tf.field,
                            state,
                            since,
                            verb: e.verb.to_string(),
                            color: tf.color,
                            entry: e,
                        });
                    } else if filter.include_tracked_false {
                        tracked_false.push(NowFieldTrackedFalse {
                            field: tf.field,
                            since,
                            last_event: since,
                            color: tf.color,
                            entry: Some(e),
                        });
                    }
                }
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
}

fn entry_event_time(e: &Entry) -> Option<DateTime<Utc>> {
    e.start_time.as_ref().or(e.end_time.as_ref()).map(|t| t.utc)
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
