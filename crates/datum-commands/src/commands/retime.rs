//! `datum retime [<id_or_slug>] [time]` — move an entry's occur_time.
//!
//! da-rorg: retime aligns with the update/merge/delete/undo defaults —
//! a bare slug resolves to `_LAST`, and a missing time-args group means
//! "set to now". Time targeting is done through three layered paths:
//!
//! * `--offset <ISO>` — explicit duration shift (legacy).
//! * Time-args group (`-t`/`-y`/`-q`/`-d`/...), the positional being one
//!   more `-t` value (`retime <id> 1040`, `retime <id> -5`) — resolved
//!   against each entry's OWN occur_time, in its own zone (da-jix52).
//! * With `-r/--relative` the same args are a pure shift of the entry's
//!   occur_time: `-y` moves it a calendar day and keeps its clock.

use anyhow::{Context, Result};
use datum_core::{
    db::{Db, RetimeFilter},
    entry::Entry,
    ids::{resolve_quickid_single, Slug},
    time::{parse_date, parse_time, DatumTime, TimeCtx},
};

use crate::args::RetimeArgs;
use crate::commands::helpers::{registry_of, resolve_batch, BatchMode};
use crate::commands::log_core::{build_time_anchored, which_reading};
use crate::commands::time_bounds::{parse_after, parse_before};
use crate::render::{show_retime, OutputArgs};

pub fn retime(mut args: RetimeArgs, db: &Db) -> Result<()> {
    // da-rorg: slug defaults to `_LAST` (matching update/merge/delete/
    // undo) so a bare `datum retime` resolves the user's last touch.
    // Bulk filters (--field/--after/--before) stay as explicit
    // overrides that bypass the `_LAST` shorthand entirely.
    let want_bulk_filter = args.field.is_some() || args.after.is_some() || args.before.is_some();

    // Resolve the target entries. The default path (no --field/--after/
    // --before) routes through the comma-aware batch resolver so
    // `retime a,b,c` shifts every named entry — the same canonical path
    // del/update/get already share (da-obes). The bulk-filter path keeps
    // single-slug resolution: a comma list combined with --field/--after/
    // --before is out of scope.
    let resolved_entries: Vec<Entry> = if want_bulk_filter {
        args.slug
            .as_deref()
            .map(|raw| resolve_quickid_single(db, raw))
            .transpose()
            .map_err(anyhow::Error::from)?
            .into_iter()
            .collect()
    } else {
        let pattern = args
            .slug
            .as_deref()
            .unwrap_or(datum_core::ids::resolve::LAST_TOKEN);
        let (entries, token_errors) = resolve_batch(db, pattern, BatchMode::Live, false);
        // Any token that fails to resolve fails the whole command, the
        // same all-or-nothing contract get uses: a bad piece surfaces as
        // an error rather than being silently dropped, so retime never
        // mutates a partial set. Propagating the first token error
        // verbatim also keeps a single-token NotFound / AmbiguousPrefix
        // reading exactly as it did before da-obes.
        if let Some((_, err)) = token_errors.into_iter().next() {
            // da-496ai: a lone unsigned number is an id since da-q1k4p,
            // so `retime 30` meant as the old shift lands here. With a
            // time arg (the positional among them) or a shift beside it,
            // the person was naming an entry, not asking for a shift.
            let hint = matches!(
                err.downcast_ref::<datum_core::error::DatumError>(),
                Some(datum_core::error::DatumError::NotFound(_))
            ) && args.time == Default::default()
                && args.offset.is_none()
                && !args.relative;
            if let Some(n) = pattern.parse::<i64>().ok().filter(|n| hint && *n >= 0) {
                anyhow::bail!("{err}\nhint: to shift _LAST by {n} minutes, write `retime +{n}`");
            }
            return Err(err);
        }
        if entries.is_empty() {
            anyhow::bail!("no entries specified");
        }
        entries
    };

    // da-uzxj integration: when the user defaulted to `_LAST` (no
    // explicit slug, no bulk filter), prompt before mutating if the
    // pointer is past the 15-minute protection window. Explicit slugs
    // and bulk filters skip the prompt entirely — the user named what
    // they wanted. retime has no `--force` flag today, so the prompt
    // is the only off-switch for the stale-default path.
    if !want_bulk_filter
        && args.slug.is_none()
        && !resolved_entries.is_empty()
        && !crate::commands::helpers::confirm_default_last_if_stale(
            db,
            /*explicit_id=*/ None,
            /*force=*/ false,
            "Retime",
            &resolved_entries,
        )?
    {
        println!("Aborted.");
        return Ok(());
    }

    let after_utc = args
        .after
        .as_deref()
        .map(parse_after)
        .transpose()
        .context("Invalid --after value")?;
    let before_utc = args
        .before
        .as_deref()
        .map(parse_before)
        .transpose()
        .context("Invalid --before value")?;

    // One selection filter per resolved entry on the default path; a
    // single filter (optional slug + field/time bounds) on the bulk path.
    // The shift then applies to every filter, so a comma list retimes all
    // named entries by the same amount.
    let slugs: Vec<Slug> = resolved_entries.into_iter().map(|e| e.slug).collect();
    let filters: Vec<RetimeFilter> = if want_bulk_filter {
        vec![RetimeFilter {
            slug_or_hid: slugs.first().map(Slug::as_str),
            field: args.field.as_deref(),
            after_utc,
            before_utc,
        }]
    } else {
        slugs
            .iter()
            .map(|s| RetimeFilter {
                slug_or_hid: Some(s.as_str()),
                ..RetimeFilter::default()
            })
            .collect()
    };

    // Path 1 — explicit ISO duration via --offset.
    if let Some(offset_str) = args.offset.as_deref() {
        let offset = datum_core::time::parse_duration(offset_str)
            .with_context(|| format!("Invalid --offset value '{offset_str}'"))?;
        return shift(db, &filters, offset);
    }

    // Path 2a — `-r/--relative` reads the time-args as a pure shift of
    // the entry's own time (-t -N → -N min, -y → the day before, -q → -5
    // min), which path 2b then builds per entry with the clock kept: a day
    // is a calendar day in the entry's zone, not 24 h (da-471ai). Absolute
    // clock/date forms (-t HH:MM, -d YYYY-MM-DD) are not meaningful as
    // shifts; reject them with a directive error rather than guessing.
    if args.relative {
        refuse_absolute(&args.time)?;
    }

    // Path 2c — `-T` takes the time away: the entry becomes a note
    // (da-1d0v2). The parser refuses it beside any other time arg.
    if args.time.omit_timestamp {
        let mut moved = Vec::new();
        for filter in &filters {
            moved.extend(db.clear_occur_time(filter)?);
        }
        return report(db, &moved);
    }

    // Path 2b — a target built from the time-args, each entry's own time
    // standing where "now" does for a log (da-jix52): `1040` is 10:40 on
    // the entry's day, `-5` five minutes before it. With no time-args there
    // is nothing to read from the entry, and the target is now; under `-r`
    // no time-args is no shift, and the entry stays where it is.
    let now = DatumTime::now().utc;
    let placed = args.relative || args.time != Default::default();
    // `+5`, `-y -t +5`, `-z` alone, anything under `-r`: no clock time is
    // stated, so how well the entry's time is known stays as it was.
    let shifted = args.relative
        || placed && (args.time.time.as_deref()).is_none_or(|t| t.starts_with(['+', '-']));
    // da-z3fzv: a wall time a fall-back night repeats is asked about as a
    // log's is (da-6i7l1), before the write, of the first entry the args
    // make it of; the answer is every entry's.
    if placed {
        let read = |own: &DatumTime, later| {
            let time = datum_parse::input::TimeArgs {
                later,
                ..args.time.clone()
            };
            build_time_anchored(&time, own.utc, Some(own), args.relative).ok()
        };
        let mut owns = Vec::new();
        for filter in &filters {
            owns.extend(
                db.retime_targets(filter)?
                    .into_iter()
                    .filter_map(|e| e.occur_time),
            );
        }
        let asked = owns.iter().find(|own| {
            matches!((read(own, false), read(own, true)), (Some(a), Some(b)) if a.utc != b.utc)
        });
        if let Some(own) = asked {
            match which_reading(|later| read(own, later))? {
                Some(later) => args.time.later = later,
                None => {
                    println!("Aborted.");
                    return Ok(());
                }
            }
        }
    }
    let mut moved = Vec::new();
    for filter in &filters {
        moved.extend(
            db.set_start_time(filter, args.time.timezone.is_some(), |own| {
                let (at, anchor) = if placed {
                    (own.utc, Some(own))
                } else {
                    (now, None)
                };
                let mut t = build_time_anchored(&args.time, at, anchor, args.relative)
                    .map_err(|e| datum_core::error::DatumError::Other(format!("{e:#}")))?;
                if shifted && t.uncertainty.is_none() {
                    t.uncertainty.clone_from(&own.uncertainty);
                }
                Ok(t)
            })?,
        );
    }
    report(db, &moved)
}

