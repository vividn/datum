//! `datum tail` — list the most recent entries.

use anyhow::Result;
use chrono::{Local, TimeZone, Utc};
use datum_core::{
    db::{Db, ListParams, TimeMetric},
    entry::Entry,
    time::{parse_date_arg, parse_time_arg, resolve_timezone, DatumTime, TimeBuilder},
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
    let auto_count = if tty && args.count.is_none() {
        Some(auto_fill_rows(detect_rows()))
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
        if tty {
            // da-3l6m: on a real terminal, hand off to the scrollable
            // alt-screen TUI (key/mouse input + live follow). It owns the
            // screen until the user quits, then restores it. Non-TTY
            // consumers stay on the append/repaint loop below.
            return crate::commands::tail_tui::run(&args, db, metric, registry.as_ref(), interval);
        }
        // da-d6yo: seed the watch loop with the signature of what we
        // just painted so the first tick's no-op render is suppressed.
        let initial = frame_signature(tty, &entries);
        watch_repaint_loop(&args, db, metric, registry.as_ref(), interval, initial)?;
    }

    Ok(())
}

/// Per-frame fingerprint used by the watch loop to suppress idle
/// flicker (da-d6yo). Two ticks that produce the same `(tty, cols,
/// rows, entry_count, max_modify_time)` paint identical pixels, so
/// the repaint is a no-op — we skip the clear+redraw and let the
/// existing frame sit. The terminal dimensions are folded in so a
/// SIGWINCH-driven wake (da-hsqd) that arrives in the same tick still
/// repaints whenever the new size actually differs from the last.
type FrameSig = (bool, usize, usize, usize, Option<chrono::DateTime<Utc>>);

fn frame_signature(tty: bool, entries: &[Entry]) -> FrameSig {
    let (cols, rows) = if tty {
        (detect_cols(), detect_rows())
    } else {
        (0, 0)
    };
    let max_modify = entries.iter().map(|e| e.modify_time).max();
    (tty, cols, rows, entries.len(), max_modify)
}

/// da-o50m: `tail --watch` re-fetches the window, recomputes the
/// column-width + row-count budget against the live terminal and the
/// fresh row set, clears the screen, and repaints — TTY or not.
///
/// da-d6yo: repaints only fire when the frame signature changes
/// (entry count / max modify_time / terminal dimensions). Idle ticks
/// where nothing has changed are no-ops, so the watch surface no
/// longer flickers between empty polls. The signature is seeded by
/// the initial paint in [`tail`] so the first tick after entering
/// the loop doesn't re-render identical pixels.
///
/// Background: the previous (da-xorl) implementation gated the
/// repaint behind `is_stdout_tty()`, falling back to an append-only
/// loop for non-tty consumers. That left agent-driven sessions where
/// stdout isn't a real TTY (tmux/PTY wrappers, IDE terminals, etc.)
/// stuck on the legacy append path — and that path computes column
/// widths per-batch, so each tick's new rows render with their own
/// alignment, producing the staircase artefact vividn caught.
///
/// Trade-off: piped consumers now see `\x1b[2J\x1b[H` escapes between
/// frames whenever data does change. Per the bead, this is the right
/// call — `datum tail --watch` is an interactive surface; scripts
/// that need a streaming event feed should use a structured surface,
/// not scrape the ANSI-aware human view.
///
/// da-hsqd: a `SIGWINCH` handler flips [`RESIZE_FLAG`]; the loop's
/// chunked sleep ([`wait_with_resize_wake`]) returns early when the
/// flag is set, so a window resize triggers an out-of-band repaint
/// instead of waiting for the next `--watch` tick. The watch is a
/// custom poll loop (no crossterm event source), so per the bead the
/// signal-hook path is the right one. No-op on non-unix targets.
fn watch_repaint_loop(
    args: &TailArgs,
    db: &Db,
    metric: TimeMetricCol,
    registry: Option<&datum_core::field_spec::SpecRegistry>,
    interval: std::time::Duration,
    initial_sig: FrameSig,
) -> Result<()> {
    use std::io::Write;
    install_sigwinch_handler();
    let columns = split_columns(&args.columns);
    let mut last_sig = initial_sig;
    loop {
        wait_with_resize_wake(interval);
        // Re-detect the terminal every tick so a live resize between
        // frames updates the layout. `detect_cols`/`detect_rows` query
        // `TIOCGWINSZ` directly on TTYs, so the post-resize repaint
        // uses the new dimensions. Non-tty consumers get the same
        // unbounded path as the initial render: no width budget,
        // default row count.
        let tty = is_stdout_tty();
        let max_total_width = if tty { Some(detect_cols()) } else { None };
        let auto_count = if tty && args.count.is_none() {
            Some(auto_fill_rows(detect_rows()))
        } else {
            None
        };
        let entries = tail_entries_with_default_count(args, auto_count, db)?;
        let sig = frame_signature(tty, &entries);
        if sig == last_sig {
            continue;
        }
        last_sig = sig;
        // Canonical clear+home — leaves scrollback intact on Ctrl-C
        // (no alt-screen entry). Flush so the escape lands before
        // the table writes.
        print!("\x1b[2J\x1b[H");
        let _ = std::io::stdout().flush();
        let tbl = TableArgs {
            output: args.output()?,
            metric,
            columns: columns.clone(),
            suppress_header: false,
            max_total_width,
        };
        let now = DatumTime::now();
        let ctxs = build_render_contexts(&entries, db);
        print_table_with(&entries, Some(&ctxs), Some(&now), registry, &tbl);
    }
}

