//! `datum flag` — manual review-later marker on a field at a time
//! point (da-dnnn, Option A).
//!
//! Stored as a `Verb::Point` entry on `field` with `at` taken
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
    time::{DefaultDirection, TimeCtx},
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

    // Resolve at from the standard timing flags so back-dated
    // markers land where the user pointed. Default is "now" — a bare
    // `datum flag <field>` stamps the moment of the command, same as
    // every other logging verb, in the zone `-z` names (da-rsa54). A day
    // alone (`-y`) keeps now's clock: a flag marks a moment, never a
    // whole day.
    let now = crate::args::window_now().unwrap_or_else(datum_core::time::DatumTime::now);
    let own_time = time.places();
    let at = time
        .builder(TimeCtx::now().facing(DefaultDirection::NearestDate))?
        .with_timezone(zone_name(&time)?)
        .build_now(now.utc);

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
        Some(at.clone()),
        Some(Dur::moment()),
        client_id()?,
    );
    entry.extra_metadata = Some(serde_json::json!({ "flag": true }));

    // Past a `--` a comma is in the name, which nothing could name after.
    datum_core::slug::validate_new_field_name(&field)?;
    crate::commands::helpers::note_unseen_field(db, &field, "flagging")?;
    // In a line of several the line's commit writes it, with the rest, and
    // says so (`cmd.rs`).
    if crate::args::push_to_multi_collector(entry.clone(), Default::default(), own_time) {
        return Ok(());
    }
    db.insert_entry(&entry)
        .with_context(|| format!("recording flag on field '{field}'"))?;
    print_flag(&entry);
    Ok(())
}

/// Whether `entry` is a `datum flag`'s.
pub(crate) fn is_flag(entry: &Entry) -> bool {
    entry
        .extra_metadata
        .as_ref()
        .is_some_and(|m| m.get("flag") == Some(&JsonValue::Bool(true)))
}

/// What `datum flag` says of the entry it wrote.
pub(crate) fn print_flag(entry: &Entry) {
    let comment = entry.state.as_ref().and_then(JsonValue::as_str);
    let preview = match comment {
        None => "(no comment)".to_string(),
        Some(c) if c.chars().count() > 60 => format!("{}…", c.chars().take(60).collect::<String>()),
        Some(c) => c.to_string(),
    };
    println!("FLAG {} {} — {preview}", entry.hid, entry.slug);
}
