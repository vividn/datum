//! The `timezone` field (da-yw6o2): reserved by name in every database, with
//! no kit and no install. An ordinary switch, seen in the tail and the
//! pictures, whose states are IANA zones or `IGNORED`. vividn, 2026-10-30:
//! «a special switch field called timezone, where you can put in the IANA
//! timezone … enforce that it is an IANA timezone, and sets it's timezone».

use serde_json::Value as JsonValue;

use crate::db::IGNORED_STATE_VALUE;

pub const TIMEZONE_FIELD: &str = "timezone";

/// What the word `timezone` runs before the field has a spec to derive it.
pub const TIMEZONE_ALIAS: &str = "switch timezone";

/// The canonical name of the IANA zone `state` names, if it names one.
pub fn state_zone(state: &JsonValue) -> Option<String> {
    datum_parse::time::iana_zone(state.as_str()?).ok()
}

/// A state the field may hold: a zone, or `IGNORED`.
pub fn holds(state: &JsonValue) -> bool {
    state == IGNORED_STATE_VALUE || state_zone(state).is_some()
}