/// SIGWINCH-handling primitives (da-hsqd). The handler runs on the
/// signal-delivery context, so the only thing it touches is an atomic
/// bool — async-signal-safe. The main loop polls/clears the flag.
use std::sync::atomic::{AtomicBool, Ordering};

static RESIZE_FLAG: AtomicBool = AtomicBool::new(false);
static HANDLER_INSTALLED: std::sync::Once = std::sync::Once::new();

#[cfg(unix)]
extern "C" fn on_sigwinch(_: libc::c_int) {
    RESIZE_FLAG.store(true, Ordering::Relaxed);
}

/// Idempotently install a SIGWINCH handler that flips
/// [`RESIZE_FLAG`]. No-op on non-unix targets and on subsequent calls
/// — the watch loop calls this on entry and we never uninstall (the
/// handler is process-wide; a tail-watch session owns the process for
/// its lifetime).
fn install_sigwinch_handler() {
    HANDLER_INSTALLED.call_once(|| {
        #[cfg(unix)]
        // SAFETY: `sigaction` is documented to read/write `sigaction`
        // structs; we zero-init the struct, set the handler to a
        // valid `extern "C"` fn, and pass a null `oldact`. `SA_RESTART`
        // keeps unrelated syscalls (e.g. db reads) restartable
        // through SIGWINCH; our wake path is the flag, not EINTR.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_sigwinch as *const () as libc::sighandler_t;
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_flags = libc::SA_RESTART;
            libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());
        }
    });
}

/// Sleep up to `interval`, but return early if [`RESIZE_FLAG`] is
/// set — a SIGWINCH between frames should trigger an immediate
/// repaint instead of waiting out the remainder of the watch tick.
///
/// Implementation: poll the flag every 50 ms. We can't reliably rely
/// on the signal interrupting `std::thread::sleep` because Rust's
/// implementation restarts on EINTR (clock_nanosleep absolute-time
/// semantics). 50 ms is well under perception threshold for a
/// "redraw on resize" UX and keeps the steady-state wake cost
/// negligible (20 Hz of an atomic-load when idle).
fn wait_with_resize_wake(interval: std::time::Duration) {
    use std::time::{Duration, Instant};
    // Clear any flag carried over from before we entered the wait —
    // a resize during the previous frame's render is already
    // reflected in what we just painted.
    RESIZE_FLAG.store(false, Ordering::Relaxed);
    let deadline = Instant::now() + interval;
    let chunk = Duration::from_millis(50);
    loop {
        if RESIZE_FLAG.load(Ordering::Relaxed) {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        let remaining = deadline - now;
        std::thread::sleep(remaining.min(chunk));
    }
}

/// Auto-fill row budget for `datum tail` when stdout is a TTY and the
/// user didn't pin `-n`. Reserves **2 rows** of terminal height:
///
/// * one for the header (`time field state hid`);
/// * one for the cursor-parking line that the trailing `\n` on the last
///   printed row puts the cursor onto.
///
/// Without the second slot the cursor lands on row `lines + 1`, which
/// scrolls the visible window up by one — taking the header off the top
/// with it on every repaint (the da-fu5l bug). `max(1)` keeps the bound
/// positive on absurdly small terminals so the query still returns rows.
fn auto_fill_rows(lines: usize) -> usize {
    lines.saturating_sub(2).max(1)
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
        // da-ghlx: `--show-meta` (alias `--show-all-metadata`) flips
        // the default-hide filter on `extra_metadata.flag = true`
        // (da-dnnn review markers) and `extra_metadata.meta = true`
        // (da-xn8j protocol notes). Default off — those rows live
        // off the live tail surface and are surfaced via
        // `datum check flags` / `datum meta show <field>` instead.
        include_meta: args.show_meta,
    })
}

