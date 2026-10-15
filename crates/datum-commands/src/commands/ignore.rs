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
    // da-t3quu: a typo's IGNORED row is hidden from `fields`.
    crate::commands::helpers::note_unseen_field(db, &args.field, "ignoring")?;
    db.insert_entry(&ignored_entry(&args.field, client))?;
    println!("✓ Ignoring '{}'", args.field);
    Ok(())
}

/// The row that ignores `field` from now on.
pub(crate) fn ignored_entry(field: &str, client: String) -> Entry {
    Entry::new_legacy(
        field,
        Verb::Switch,
        Some(JsonValue::String(IGNORED_STATE_VALUE.to_string())),
        None,
        Some(DatumTime::now()),
        None,
        client,
    )
}
