//! The `timezone` field and the entries around it (da-mvt5i): which zone a log
//! is in, and what a device in another zone does about it.

use anyhow::Result;
use chrono::{DateTime, Utc};
use datum_core::{
    action::Action,
    db::{is_meta_field, Db},
    timezone_field::{same_zone, state_zone, TIMEZONE_FIELD},
};
use datum_parse::input::TimeArgs;
use serde_json::Value as JsonValue;
use std::io::IsTerminal;

use crate::args::LogArgs;
use crate::commands::log_core::{
    build_time_from_flags, compound_field, extract_reserved_columns, log_entry, parse_log_args,
    parse_reserved_occur_time, resolve_underscore_state, whole_day, written_field, zone_is_settled,
};
use crate::commands::question::{is_yes, page, Question};

/// Whether a line can be a `timezone` entry's, so every other log parses once
/// more only if it holds a word starting `field`: `field=timezone`, `-k field`.
pub(crate) fn may_name_timezone(args: &LogArgs) -> bool {
    args.field_or_verb == TIMEZONE_FIELD
        || std::iter::once(&args.field_or_verb)
            .chain(&args.rest)
            .chain(&args.keys)
            .chain(&args.field_keys)
            .any(|word| word.starts_with("field"))
}

/// `Some` where a line is a `timezone` entry, with the zone it will name, if it
/// names one: the state after its trailing edge where it has one, `_` read as
/// the log reads it, at now and so with no need of the instant this zone will
/// place.
pub(crate) fn timezone_line(args: &LogArgs, action: Action, db: &Db) -> Option<Option<String>> {
    let mut parsed = parse_log_args(args, action, None, None).ok()?;
    let mut reserved = extract_reserved_columns(&mut parsed.extra_data).ok()?;
    let field = compound_field(
        reserved.field.take().unwrap_or(parsed.field),
        &args.field_keys,
        &parsed.field_values,
    )
    .ok()?;
    let field = written_field(db, &field, args.setup).ok()?.unwrap_or(field);
    if field != TIMEZONE_FIELD {
        return None;
    }
    let back = |state: Option<JsonValue>| match action {
        Action::Switch => resolve_underscore_state(state, db, &field).ok()?,
        _ => state,
    };
    Some(
        [
            reserved.following_state,
            back(reserved.state),
            back(parsed.value),
        ]
        .iter()
        .flatten()
        .find_map(state_zone),
    )
}

/// The zone the `timezone` field holds at the instant the line names, where
/// no `-z` or offset says (da-mvt5i). Which zone applies depends on the
/// instant the zone gives, so each stretch reads the line in its own zone
/// ([`Declared::reading_itself`]): `-yyt 8` is 8:00 in Lisbon when the trip
/// begins at 8:00 Lisbon time, though 8:00 here was before it. Where no stretch
/// reads it inside itself, a clock skipped by a move east, it is the zone
/// held at the line read in the host's, keeping the clock typed, which `check
/// tz` then names (`tz_off_declared`).
pub(crate) fn declared_zone(
    db: &Db,
    args: &LogArgs,
    action: Action,
    now: DateTime<Utc>,
) -> Result<Option<String>> {
    if zone_is_settled(&args.time) {
        return Ok(None);
    }
    let declared = db.declared_zones()?;
    let zone = declared
        .reading_itself(|zone| read_in(args, action, Some(zone), now))
        .or_else(|| declared.at(read_in(args, action, None, now)?));
    Ok(zone.map(str::to_string))
}

/// The instant the line names, read in `zone`, or the host's: the flags', or
/// an `occur_time=` key's, one more `-t` on them (da-87wvg), read as the log
/// reads it: from the flags' time and not the midnight `-D` makes of it, then
/// made the key's whole day.
fn read_in(
    args: &LogArgs,
    action: Action,
    zone: Option<&str>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let time = TimeArgs {
        timezone: zone.map(str::to_string),
        ..args.time.clone()
    };
    let clock = TimeArgs {
        full_day: false,
        ..time.clone()
    };
    let base = build_time_from_flags(&clock, now).ok()?;
    let key = parse_log_args(args, action, Some(base.clone()), None)
        .ok()
        .and_then(|parsed| parsed.extra_data?.get("occur_time").cloned());
    let at = match key {
        Some(key) => parse_reserved_occur_time(&key, &base).ok()?,
        None => return Some(build_time_from_flags(&time, now).ok()?.utc),
    };
    Some(
        if time.full_day {
            whole_day(&at).ok()?
        } else {
            at
        }
        .utc,
    )
}

