//! `datum retime [<id_or_slug>] [shift]` — move an entry's occur_time.
//!
//! da-rorg: retime aligns with the update/merge/delete/undo defaults —
//! a bare slug resolves to `_LAST`, and a missing time-args group means
//! "set to now". Time targeting is done through three layered paths:
//!
//! * `--offset <ISO>` — explicit duration shift (legacy).
//! * Bare numeric positional (`retime <id> -5`) — minutes-offset
//!   shorthand for the relative path; auto-relative since retime's
//!   only meaningful trailing positional is a time.
//! * Time-args group (`-t`/`-y`/`-q`/`-d`/...) — absolute target by
//!   default, anchored at "now"; with `-r/--relative` the same args
//!   compute a delta from "now" that is applied to the entry's
//!   current occur_time instead.

use anyhow::{Context, Result};
use datum_core::time::Zone;
use datum_core::{
    db::{Db, RetimeFilter},
    entry::Entry,
    ids::{resolve_quickid_single, Slug},
    time::{parse_date, parse_time, DatumTime, TimeBuilder, TimeCtx},
};

use crate::args::RetimeArgs;
use crate::commands::helpers::{resolve_batch, BatchMode};

pub fn retime(args: RetimeArgs, db: &Db) -> Result<()> {
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
        .map(parse_time_bound)
        .transpose()
        .context("Invalid --after value")?;
    let before_utc = args
        .before
        .as_deref()
        .map(parse_time_bound)
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

    // Path 1 — bare numeric positional: minutes-offset shorthand for
    // `-r -t <N>`. Applied as a pure duration shift; we don't need to
    // re-anchor a TimeBuilder for the minutes-only case.
    if let Some(mins) = args.bare_offset_min {
        let offset = chrono::Duration::minutes(mins);
        let mut n = 0;
        for filter in &filters {
            n += db.update_times(filter, offset)?;
        }
        report_shift(n, offset);
        return Ok(());
    }

    // Path 2 — explicit ISO duration via --offset.
    if let Some(offset_str) = args.offset.as_deref() {
        let offset = datum_core::time::parse_duration(offset_str)
            .with_context(|| format!("Invalid --offset value '{offset_str}'"))?;
        let mut n = 0;
        for filter in &filters {
            n += db.update_times(filter, offset)?;
        }
        report_shift(n, offset);
        return Ok(());
    }

    // Path 3a — `-r/--relative` resolves the time-args as a pure
    // duration delta (-t -N → -N min, -y → -1 day, -q → -5 min) and
    // applies it as a shift on the entry's current occur_time. Bare
    // numeric positionals (Path 1) take the same semantics without
    // needing the flag. Absolute clock/date forms (-t HH:MM,
    // -d YYYY-MM-DD) are not meaningful as deltas; reject them with
    // a directive error rather than guessing.
    if args.relative {
        let delta = relative_delta(&args.time)?;
        let mut n = 0;
        for filter in &filters {
            n += db.update_times(filter, delta)?;
        }
        report_shift(n, delta);
        return Ok(());
    }

    // Path 3b — absolute target. No time-args still computes a target
    // (TimeBuilder::now() with nothing applied = exact "now"), which
    // is the default "set to now" path the bead asks for.
    let new_time = build_absolute(&args)?;
    let mut n = 0;
    for filter in &filters {
        n += db.set_start_time(filter, &new_time)?;
    }
    // da-r8xj: the entry was just set to a time the user expressed in
    // local terms — echo it back through the shared local formatter, not
    // raw UTC.
    println!(
        "✓ Retimed {n} → {}{}",
        crate::render::action::format_timestamp(
            &new_time,
            &crate::render::action::now_local(),
            None
        ),
        // The uncertainty travels on the time now, so echo it only when the
        // user actually stated one — an absent value is "unknown", not "0".
        match new_time.uncertainty.as_deref() {
            Some(u) => format!(" (±{u})"),
            None => String::new(),
        },
    );
    Ok(())
}

/// Compute the duration delta encoded by a `TimeArgs` group when used
/// in relative mode. Numeric `-t -N` becomes `N` minutes, stacked `-q`
/// becomes `quick * 5` minutes back, stacked `-y` becomes `yesterday`
/// days back, and numeric `-d ±N` becomes `±N` days. Absolute clock
/// or date forms have no meaning as pure deltas and are rejected up-
/// front so the user gets a directive error instead of a surprise
/// shift derived from "now".
fn relative_delta(time: &datum_parse::input::TimeArgs) -> Result<chrono::Duration> {
    let mut delta = chrono::Duration::zero();
    if time.yesterday > 0 {
        delta -= chrono::Duration::days(time.yesterday as i64);
    }
    if time.quick > 0 {
        delta -= chrono::Duration::minutes(time.quick as i64 * 5);
    }
    if let Some(t) = &time.time {
        let ta = parse_time(t, TimeCtx::now())?;
        if let Some(off) = ta.offset {
            delta += off;
        } else if ta.absolute.is_some() || ta.iso_instant.is_some() || ta.iso_naive.is_some() {
            anyhow::bail!(
                "-r/--relative does not support absolute clock-time forms (-t HH:MM); \
                 use a numeric -t -N or stacked -q for relative shifts."
            );
        }
    }
    if let Some(d) = &time.date {
        let da = parse_date(d, TimeCtx::now())?;
        if da.absolute.is_some() {
            anyhow::bail!(
                "-r/--relative does not support absolute date forms (-d YYYY-MM-DD); \
                 use a numeric -d ±N or stacked -y for relative day shifts."
            );
        }
        delta += chrono::Duration::days(da.days_offset);
    }
    Ok(delta)
}

/// Build the absolute target time from `args.time`, with its uncertainty
/// already on it. `args.time` with no flags set yields "now" with no stated
/// uncertainty, which is the default-to-now path.
fn build_absolute(args: &RetimeArgs) -> Result<DatumTime> {
    let mut builder = TimeBuilder::now()
        .with_yesterday_count(args.time.yesterday as i64)
        .with_quick_count(args.time.quick as i64)
        .with_quick_uncertainty(args.time.quick)
        .with_full_day(args.time.full_day);
    if let Some(d) = &args.time.date {
        builder = builder.with_date_arg(parse_date(d, TimeCtx::now())?);
    }
    if let Some(t) = &args.time.time {
        builder = builder.with_time_arg(parse_time(t, TimeCtx::now())?);
    }
    if let Some(tz) = &args.time.timezone {
        let resolved = datum_core::time::resolve_timezone(tz)
            .map_err(|e| anyhow::anyhow!("Invalid -z/--timezone value '{tz}': {e}"))?;
        builder = builder.with_timezone(Some(resolved));
    }
    Ok(builder.build_with_uncertainty())
}

fn report_shift(n: usize, offset: chrono::Duration) {
    let secs = offset.num_seconds();
    let sign = if secs >= 0 { "+" } else { "-" };
    println!("✓ Retimed {n} ({sign}{}s)", secs.abs());
}

/// Parse a `--after` / `--before` argument to a UTC timestamp.
/// Accepts `YYYY-MM-DD` (midnight local) or RFC3339 timestamps.
fn parse_time_bound(s: &str) -> Result<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let naive = d
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("bad date: {s}"))?;
        // A selection bound, not a logged time: `for_boundary` always yields
        // an instant, so `--after 2024-11-03` no longer refuses on the two
        // days a year local midnight is ambiguous (da-ztae).
        return Ok(Zone::host().resolve(naive).for_boundary());
    }
    anyhow::bail!("expected YYYY-MM-DD or RFC3339, got '{s}'")
}
