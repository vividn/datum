//! `datum ignore <field>` — mark a field as ignored (hidden from `now` etc).

use anyhow::Result;
use datum_core::{
    db::{Db, IGNORED_STATE_VALUE},
    entry::{Entry, Verb},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use crate::args::IgnoreArgs;
use crate::config::device_id;

pub fn ignore(args: IgnoreArgs, db: &Db) -> Result<()> {
    // da-2sr: writing a switch entry with the IGNORED sentinel is the only
    // way to mark a field ignored. No spec writes — the spec keeps its
    // schema-only role.
    let device = device_id()?;
    let entry = Entry::new_legacy(
        &args.field,
        Verb::Switch,
        Some(JsonValue::String(IGNORED_STATE_VALUE.to_string())),
        None,
        Some(DatumTime::now()),
        None,
        device,
    );
    db.insert_entry(&entry)?;
    println!("✓ Ignoring '{}'", args.field);
    Ok(())
}
