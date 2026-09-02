//! `datum flag` — manual review-later marker on a field at a time
//! point (da-dnnn, Option A).
//!
//! Stored as a `Verb::Point` entry on `field` with `occur_time` taken
//! from the standard timing flags (so back-dated flags land where the
//! user said) and `extra_metadata = { flag: true, comment: "…" }`.
//! This is the same shape `datum meta` uses, swapping `meta: true` for
//! `flag: true` — renderers that filter out meta-rows can extend the
//! check with the same one-liner.
//!
//! `datum check`-integration (surfacing pending flags during integrity
//! sweeps) is a follow-up — filed as a separate bead. The storage
//! shape here is forward-compatible.

use anyhow::{Context, Result};
use datum_core::{
    db::Db,
    dur::Dur,
    entry::{Entry, Verb},
    time::{parse_date_arg, parse_time_arg, TimeBuilder, TimeCtx},
};
use serde_json::Value as JsonValue;

use crate::config::device_id;
use datum_parse::cli::FlagArgs;

pub fn flag(args: FlagArgs, db: &Db) -> Result<()> {
    let FlagArgs {
        field,
        comment,
        time,
    } = args;

    // Resolve occur_time from the standard timing flags so back-dated
    // markers land where the user pointed. Default is "now" — a bare
    // `datum flag <field>` stamps the moment of the command, same as
    // every other logging verb.
    let mut builder = TimeBuilder::now()
        .with_yesterday_count(time.yesterday as i64)
        .with_quick_count(time.quick as i64);
    if let Some(d) = time.date.as_deref() {
        builder = builder.with_date_arg(parse_date_arg(d, TimeCtx::now())?);
    }
    if let Some(t) = time.time.as_deref() {
        builder = builder.with_time_arg(parse_time_arg(t, TimeCtx::now())?);
    }
    let occur_time = builder.build();

    // Build the extra_metadata payload. Always emit `flag: true` so
    // downstream filters can match without caring whether a comment
    // exists; the comment is omitted (not empty-string'd) when the
    // bare form is used so renderers don't have to special-case "".
    let mut meta_obj = serde_json::Map::new();
    meta_obj.insert("flag".to_string(), JsonValue::Bool(true));
    if !comment.is_empty() {
        meta_obj.insert("comment".to_string(), JsonValue::String(comment.clone()));
    }

    // Value = comment so a quick `datum get <hid>` shows the payload
    // without diving into extra_metadata. Empty bare-form flags carry
    // no value — they're pure "look here" markers.
    let value = if comment.is_empty() {
        None
    } else {
        Some(JsonValue::String(comment.clone()))
    };

    // `Dur::Moment` locks the row to Verb::Point regardless of state
    // shape so the verb classifier doesn't promote it. Matches the
    // pattern `datum meta` uses.
    let mut entry = Entry::new(
        &field,
        Verb::Point,
        value,
        None,
        Some(occur_time.clone()),
        Some(Dur::Moment),
        device_id()?,
    );
    entry.extra_metadata = Some(JsonValue::Object(meta_obj));

    db.insert_entry(&entry)
        .with_context(|| format!("recording flag on field '{field}'"))?;

    let preview = if comment.is_empty() {
        "(no comment)".to_string()
    } else {
        let max = 60;
        if comment.chars().count() > max {
            let trimmed: String = comment.chars().take(max).collect();
            format!("{trimmed}…")
        } else {
            comment.clone()
        }
    };
    println!("FLAG {} {} — {preview}", entry.hid, entry.slug);
    Ok(())
}
