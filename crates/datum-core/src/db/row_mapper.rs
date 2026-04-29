//! `entries`-row → [`Entry`] adapters.
//!
//! [`row_to_entry_public`] is the only `pub` surface here: external crates
//! (`datum-sync-client`, `views/state_change`, `ids/resolve`, `conflicts`)
//! all build their own `SELECT * FROM entries` queries and need a way to
//! materialize a row without going through the typed `Db` API.

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

    let shape_str: String = row.get("entry_shape")?;
    let entry_shape: EntryShape = shape_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;

    let value: Option<JsonValue> = row
        .get::<_, Option<String>>("value")?
        .and_then(|s| serde_json::from_str(&s).ok());

    let data: Option<JsonValue> = row
        .get::<_, Option<String>>("data")?
        .and_then(|s| serde_json::from_str(&s).ok());

    let start_time = build_datum_time(
        row.get("start_time")?,
        row.get("start_time_offset")?,
        row.get("start_time_tz")?,
    );
    let end_time = build_datum_time(
        row.get("end_time")?,
        row.get("end_time_offset")?,
        row.get("end_time_tz")?,
    );

    let start_time_precision: u32 = row.get::<_, i64>("start_time_precision")? as u32;
    let start_precision_unit_str: String = row.get("start_time_precision_unit")?;
    let start_time_precision_unit: PrecisionUnit = start_precision_unit_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;
    let end_time_precision: u32 = row.get::<_, i64>("end_time_precision")? as u32;
    let end_precision_unit_str: String = row.get("end_time_precision_unit")?;
    let end_time_precision_unit: PrecisionUnit = end_precision_unit_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;

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

    Ok(Entry {
        hid: row.get("hid")?,
        slug: row.get("slug")?,
        content_hash: row.get("content_hash")?,
        field: row.get("field")?,
        entry_shape,
        intent: row.get("intent")?,
        verb,
        value,
        data,
        unit: row.get("unit")?,
        start_time,
        end_time,
        start_time_precision,
        start_time_precision_unit,
        end_time_precision,
        end_time_precision_unit,
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