/// How far from now a line's time may be and still be the place this device
/// is in now, for the question below.
const ABOUT_NOW: chrono::TimeDelta = chrono::TimeDelta::minutes(15);

/// The zone of the `timezone` switch this log is moved by, already logged, or
/// `None`, the entry keeping `stored`. Only at about now, read in either zone:
/// a person in Tokyo typing `-t 1513` means Tokyo's. A multi line's sides write
/// together, so none would see the switch another side asked for: it asks at
/// the next log. The caller keeps the field's own line from here.
pub(crate) fn following(
    db: &Db,
    args: &LogArgs,
    action: Action,
    stored: &str,
    now: DateTime<Utc>,
) -> Result<Option<String>> {
    if is_meta_field(&args.field_or_verb) || crate::args::window_now().is_some() {
        return Ok(None);
    }
    let Some(device) = device_zone() else {
        return Ok(None);
    };
    let in_device = read_in(args, action, Some(&device), now);
    let about_now = [read_in(args, action, Some(stored), now), in_device]
        .into_iter()
        .flatten()
        .any(|at| (at - now).abs() <= ABOUT_NOW);
    if !about_now || !follows(db, stored, &device)? {
        return Ok(None);
    }
    let at = in_device.map_or(now, |at| at.min(now));
    log_zone_switch(db, &device, at, &args.output)?;
    Ok(Some(device))
}

/// This device's zone, as `-z` would name it.
fn device_zone() -> Option<String> {
    datum_core::time::clock::host_iana_tz_name()
}

/// Whether the field, holding `stored`, moves to `device`: by `timezone_follow`,
/// `auto` without a question and `ask` by asking its person, and only with a
/// person there, so a script on a server in UTC neither moves the field nor
/// waits on stdin. By name, a tzdb spelling being the zone (`same_zone`). A no
/// is kept for this pair until the two agree or another pair comes; no answer
/// keeps nothing, and the question comes again. Asked before a transaction, as
/// any question is.
fn follows(db: &Db, stored: &str, device: &str) -> Result<bool> {
    if same_zone(stored, device) {
        // A read before the write, so a log with nothing to forget writes nothing.
        if db.zone_declined_any()? {
            db.zone_agreed()?;
        }
        return Ok(false);
    }
    if !(std::io::stdin().is_terminal() || page()) {
        return Ok(false);
    }
    if db.setting("timezone_follow")?.is_some_and(|v| v == "auto") {
        return Ok(true);
    }
    if db.zone_declined(stored, device)? {
        return Ok(false);
    }
    let question = Question::yes_no(
        format!("This device is in {device}, and the timezone field holds {stored}. Switch it to {device}?"),
        &format!("switch to {device}"),
        Some(false),
    );
    let yes = is_yes(&question.ask()?);
    if !yes {
        db.decline_zone(stored, device)?;
    }
    Ok(yes)
}

/// Log the `timezone` switch a device's zone asked for, with a line of its own
/// as `raw_input`, what `reparse` and `why` read, the log that asked keeping
/// its line. Its echo says it happened, as any log's does.
fn log_zone_switch(
    db: &Db,
    zone: &str,
    at: DateTime<Utc>,
    output: &datum_parse::input::OutputArgs,
) -> Result<()> {
    let at = at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let line = vec![
        TIMEZONE_FIELD.to_string(),
        zone.to_string(),
        "-t".to_string(),
        at.clone(),
    ];
    let held = crate::args::take_intent_context();
    crate::args::set_intent_context(crate::args::IntentContext {
        raw_input: datum_parse::alias::shell_join(&line),
        raw_argv: line,
        alias_chain: Vec::new(),
    });
    let logged = log_entry(
        Action::Switch,
        LogArgs {
            field_or_verb: TIMEZONE_FIELD.to_string(),
            rest: vec![zone.to_string()],
            time: TimeArgs {
                time: Some(at),
                ..TimeArgs::default()
            },
            output: output.clone(),
            ..LogArgs::default()
        },
        db,
    );
    crate::args::take_intent_context();
    if let Some(held) = held {
        crate::args::set_intent_context(held);
    }
    logged
}

/// `datum follow-zone`: datum.day's load, the question of a log at now, with
/// no log (da-mvt5i). A person there, the page's env, is the caller's to be.
pub fn follow_zone(db: &Db) -> Result<()> {
    let now = Utc::now();
    let (Some(device), Some(stored)) = (
        device_zone(),
        db.declared_zones()?.at(now).map(str::to_string),
    ) else {
        return Ok(());
    };
    if follows(db, &stored, &device)? {
        log_zone_switch(db, &device, now, &Default::default())?;
    }
    Ok(())
}
