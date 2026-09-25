//! `datum now` — snapshot of the currently-tracked fields and their states.

use anyhow::Result;
use chrono::Utc;
use datum_core::db::{Db, NowFilter};
use datum_core::time::DatumTime;

use crate::args::{NowArgs, ShowFlag};
use crate::config;
use crate::render::{render_pending_integrity_warning, render_snapshot};

/// `cli_db` is the global `--db` flag — it beats `DATUM_DB` when naming
/// the active DB for the stale-sync warning (da-cyn4).
pub fn now(args: NowArgs, cli_db: Option<&str>, db: &Db) -> Result<()> {
    // da-tx8j: resolve the optional snapshot_time from time_args. When
    // ANY time arg was supplied we treat it as "the user explicitly
    // asked for a past moment" and pin the whole render to it: query
    // anchor, JSON `now`, "since" timestamps, and band suppression all
    // hang off `snapshot_time.is_some()`.
    let snapshot_time = resolve_snapshot_time(&args)?;

    // da-84cg + da-mpod: the snapshot is the single source of truth for
    // both the JSON wire path and the terminal renderer. Layered
    // disclosure is encoded in the `NowFilter` passed to
    // `db.current_now` — the JSON branch and the terminal branch
    // share the same construction so the two surfaces never drift.
    let filter = NowFilter {
        include_tracked_false: args.show_tracked || args.show_all,
        include_ignored: args.show_all,
        snapshot_time,
    };

    if args.show == ShowFlag::None {
        return Ok(());
    }

    if args.show == ShowFlag::Json {
        // `--show json` bypasses sync banners / integrity warnings /
        // nowview band so the output is a single stable JSON document.
        let snapshot = db.current_now(&filter)?;
        let rendered = serde_json::to_string_pretty(&snapshot)?;
        println!("{rendered}");
        return Ok(());
    }

    // Stale-sync warning is meaningless when the user explicitly
    // pinned a past time — the current sync state has nothing to do
    // with the snapshot they asked for. Suppress.
    if snapshot_time.is_none() {
        emit_stale_sync_warning(cli_db, db);
    }

    // C4e: surface a top-line banner when there are unresolved integrity
    // warnings. Swallow read errors silently — the now view is still
    // useful even if the warning-count query trips (e.g., a future schema
    // migration hasn't run yet on this DB).
    if let Ok(n) = db.pending_integrity_count() {
        if n > 0 {
            print!("{}", render_pending_integrity_warning(n));
        }
    }

    let snapshot = db.current_now(&filter)?;
    if snapshot.active.is_empty()
        && snapshot.tracked_false.is_empty()
        && snapshot.ignored.is_empty()
    {
        // Discriminate "nothing tracked at all" from "tracked but
        // filter-hidden" — the former is the legacy onboarding hint;
        // the latter falls through to `render_snapshot`'s sentinel.
        if db.get_tracked_fields(chrono::Utc::now())?.is_empty() {
            println!("No tracked fields. Use `datum track <field>` or log with `occur`/`start`.");
            return Ok(());
        }
    }

    let state_colors = db.get_all_state_colors()?;

    // da-coi0m: one width for the list and the band — `--cols`, else
    // `$COLUMNS`, else the terminal. Piped with none of them the list
    // prints whole and the band keeps its 80.
    let cols = args
        .cols
        .map(usize::from)
        .or_else(|| crate::render::term_layout::given_cols(20));

    // Overlap-count map is still computed per-field outside the
    // snapshot — `current_now` doesn't carry overlap state. Compute
    // only for the fields actually visible (active + tracked_false +
    // ignored) so the query cost scales with what we'll render.
    let mut overlap_counts = std::collections::HashMap::new();
    for a in &snapshot.active {
        let open = db.open_count_for_field(&a.field)?;
        if open >= 2 {
            overlap_counts.insert(a.field.clone(), open);
        }
    }
    for f in &snapshot.tracked_false {
        let open = db.open_count_for_field(&f.field)?;
        if open >= 2 {
            overlap_counts.insert(f.field.clone(), open);
        }
    }

    print!(
        "{}",
        render_snapshot(
            &snapshot,
            &state_colors,
            &overlap_counts,
            !args.no_color && crate::render::colors::color_enabled(),
            cols,
        )
    );

    // da-raz: `datum now` also renders a terminal nowview band beneath the
    // active-states list. `--bare` suppresses it (e.g. for piping). The
    // band defaults to a 1-hour window, paralleling the SVG `nowview`
    // surface but in cells instead of pixels. A pinned snapshot_time
    // centres it on that instant (da-coi0m), as nowview's is.
    if !args.bare {
        let cols = cols.unwrap_or(80).min(u16::MAX as usize) as u16;
        print_now_band(&args, db, cols, snapshot_time);
    }
    Ok(())
}