struct TailWindow {
    point_utc: chrono::DateTime<Utc>,
    day_start_utc: chrono::DateTime<Utc>,
    day_end_utc: chrono::DateTime<Utc>,
}

fn build_window(args: &TailArgs) -> Result<TailWindow> {
    // da-nxm6: `-z`/`--timezone` resolves to an IANA name (or the host
    // locale's name when omitted) and feeds two surfaces: the
    // `TimeBuilder` that anchors the reference moment, and the day-
    // boundary computation below. Both must agree — building the
    // moment in tz A while computing day edges in `Local` would drop
    // border rows whenever the two zones disagree on what date "now"
    // falls on.
    let tz_name = match args.time.timezone.as_deref() {
        Some(spec) => Some(
            resolve_timezone(spec)
                .map_err(|e| anyhow::anyhow!("Invalid -z/--timezone value '{spec}': {e}"))?,
        ),
        None => None,
    };
    let mut builder = TimeBuilder::now()
        .with_yesterday_count(args.time.yesterday as i64)
        .with_quick_count(args.time.quick as i64)
        .with_timezone(tz_name.clone());
    if let Some(d) = &args.time.date {
        builder = builder.with_date_arg(parse_date_arg(d)?);
    }
    if let Some(t) = &args.time.time {
        builder = builder.with_time_arg(parse_time_arg(t)?);
    }
    let point = builder.build();
    let (day_start_utc, day_end_utc) = day_bounds_utc(point.utc, tz_name.as_deref())?;
    Ok(TailWindow {
        point_utc: point.utc,
        day_start_utc,
        day_end_utc,
    })
}