/// Apply a pure duration shift across every filter and report it.
fn shift(db: &Db, filters: &[RetimeFilter], offset: chrono::Duration) -> Result<()> {
    let mut moved = Vec::new();
    for filter in filters {
        moved.extend(db.update_times(filter, offset)?);
    }
    report(db, &moved)
}

/// da-bgq2: one RETIME line per moved row — the same one-action-line-per-
/// entry shape `del` renders — rather than a count and a raw second-offset.
/// A retime that matched nothing is an error, not a silent success: the
/// only ways to get here empty are a bulk filter that selected no rows and
/// a target that is timeless (never a retime target).
fn report(db: &Db, moved: &[datum_core::db::Retimed]) -> Result<()> {
    if moved.is_empty() {
        anyhow::bail!("no entries retimed");
    }
    crate::cmd::stamp_batch(db);
    let registry = registry_of(db, moved.iter().map(|r| &r.entry));
    let output = OutputArgs::default();
    for r in moved {
        show_retime(&r.entry, &r.before, registry.as_ref(), &output);
    }
    Ok(())
}

/// `-r/--relative` shifts the entry's own time, so an absolute clock or
/// date form has no meaning under it and is refused up front: the user
/// gets a directive error instead of a surprise placement.
fn refuse_absolute(time: &datum_parse::input::TimeArgs) -> Result<()> {
    // A day `-t` names (`-t sunday`) is a date as `-d`'s is.
    let mut dates = Vec::new();
    for t in time.time_values() {
        let ta = parse_time(t, TimeCtx::now())?;
        if ta.absolute.is_some() || ta.iso_instant.is_some() || ta.iso_naive.is_some() {
            anyhow::bail!(
                "-r/--relative does not support absolute clock-time forms (-t HH:MM); \
                 use a numeric -t -N or stacked -q for relative shifts."
            );
        }
        dates.extend(ta.date);
    }
    if let Some(d) = &time.date {
        dates.push(parse_date(d, TimeCtx::now())?);
    }
    if dates.iter().any(|d| d.absolute.is_some()) {
        anyhow::bail!(
            "-r/--relative does not support absolute date forms (-d YYYY-MM-DD); \
             use a numeric -d ±N or stacked -y for relative day shifts."
        );
    }
    Ok(())
}