/// Resolve `-t/-d/-y/-q/-z` into a snapshot anchor. Returns `None` when
/// no timing flag was supplied (the legacy wall-clock-now path stays in
/// effect). Uses `DatumTime::now()` as the reference so relative
/// offsets (`-y`, `-t -1h`) compose against the host's wall-clock as a
/// user would expect.
fn resolve_snapshot_time(args: &NowArgs) -> Result<Option<chrono::DateTime<Utc>>> {
    let reference = DatumTime::now();
    let from = args
        .time
        .build(&reference)
        .map_err(|e| anyhow::anyhow!("invalid time args for `datum now`: {e}"))?;
    if from.unmodified {
        return Ok(None);
    }
    Ok(from.time.map(|dt| dt.utc))
}

/// Render the terminal nowview band beneath `datum now`. Mirrors the old
/// `datum nowview --terminal` path that lived in commands/nowview.rs.
///
/// `at` is the pinned snapshot time: the band is then centred on it, half
/// its window either side with the clock on the divider (da-coi0m), in
/// place of the history-then-now-panel it draws at the wall clock.
fn print_now_band(args: &NowArgs, db: &Db, cols: u16, at: Option<chrono::DateTime<Utc>>) {
    use datum_layout::nowview_terminal::{render_nowview_terminal, RenderMode, TerminalConfig};

    use crate::commands::nowview::{build_fields, format_local_hh_mm};
    use crate::render::colors;

    const HISTORY_MINUTES: f64 = 60.0;
    const NOW_WIDTH_MINUTES: f64 = 5.0;

    let now = Utc::now();
    let (pivot, history, after) = match at {
        Some(t) => (t, HISTORY_MINUTES / 2.0, HISTORY_MINUTES / 2.0),
        None => (now, HISTORY_MINUTES, 0.0),
    };
    let end = pivot + chrono::Duration::milliseconds((after * 60_000.0) as i64);
    let fields = match build_fields(db, end, HISTORY_MINUTES, false) {
        Ok(f) => f,
        Err(_) => return, // never block the state list on a band-render error
    };
    // da-tpny: the canonical color gate (DATUM_COLOR → NO_COLOR → TTY),
    // so the band colors exactly like the state list above it.
    let use_color = !args.no_color && colors::color_enabled();
    let cfg = TerminalConfig {
        end_ms: pivot.timestamp_millis(),
        now_ms: now.timestamp_millis(),
        history_minutes: history,
        now_width_minutes: NOW_WIDTH_MINUTES,
        after_minutes: after,
        cols,
        rows: 0,
        use_color,
        current_time_label: format_local_hh_mm(pivot),
        mode: RenderMode::InlineBand,
    };
    let band = render_nowview_terminal(&fields, &cfg);
    println!("{band}");
}

/// Prepend a stale-sync warning to `datum now` when sync coverage looks
/// broken for the active DB. C4d upgrade: check daemon liveness too.
///
/// Rules:
/// - No sync configured → no warning
/// - Daemon running AND recent pull (< 1h) → no warning
/// - Daemon down OR stale pull (≥ 1h) → warn, explaining why
fn emit_stale_sync_warning(cli_db: Option<&str>, db: &Db) {
    let threshold = chrono::Duration::hours(1);

    // Active DB name for per-db config lookup. Bare paths have no name.
    // `--db` wins over `DATUM_DB` (da-cyn4).
    let db_name = config::active_db_name(cli_db);
    let cfg = match config::load_raw_config_or_default() {
        Ok(c) => c,
        Err(_) => return,
    };
    if config::resolve_sync_server(&cfg, db_name.as_deref()).is_none() {
        return;
    }

    #[cfg(feature = "native")]
    let daemon_running = crate::commands::sync::daemon_status::read().is_running();
    // No sync client in this build, so no daemon can be running.
    #[cfg(not(feature = "native"))]
    let daemon_running = false;
    // `sync_state` reads a local table and moved to datum-core in da-9d6j, so
    // this line needs no gate — reading the last-pull timestamp never required
    // an HTTP client, which was that bead's whole point.
    let last_pull = datum_core::sync_state::get_last_pull_at(db).ok().flatten();
    let label = db_name.as_deref().unwrap_or("main");

    match (daemon_running, last_pull) {
        // Best case: daemon up, recent pull — say nothing.
        (true, Some(t)) if Utc::now().signed_duration_since(t) <= threshold => {}

        // Daemon running but pull still stale: the daemon's unhealthy
        // (backing off, wrong server, expired token). Tell the user.
        (true, Some(t)) => {
            let age = Utc::now().signed_duration_since(t);
            println!(
                "⚠ sync stale: {label} last pulled {} (daemon running)",
                human_age(age),
            );
        }
        (true, None) => {
            println!("⚠ sync stale: {label} has never pulled (daemon running)");
        }

        // Daemon down: user needs to install or start it.
        (false, _) => {
            let age_part = match last_pull {
                Some(t) => format!(
                    ", last pulled {}",
                    human_age(Utc::now().signed_duration_since(t)),
                ),
                None => "".to_string(),
            };
            println!(
                "⚠ sync daemon not running{age_part} — \
                 run `datum sync install-daemon` to enable auto-sync",
            );
        }
    }
}

fn human_age(age: chrono::Duration) -> String {
    if age.num_hours() < 24 {
        format!("{}h ago", age.num_hours().max(1))
    } else {
        format!("{}d ago", age.num_days().max(1))
    }
}
