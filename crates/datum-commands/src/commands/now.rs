//! `datum now` — snapshot of the currently-tracked fields and their states.

use anyhow::Result;
use chrono::Utc;
use datum_core::db::{Db, NowFilter};
use datum_core::time::DatumTime;

use crate::args::NowArgs;
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

    if args.json {
        // `--json` bypasses sync banners / integrity warnings / nowview
        // band so the output is a single stable JSON document.
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
        render_snapshot(&snapshot, &state_colors, &overlap_counts)
    );

    // da-raz: `datum now` also renders a terminal nowview band beneath the
    // active-states list. `--bare` suppresses it (e.g. for piping). The
    // band defaults to a 1-hour window, paralleling the SVG `nowview`
    // surface but in cells instead of pixels.
    //
    // da-tx8j: also suppress the band when the user pinned a past
    // snapshot_time. The band is anchored to *real* now (its
    // history-minutes window literally runs up to `Utc::now()`); a band
    // anchored to wall-clock now alongside a snapshot showing the past
    // would conflict semantically — vividn's intuition in the bead.
    if !args.bare && snapshot_time.is_none() {
        print_now_band(&args, db);
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
fn print_now_band(args: &NowArgs, db: &Db) {
    use std::io::IsTerminal;

    use datum_layout::nowview_terminal::{render_nowview_terminal, RenderMode, TerminalConfig};

    use crate::commands::nowview::{build_fields, format_local_hh_mm};

    const HISTORY_MINUTES: f64 = 60.0;
    const NOW_WIDTH_MINUTES: f64 = 5.0;

    let end = Utc::now();
    let fields = match build_fields(db, end, HISTORY_MINUTES, false) {
        Ok(f) => f,
        Err(_) => return, // never block the state list on a band-render error
    };
    let cols = args.cols.unwrap_or_else(detect_terminal_cols);
    let use_color =
        !args.no_color && std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal();
    let cfg = TerminalConfig {
        end_ms: end.timestamp_millis(),
        history_minutes: HISTORY_MINUTES,
        now_width_minutes: NOW_WIDTH_MINUTES,
        cols,
        rows: 0,
        use_color,
        current_time_label: format_local_hh_mm(end),
        mode: RenderMode::InlineBand,
    };
    let band = render_nowview_terminal(&fields, &cfg);
    println!("{band}");
}

/// Detect terminal width via `$COLUMNS`, falling back to 80. Pure-Rust
/// (no termios crate) so the code path stays wasm-safe in case a host
/// embedder ever calls into this from a non-tty context.
fn detect_terminal_cols() -> u16 {
    if let Ok(s) = std::env::var("COLUMNS") {
        if let Ok(n) = s.parse::<u16>() {
            if n >= 20 {
                return n;
            }
        }
    }
    80
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

    let daemon_running = crate::commands::sync::daemon_status::read().is_running();
    let last_pull = datum_sync_client::sync_state::get_last_pull_at(db)
        .ok()
        .flatten();
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
