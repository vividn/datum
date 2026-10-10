//! `datum ignore <field>` — mark a field as ignored (hidden from `now` etc).

use anyhow::Result;
use datum_core::{
    db::{Db, IGNORED_STATE_VALUE},
    entry::{Entry, Verb},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use crate::args::IgnoreArgs;
use crate::config::client_id;

pub fn ignore(args: IgnoreArgs, db: &Db) -> Result<()> {
    // da-2sr: writing a switch entry with the IGNORED sentinel is the only
    // way to mark a field ignored. No spec writes — the spec keeps its
    // schema-only role.
    let client = client_id()?;
    // da-t3quu: a name datum has never seen is most often a typo, and its
    // IGNORED row is hidden from `fields`. Ignoring a field ahead of its
    // first entry is a use, so the row is written and the note says so.
    if db.entries_for_field(&args.field, Some(1))?.is_empty()
        && db.load_spec(&args.field)?.is_none()
    {
        eprintln!(
            "note: no field '{}' yet (no entries, no spec); ignoring it anyway",
            args.field
        );
    }
    let entry = Entry::new_legacy(
        &args.field,
        Verb::Switch,
        Some(JsonValue::String(IGNORED_STATE_VALUE.to_string())),
        None,
        Some(DatumTime::now()),
        None,
        client,
    );
    db.insert_entry(&entry)?;
    println!("✓ Ignoring '{}'", args.field);
    Ok(())
}
