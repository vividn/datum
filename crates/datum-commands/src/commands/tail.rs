//! `datum tail` — list the most recent entries.

use anyhow::Result;
use chrono::{Local, TimeZone, Utc};
use datum_core::{
    db::{Db, ListParams, TimeMetric},
    entry::Entry,
    time::{parse_date_arg, parse_time_arg, DatumTime, TimeBuilder},
};

use crate::args::{OutputFlagsExt, TailArgs};
use crate::cmd::{build_render_contexts, load_registry_opt};
use crate::render::{print_table_with, TableArgs, TimeMetricCol};

pub fn tail(args: TailArgs, db: &Db) -> Result<()> {
    let columns = split_columns(&args.columns);
    let metric: TimeMetricCol = args
        .metric
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    let entries = tail_entries(&args, db)?;
    let registry = load_registry_opt(db);
    let output = args.output();

    // Per da-6t0: tail/head always render through the tabular path so the
    // header row, column alignment, and `hid` column are on by default. The
    // legacy info-line branch is gone; `--table` is now a no-op flag kept
    // only for backward CLI compatibility.
    let tbl = TableArgs {
        output: output.clone(),
        metric,
        columns,
        suppress_header: false,
    };
    let now = DatumTime::now();
    let ctxs = build_render_contexts(&entries, db);
    print_table_with(&entries, Some(&ctxs), Some(&now), registry.as_ref(), &tbl);

    if let Some(secs) = args.watch {
        let interval = watch_interval(secs);
        let mut since = entries
            .iter()
            .map(|e| e.create_time)
            .max()
            .unwrap_or_else(chrono::Utc::now);
        let field = args.field.as_deref();
        let tick_tbl = TableArgs {
            suppress_header: true,
            ..tbl.clone()
        };
        loop {
            std::thread::sleep(interval);
            let new_rows = db.list_entries_created_after(field, since)?;
            if let Some(latest) = new_rows.iter().map(|e| e.create_time).max() {
                since = latest;
            }
            if !new_rows.is_empty() {
                let now = DatumTime::now();
                let tick_ctxs = build_render_contexts(&new_rows, db);
                print_table_with(
                    &new_rows,
                    Some(&tick_ctxs),
                    Some(&now),
                    registry.as_ref(),
                    &tick_tbl,
                );
            }
        }
    }

    Ok(())
}

/// Clamp a user-supplied `--watch` interval into a sane poll cadence.
/// Non-finite, zero, or negative values collapse to 1 s; anything positive
/// is honoured at millisecond granularity (so `--watch 0.25` → 250 ms).
fn watch_interval(secs: f64) -> std::time::Duration {
    if !secs.is_finite() || secs <= 0.0 {
        return std::time::Duration::from_secs(1);
    }
    let millis = (secs * 1000.0).round() as u64;
    std::time::Duration::from_millis(millis.max(1))
}

/// Run the tail query as the verb does, but return the entries instead of
/// printing. Used by integration tests and by `head` when it routes through
/// the shared tail path.
pub fn tail_entries(args: &TailArgs, db: &Db) -> Result<Vec<Entry>> {
    let params = tail_params(args)?;
    Ok(db.list_entries(&params)?)
}

/// Split `--column` args (comma-splittable, repeatable) into a flat list.
pub fn split_columns(raw: &[String]) -> Vec<String> {
    raw.iter()
        .flat_map(|s| s.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Translate [`TailArgs`] into [`ListParams`] for [`Db::list_entries`].
///
/// Window semantics (matching datum):
/// * No time args → unbounded (future entries included).
/// * `-d`/`-y` with no `-t`/`-q` → full local-day window centred on the
///   computed date, with a ±1-day pad to absorb timezone offsets; no default
///   limit (unless `-n` is set).
/// * `-t` (alone or with `-d`/`-y`) → upper-bound only (classic tail).
pub fn tail_params(args: &TailArgs) -> Result<ListParams> {
    let metric: TimeMetric = args
        .metric
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let has_date = args.time.date.is_some() || args.time.yesterday > 0;
    let has_time = args.time.time.is_some() || args.time.quick > 0;
    let is_date_only = has_date && !has_time;

    let (start_utc, end_utc) = if !has_date && !has_time {
        (None, None)
    } else {
        let window = build_window(args)?;
        if is_date_only {
            // Pad by ±1 day so timezone skew doesn't drop border rows; caller
            // can further refine if needed, but for tail the pad is harmless
            // since `-n` still bounds the result.
            (Some(window.day_start_utc), Some(window.day_end_utc))
        } else {
            (None, Some(window.point_utc))
        }
    };

    // Default limit: 10 for "tail", unlimited when date-only (matches datum).
    let limit = args.count.or(if is_date_only { None } else { Some(10) });

    Ok(ListParams {
        field: args.field.clone(),
        metric,
        start_utc,
        end_utc,
        limit,
        head: args.oldest_first,
        reverse: args.reverse,
    })
}

struct TailWindow {
    point_utc: chrono::DateTime<Utc>,
    day_start_utc: chrono::DateTime<Utc>,
    day_end_utc: chrono::DateTime<Utc>,
}

fn build_window(args: &TailArgs) -> Result<TailWindow> {
    let mut builder = TimeBuilder::now()
        .with_yesterday_count(args.time.yesterday as i64)
        .with_quick_count(args.time.quick as i64);
    if let Some(d) = &args.time.date {
        builder = builder.with_date_arg(parse_date_arg(d)?);
    }
    if let Some(t) = &args.time.time {
        builder = builder.with_time_arg(parse_time_arg(t)?);
    }
    let point = builder.build();
    let local = point.utc.with_timezone(&Local);
    let day = local.date_naive();
    let day_start = Local
        .from_local_datetime(
            &(day - chrono::Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap(),
        )
        .single()
        .unwrap();
    let day_end = Local
        .from_local_datetime(
            &(day + chrono::Duration::days(1))
                .and_hms_opt(23, 59, 59)
                .unwrap(),
        )
        .single()
        .unwrap();
    Ok(TailWindow {
        point_utc: point.utc,
        day_start_utc: day_start.with_timezone(&Utc),
        day_end_utc: day_end.with_timezone(&Utc),
    })
}