/// Compute the ±1-day-padded local-day bounds for `utc` in the user-
/// supplied IANA zone, or fall back to the host's `Local` zone when the
/// flag is absent. Returns `(start_utc, end_utc)`. Errors only when the
/// IANA name fails to parse — `resolve_timezone` already validates the
/// spec upstream, so this is a defence-in-depth bail.
fn day_bounds_utc(
    utc: chrono::DateTime<Utc>,
    tz_name: Option<&str>,
) -> Result<(chrono::DateTime<Utc>, chrono::DateTime<Utc>)> {
    if let Some(name) = tz_name {
        let tz: chrono_tz::Tz = name
            .parse()
            .map_err(|_| anyhow::anyhow!("Unknown timezone: {name}"))?;
        let local = utc.with_timezone(&tz);
        let day = local.date_naive();
        let start = tz
            .from_local_datetime(
                &(day - chrono::Duration::days(1))
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .single()
            .ok_or_else(|| anyhow::anyhow!("ambiguous local time in zone {name}"))?;
        let end = tz
            .from_local_datetime(
                &(day + chrono::Duration::days(1))
                    .and_hms_opt(23, 59, 59)
                    .unwrap(),
            )
            .single()
            .ok_or_else(|| anyhow::anyhow!("ambiguous local time in zone {name}"))?;
        Ok((start.with_timezone(&Utc), end.with_timezone(&Utc)))
    } else {
        let local = utc.with_timezone(&Local);
        let day = local.date_naive();
        let start = Local
            .from_local_datetime(
                &(day - chrono::Duration::days(1))
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap();
        let end = Local
            .from_local_datetime(
                &(day + chrono::Duration::days(1))
                    .and_hms_opt(23, 59, 59)
                    .unwrap(),
            )
            .single()
            .unwrap();
        Ok((start.with_timezone(&Utc), end.with_timezone(&Utc)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    // `RESIZE_FLAG` is a process-global, so tests that mutate it must
    // not run concurrently — otherwise a "set" test flips the flag
    // out from under a "clear" test and the latter wakes early. The
    // shared env lock is the canonical process-global-state lock.
    use datum_test_util::env_lock;

    /// With the flag clear, `wait_with_resize_wake` blocks for at
    /// least the requested interval (give or take chunk granularity).
    #[test]
    fn wait_blocks_full_interval_when_no_resize() {
        let _guard = env_lock();
        RESIZE_FLAG.store(false, Ordering::Relaxed);
        let target = Duration::from_millis(150);
        let start = Instant::now();
        wait_with_resize_wake(target);
        let elapsed = start.elapsed();
        // Allow one chunk of slack — Linux's `clock_nanosleep` can
        // return slightly early on some kernels, and the loop's last
        // sub-chunk sleep is `remaining.min(chunk)` not a full chunk.
        assert!(
            elapsed >= target.saturating_sub(Duration::from_millis(20)),
            "expected wait near {target:?}, got {elapsed:?}"
        );
        assert!(
            elapsed < target + Duration::from_millis(500),
            "wait overran significantly: {elapsed:?}"
        );
    }

    /// A flag set *during* the wait returns within a chunk (~50 ms).
    /// We don't rely on a real SIGWINCH — just flip the atomic from
    /// another thread and verify the loop wakes early.
    #[test]
    fn wait_returns_early_when_flag_set_mid_wait() {
        let _guard = env_lock();
        RESIZE_FLAG.store(false, Ordering::Relaxed);
        let handle = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(60));
            RESIZE_FLAG.store(true, Ordering::Relaxed);
        });
        let start = Instant::now();
        wait_with_resize_wake(Duration::from_secs(5));
        let elapsed = start.elapsed();
        handle.join().unwrap();
        // Should wake within ~chunk-granularity of the flag flip
        // (60 ms + 50 ms slack). Generous ceiling to avoid CI flake.
        assert!(
            elapsed < Duration::from_millis(500),
            "wait did not return promptly on resize: {elapsed:?}"
        );
    }

    /// `wait_with_resize_wake` clears any pre-existing flag on entry
    /// so a leftover signal from before the wait doesn't immediately
    /// return — that would burn through the interval in a tight loop.
    #[test]
    fn wait_clears_stale_flag_on_entry() {
        let _guard = env_lock();
        RESIZE_FLAG.store(true, Ordering::Relaxed);
        let target = Duration::from_millis(120);
        let start = Instant::now();
        wait_with_resize_wake(target);
        let elapsed = start.elapsed();
        assert!(
            elapsed >= target.saturating_sub(Duration::from_millis(20)),
            "wait short-circuited on a stale flag: {elapsed:?}"
        );
    }

    /// Idempotency: calling `install_sigwinch_handler` twice is safe.
    /// The `Once` guard means only the first call actually wires the
    /// handler; subsequent calls return immediately.
    #[test]
    fn install_sigwinch_handler_is_idempotent() {
        install_sigwinch_handler();
        install_sigwinch_handler();
    }

    // ─── da-fu5l: auto-fill row budget ──────────────────────────────────

    /// Typical 24-line terminal: budget is 22 data rows so header +
    /// data + cursor-park-line = 24 rows total. One more row would
    /// scroll the header off.
    #[test]
    fn auto_fill_reserves_two_rows_on_normal_terminal() {
        assert_eq!(auto_fill_rows(24), 22);
        assert_eq!(auto_fill_rows(50), 48);
    }

    /// Tiny terminals stay positive — the `max(1)` floor keeps the
    /// query from collapsing to zero rows on absurd inputs.
    #[test]
    fn auto_fill_clamps_to_one_on_tiny_terminal() {
        assert_eq!(auto_fill_rows(2), 1);
        assert_eq!(auto_fill_rows(1), 1);
        assert_eq!(auto_fill_rows(0), 1);
    }

    /// The header-plus-data row count must fit inside the terminal
    /// height with at least one row to spare for cursor parking —
    /// expressed as the invariant `header + auto_fill_rows(lines) + 1
    /// <= lines` for all lines where the formula isn't clamped.
    #[test]
    fn auto_fill_invariant_leaves_room_for_cursor_park() {
        for lines in 4..=200 {
            let header = 1;
            let trailing_park = 1;
            assert!(
                header + auto_fill_rows(lines) + trailing_park <= lines,
                "row budget overflows at LINES={lines}: data={}, total={}",
                auto_fill_rows(lines),
                header + auto_fill_rows(lines) + trailing_park,
            );
        }
    }
}
