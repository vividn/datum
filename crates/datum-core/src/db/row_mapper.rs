//! `entries`-row → [`Entry`] adapters.
//!
//! [`row_to_entry_public`] is the only `pub` surface here: external crates
//! (`datum-sync-client`, `views/state_change`, `ids/resolve`, `conflicts`)
//! all build their own `SELECT * FROM entries` queries and need a way to
//! materialize a row without going through the typed `Db` API.
//!
//! da-5ns: rebuilt to read the new (occur_time, state, dur,
//! following_state, verb) shape. Sub-bead 1 stub — sub-bead 3 wires
//! verb-derivation into the insert side.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

use crate::entry::{Entry, EntryShape, PrecisionUnit, Verb};
use crate::time::DatumTime;

/// Row → `Entry` adapter for callers that build their own queries over the
/// `entries` table. Used by `datum-sync-client::ops` for pack/apply, which
/// reads and writes full entry rows outside the normal `Db` API surface.
pub fn row_to_entry_public(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    row_to_entry(row)
}

pub(super) fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    let verb_str: String = row.get("verb")?;
    let verb: Verb = verb_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;

    let state: Option<JsonValue> = row
        .get::<_, Option<String>>("state")?
        .and_then(|s| serde_json::from_str(&s).ok());

    let dur: Option<String> = row.get("dur")?;
    let following_state: Option<JsonValue> = row
        .get::<_, Option<String>>("following_state")?
        .and_then(|s| serde_json::from_str(&s).ok());

    let data: Option<JsonValue> = row
        .get::<_, Option<String>>("data")?
        .and_then(|s| serde_json::from_str(&s).ok());

    let occur_time = build_datum_time(
        row.get("occur_time")?,
        row.get("occur_time_offset")?,
        row.get("occur_time_tz")?,
    );
    let occur_time_precision: Option<String> = row.get("occur_time_precision")?;

    let create_time: DateTime<Utc> = row
        .get::<_, String>("create_time")?
        .parse()
        .map_err(|e: chrono::ParseError| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
        })?;
    let modify_time: DateTime<Utc> = row
        .get::<_, String>("modify_time")?
        .parse()
        .map_err(|e: chrono::ParseError| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
        })?;
    let deleted_at: Option<DateTime<Utc>> = row
        .get::<_, Option<String>>("deleted_at")?
        .and_then(|s| s.parse().ok());

    let alias_chain: Option<JsonValue> = row
        .get::<_, Option<String>>("alias_chain")?
        .and_then(|s| serde_json::from_str(&s).ok());
    let parser_version: u32 = row.get::<_, i64>("parser_version")? as u32;
    let user_asserted_last_state: Option<JsonValue> = row
        .get::<_, Option<String>>("user_asserted_last_state")?
        .and_then(|s| serde_json::from_str(&s).ok());
    let extra_metadata: Option<JsonValue> = row
        .get::<_, Option<String>>("extra_metadata")?
        .and_then(|s| serde_json::from_str(&s).ok());

    // da-5ns sub-bead 1 compat shims: see entry.rs for rationale.
    // Populate the v=19 fields from the v=20 column data so legacy
    // call sites still see meaningful values during the transition.
    let entry_shape = match dur.as_deref() {
        None => EntryShape::Point,
        Some(d) if matches!(d, "PT0S" | "P0D" | "PT0M" | "PT0H") => EntryShape::Point,
        Some(_) => EntryShape::Block,
    };
    let end_time_compat: Option<DatumTime> = match (occur_time.as_ref(), dur.as_deref()) {
        (Some(t), Some(d)) => crate::entry::compat_dur_secs(d).map(|secs| {
            let mut e = t.clone();
            e.utc += chrono::Duration::seconds(secs);
            e
        }),
        _ => None,
    };

    Ok(Entry {
        hid: row.get("hid")?,
        slug: row.get("slug")?,
        content_hash: row.get("content_hash")?,
        field: row.get("field")?,
        verb,
        state: state.clone(),
        dur,
        following_state,
        data,
        unit: row.get("unit")?,
        occur_time: occur_time.clone(),
        occur_time_precision,
        lat: row.get("lat")?,
        lon: row.get("lon")?,
        location: row.get("location")?,
        create_time,
        modify_time,
        device_id: row.get("device_id")?,
        deleted_at,
        id_structure: row.get("id_structure")?,
        field_structure: row.get("field_structure")?,
        raw_input: row.get("raw_input")?,
        alias_chain,
        parser_version,
        user_asserted_last_state,
        extra_metadata,
        // Compat shims:
        start_time: occur_time,
        end_time: end_time_compat,
        start_time_precision: 0,
        start_time_precision_unit: PrecisionUnit::Exact,
        end_time_precision: 0,
        end_time_precision_unit: PrecisionUnit::Exact,
        entry_shape,
        intent: None,
        value: state,
    })
}

pub(super) fn build_datum_time(
    utc_str: Option<String>,
    offset: Option<i32>,
    tz: Option<String>,
) -> Option<DatumTime> {
    let utc_str = utc_str?;
    let utc: DateTime<Utc> = utc_str.parse().ok()?;
    Some(DatumTime {
        utc,
        offset_minutes: offset.unwrap_or(0),
        timezone: tz.unwrap_or_else(|| "UTC".to_string()),
    })
}
