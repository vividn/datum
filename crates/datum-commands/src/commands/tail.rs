//! `datum tail` — list the most recent entries.

use anyhow::Result;
use datum_core::{db::Db, time::DatumTime};

use crate::args::{OutputFlagsExt, TailArgs};
use crate::cmd::{build_render_contexts, load_registry_opt, split_columns, tail_entries};
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
