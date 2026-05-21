//! `datum tail` — list the most recent entries.

use anyhow::Result;
use chrono::{Local, TimeZone, Utc};
use datum_core::{
    db::{Db, ListParams, TimeMetric},
    entry::Entry,
    time::{parse_date_arg, parse_time_arg, DatumTime, TimeBuilder},
};

use crate::args::{OutputFlagsExt, TailArgs};
use crate::commands::helpers::load_registry_opt;
use crate::commands::render_helpers::build_render_contexts;
use crate::render::term_layout::{detect_cols, detect_rows, is_stdout_tty};
use crate::render::{print_table_with, TableArgs, TimeMetricCol};

pub fn tail(args: TailArgs, db: &Db) -> Result<()> {
    // `field:state` form (da-plbb): block-mode rendering replaces the
    // per-entry table entirely. The state-block path doesn't share any
    // of the column/metric/watch plumbing below — bail early. The parser
    // guarantees `state_filter` is only set alongside exactly one field.
    if let Some(state) = args.state_filter.as_deref() {
        if let Some(field) = args.fields.first() {
            return crate::commands::tail_state::tail_state(
                field,
                state,
                args.count,
                args.oldest_first,
                db,
            );
        }
    }
    let columns = split_columns(&args.columns);
    let metric: TimeMetricCol = args
        .metric
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    // da-5025: when stdout is a real terminal AND the user didn't pin
    // `-n`, expand the query to fill the visible window so the default
    // tail actually uses the screen. Piped / redirected output stays on
    // the legacy `Some(10)` so scripts get the stable behaviour they
    // expect.
    let tty = is_stdout_tty();
    let max_total_width = if tty { Some(detect_cols()) } else { None };
    // -1 reserves a row for the header. Below 5 stays unhelpful; clamp.
    let auto_count = if tty && args.count.is_none() {
        Some(detect_rows().saturating_sub(1).max(1))
    } else {
        None
    };
    let entries = tail_entries_with_default_count(&args, auto_count, db)?;
    let registry = load_registry_opt(db);
    let output = args.output()?;

    // Per da-6t0: tail/head always render through the tabular path so the
    // header row, column alignment, and `hid` column are on by default. The
    // legacy info-line branch is gone; `--table` is now a no-op flag kept
    // only for backward CLI compatibility.
    let tbl = TableArgs {
        output: output.clone(),
        metric,
        columns,
        suppress_header: false,
        max_total_width,
    };
    let now = DatumTime::now();
    let ctxs = build_render_contexts(&entries, db);
    print_table_with(&entries, Some(&ctxs), Some(&now), registry.as_ref(), &tbl);

    if let Some(secs) = args.watch {
        let interval = watch_interval(secs);
        // da-xorl: on a real terminal, do a full re-fetch + clear-and-
        // repaint per tick so (a) column widths recompute against the
        // current window's contents and (b) back-dated entries slot
        // into their actual chronological position via `occur_time DESC`,
        // not appended at the bottom. Piped / non-tty output keeps the
        // legacy append-only behaviour so scripts that consume the
        // running stream don't break — clearing the screen makes no
        // sense in a pipe.
        if tty {
            watch_repaint_loop(&args, db, auto_count, &tbl, registry.as_ref(), interval)?;
        } else {
            watch_append_loop(&args, db, &tbl, registry.as_ref(), &entries, interval)?;
        }
    }

    Ok(())
}

/// da-xorl: terminal `--watch` loop — every tick, re-fetch the entire
/// window through the same path the initial render used, clear the
/// screen, and repaint. Recomputes column widths against the fresh
/// row set and naturally absorbs back-dated entries (the underlying
/// `list_entries` orders by `occur_time DESC`, so a `-t`-backfilled
/// switch slots into its chronological position on the next tick).
fn watch_repaint_loop(
    args: &TailArgs,
    db: &Db,
    auto_count: Option<usize>,
    base_tbl: &TableArgs,
    registry: Option<&datum_core::field_spec::SpecRegistry>,
    interval: std::time::Duration,
) -> Result<()> {
    use std::io::Write;
    loop {
        std::thread::sleep(interval);
        // ANSI clear-and-home so the new render starts at the top.
        // The pair "\x1b[2J\x1b[H" is the canonical clear+home — same
        // sequence less invasive than entering the alt-screen, so the
        // user's scrollback isn't blown away when they Ctrl-C.
        print!("\x1b[2J\x1b[H");
        let _ = std::io::stdout().flush();
        let entries = tail_entries_with_default_count(args, auto_count, db)?;
        let now = DatumTime::now();
        let ctxs = build_render_contexts(&entries, db);
        print_table_with(&entries, Some(&ctxs), Some(&now), registry, base_tbl);
    }
}

/// Legacy `--watch` loop for piped / non-tty output: append-only,
/// only emits rows that landed since the last tick. Kept verbatim
/// so scripts consuming the streaming output don't see the screen-
/// clearing escapes the TTY path now emits.
fn watch_append_loop(
    args: &TailArgs,
    db: &Db,
    base_tbl: &TableArgs,
    registry: Option<&datum_core::field_spec::SpecRegistry>,
    seed_entries: &[Entry],
    interval: std::time::Duration,
) -> Result<()> {
    let mut since = seed_entries
        .iter()
        .map(|e| e.create_time)
        .max()
        .unwrap_or_else(chrono::Utc::now);
    let field_refs: Vec<&str> = args.fields.iter().map(|s| s.as_str()).collect();
    let tick_tbl = TableArgs {
        suppress_header: true,
        ..base_tbl.clone()
    };
    loop {
        std::thread::sleep(interval);
        let new_rows = db.list_entries_created_after(&field_refs, since)?;
        if let Some(latest) = new_rows.iter().map(|e| e.create_time).max() {
            since = latest;
        }
        if !new_rows.is_empty() {
            let now = DatumTime::now();
            let tick_ctxs = build_render_contexts(&new_rows, db);
            print_table_with(&new_rows, Some(&tick_ctxs), Some(&now), registry, &tick_tbl);
        }
    }
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
    tail_entries_with_default_count(args, None, db)
}

/// Variant of [`tail_entries`] that lets the caller override the hard-coded
/// `Some(10)` default when `args.count` is `None`. Used by the live tail
/// path to swap in a screen-filling row count from `$LINES` (da-5025);
/// existing callers preserve the legacy default by passing `None`.
pub fn tail_entries_with_default_count(
    args: &TailArgs,
    default_count: Option<usize>,
    db: &Db,
) -> Result<Vec<Entry>> {
    let mut params = tail_params(args)?;
    if args.count.is_none() {
        if let Some(n) = default_count {
            params.limit = Some(n);
        }
    }
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
        fields: args.fields.clone(),
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
