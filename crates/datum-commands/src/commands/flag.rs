//! `datum flag` — manual review-later marker on a field at a time
//! point (da-dnnn, Option A).
//!
//! Stored as a `Verb::Point` entry on `field` with `occur_time` taken
//! from the standard timing flags (so back-dated flags land where the
//! user said), its words in `state` and `extra_metadata = { flag: true }`.
//! `datum meta` marks its rows the same way, with `meta: true` for
//! `flag: true` — renderers that filter out meta-rows can extend the
//! check with the same one-liner. A flag's words are in `state` alone
//! (da-tgulk): a second copy in `extra_metadata`, which no edit reaches,
//! kept the old words after `edit` or `update state=…`.
//!
//! `datum check`-integration (surfacing pending flags during integrity
//! sweeps) is a follow-up — filed as a separate bead. The storage
//! shape here is forward-compatible.

use anyhow::{Context, Result};
use datum_core::{
    db::Db,
    dur::Dur,
    entry::{Entry, Verb},
    time::TimeCtx,
};
use serde_json::Value as JsonValue;

use crate::commands::log_core::zone_name;
use crate::config::client_id;
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
    // every other logging verb, in the zone `-z` names (da-rsa54). A day
    // alone (`-y`) keeps now's clock: a flag marks a moment, never a
    // whole day.
    let occur_time = time
        .builder(TimeCtx::now())?
        .with_timezone(zone_name(&time)?)
        .build();

    // State = comment, its one copy: `edit` and `update` reach it, and a
    // quick `datum get <hid>` shows it. Empty bare-form flags carry no
    // state — they're pure "look here" markers.
    let state = if comment.is_empty() {
        None
    } else {
        Some(JsonValue::String(comment.clone()))
    };

    // `Dur::moment()` locks the row to Verb::Point regardless of state
    // shape so the verb classifier doesn't promote it. Matches the
    // pattern `datum meta` uses.
    let mut entry = Entry::new(
        &field,
        Verb::Point,
        state,
        None,
        Some(occur_time.clone()),
        Some(Dur::moment()),
        client_id()?,
    );
    entry.extra_metadata = Some(serde_json::json!({ "flag": true }));

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
