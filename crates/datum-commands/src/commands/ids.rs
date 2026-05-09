//! `datum ids` — list entry ids (or slugs) in time order.

use anyhow::{Context, Result};
use chrono::{Local, TimeZone, Utc};
use datum_core::{db::Db, time::parse_duration};

use crate::args::IdsArgs;

pub fn ids(args: IdsArgs, db: &Db) -> Result<()> {
    let ids = compute_ids(&args, db)?;
    if args.json {
        let arr: Vec<serde_json::Value> =
            ids.iter().map(|s| serde_json::Value::String(s.clone())).collect();
        println!("{}", serde_json::Value::Array(arr));
    } else {
        for id in ids {
            println!("{id}");
        }
    }
    Ok(())
}

pub fn compute_ids(args: &IdsArgs, db: &Db) -> Result<Vec<String>> {
    let since_utc = args.since.as_deref().map(parse_since).transpose()?;
    Ok(db.list_entry_ids(
        args.field.as_deref(),
        since_utc,
        args.limit,
        args.oldest_first,
        args.slug,
    )?)
}

/// Parse `--since` into a UTC lower bound.
///
/// Grammar (tried in order):
///   1. RFC3339 datetime, e.g. `2024-03-16T00:00:00Z`
///   2. `YYYY-MM-DD` — interpreted as local midnight
///   3. Duration offset (`1d`, `30m`, `2h`, `PT5M`) — treated as "N ago"
pub(crate) fn parse_since(s: &str) -> Result<chrono::DateTime<Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let local_midnight = Local
            .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .context("ambiguous local midnight")?;
        return Ok(local_midnight.with_timezone(&Utc));
    }
    let dur = parse_duration(s)
        .map_err(|e| anyhow::anyhow!("invalid --since '{s}': {e}"))?;
    Ok(chrono::Utc::now() - dur)
}
