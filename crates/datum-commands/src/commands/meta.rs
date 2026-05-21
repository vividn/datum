//! `datum meta` — out-of-band protocol notes about a field's
//! classification (da-xn8j).
//!
//! Each note is stored as a `Verb::Point` entry on the named field,
//! `occur_time = now` (so multiple notes on the same field disambiguate
//! by their wall-clock timestamps via the standard `<field>:<time>`
//! slug — `:unset` would collide on UNIQUE slug after the first note),
//! and `extra_metadata = { "meta": true, "text": "<note>" }`. Plain
//! reads (`tail`, `now`, `dayview`) can filter on `extra_metadata.meta`
//! to hide these; `meta show` queries explicitly for them.

use anyhow::{Context, Result};
use datum_core::{
    db::Db,
    dur::Dur,
    entry::{Entry, Verb},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use crate::config::device_id;
use datum_parse::cli::MetaCommand;

pub fn meta(cmd: MetaCommand, db: &Db) -> Result<()> {
    match cmd {
        MetaCommand::Add { field, text } => add_note(&field, &text, db),
        MetaCommand::Show { field } => show_notes(&field, db),
    }
}

fn add_note(field: &str, text: &str, db: &Db) -> Result<()> {
    if text.trim().is_empty() {
        anyhow::bail!("`datum meta {field}` requires a non-empty note");
    }
    // `occur_time = now` so notes slug to `<field>:<timestamp>` and don't
    // collide with each other on the UNIQUE(slug) constraint. (A truly
    // timeless `:unset` slug would only allow one note per field.) The
    // user-facing temporal meaning still lives in `create_time`, which
    // equals this `occur_time` at insert.
    //
    // `Dur::Moment` locks the row to `Verb::Point` regardless of state
    // shape so the verb classifier doesn't promote it to a switch.
    let mut entry = Entry::new(
        field,
        Verb::Point,
        Some(JsonValue::String(text.to_string())),
        None,
        Some(DatumTime::now()),
        Some(Dur::Moment),
        device_id()?,
    );
    let mut meta_obj = serde_json::Map::new();
    meta_obj.insert("meta".to_string(), JsonValue::Bool(true));
    meta_obj.insert("text".to_string(), JsonValue::String(text.to_string()));
    entry.extra_metadata = Some(JsonValue::Object(meta_obj));

    db.insert_entry(&entry)
        .with_context(|| format!("recording meta note on field '{field}'"))?;
    println!("META {} {}", entry.hid, entry.slug);
    Ok(())
}

fn show_notes(field: &str, db: &Db) -> Result<()> {
    // Pull every row for this field — meta-notes always slug to
    // `<field>:unset` so there's at most one per insert per
    // `create_time` tick. Filter for rows whose `extra_metadata.meta`
    // is true and print them in `create_time` order.
    let mut entries = db
        .entries_for_field(field, None)
        .with_context(|| format!("loading entries for field '{field}'"))?;
    entries.retain(|e| {
        e.extra_metadata
            .as_ref()
            .and_then(|v| v.get("meta"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    });
    if entries.is_empty() {
        println!("(no meta notes for '{field}')");
        return Ok(());
    }
    entries.sort_by_key(|e| e.create_time);
    for e in &entries {
        let text = e
            .extra_metadata
            .as_ref()
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("(no text)");
        let stamp = e.create_time.format("%Y-%m-%d %H:%M");
        println!("{stamp}  {text}");
    }
    Ok(())
}
