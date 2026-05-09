//! `datum retime <id_or_slug>` — move an entry's time_start/time_end.

use anyhow::{Context, Result};
use chrono::{Local, Utc};
use datum_core::{
    db::{Db, RetimeFilter},
    ids::resolve_quickid_single,
    time::{parse_duration, parse_time_arg, TimeBuilder},
};

use crate::args::RetimeArgs;

pub fn retime(args: RetimeArgs, db: &Db) -> Result<()> {
    if args.slug.is_none() && args.field.is_none() && args.after.is_none() && args.before.is_none()
    {
        anyhow::bail!(
            "retime requires at least one selector: <slug>, --field, --after, or --before"
        );
    }

    // Accept the full quickId surface (`_`, `<field>:_`, hid/slug prefix,
    // slug wildcard) where retime previously required an exact slug/hid.
    // The underlying filter still matches on exact slug/hid, so we resolve
    // the user-supplied pattern to a concrete slug first.
    let resolved_slug: Option<datum_core::ids::Slug> = args
        .slug
        .as_deref()
        .map(|raw| resolve_quickid_single(db, raw).map(|e| e.slug))
        .transpose()
        .map_err(anyhow::Error::from)?;

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

    let filter = RetimeFilter {
        slug_or_hid: resolved_slug.as_deref(),
        field: args.field.as_deref(),
        after_utc,
        before_utc,
    };

    // --time: re-parse a fresh absolute time and overwrite both the start
    // timestamp and its precision fields for each match. end_time (if any)
    // shifts by the same delta so the duration is preserved.
    if let Some(time_str) = args.time.as_deref() {
        let ta = parse_time_arg(time_str)
            .with_context(|| format!("Invalid --time value '{time_str}'"))?;
        let builder = TimeBuilder::now().with_time_arg(ta);
        let (new_time, precision) = builder.build_with_precision();
        let n = db.set_start_time_with_precision(
            &filter,
            &new_time,
            precision.seconds,
            &precision.unit.to_string(),
        )?;
        println!(
            "✓ Retimed {n} → {} (±{}{})",
            new_time.utc.to_rfc3339(),
            precision.seconds,
            if precision.seconds == 0 { "" } else { "s" },
        );
        return Ok(());
    }

    // --offset: classic duration-shift path.
    let offset_str = args
        .offset
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("retime requires either --offset or --time"))?;
    let offset = parse_duration(offset_str)
        .with_context(|| format!("Invalid --offset value '{offset_str}'"))?;

    let n = db.update_times(&filter, offset)?;
    let secs = offset.num_seconds();
    let sign = if secs >= 0 { "+" } else { "-" };
    println!("✓ Retimed {n} ({sign}{}s)", secs.abs());
    Ok(())
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
        let local = naive
            .and_local_timezone(Local)
            .single()
            .ok_or_else(|| anyhow::anyhow!("ambiguous local time for {s}"))?;
        return Ok(local.with_timezone(&Utc));
    }
    anyhow::bail!("expected YYYY-MM-DD or RFC3339, got '{s}'")
}
