//! `datum now` — snapshot of the currently-tracked fields and their states.

use anyhow::Result;
use chrono::Utc;
use datum_core::db::Db;

use crate::args::NowArgs;
use crate::config;
use crate::render::{print_now_view, render_pending_conflict_warning};

pub fn now(args: NowArgs, db: &Db) -> Result<()> {
    emit_stale_sync_warning(db);

    // C4e: surface a top-line warning when there are unresolved conflicts.
    // We swallow read errors silently — the now view is still useful even
    // if the conflict-count query trips (e.g., a future schema migration
    // hasn't run yet on this DB).
    if let Ok(n) = db.pending_conflict_count() {
        if n > 0 {
            print!("{}", render_pending_conflict_warning(n));
        }
    }

    let tracked = db.get_tracked_fields()?;
    if tracked.is_empty() {
        println!("No tracked fields. Use `datum track <field>` or log with `occur`/`start`.");
        return Ok(());
    }

    let state_colors = db.get_all_state_colors()?;

    let mut latest_per_field = std::collections::HashMap::new();
    let mut overlap_counts = std::collections::HashMap::new();
    for tf in &tracked {
        if tf.ignored && !args.show_all {
            continue;
        }
        if let Some(entry) = db.latest_entry_for_now(&tf.field)? {
            latest_per_field.insert(tf.field.clone(), entry);
        }
        let open = db.open_count_for_field(&tf.field)?;
        if open >= 2 {
            overlap_counts.insert(tf.field.clone(), open);
        }
    }

    print_now_view(
        &tracked,
        &latest_per_field,
        &state_colors,
        &overlap_counts,
        args.show_all,
    );

    // da-raz: `datum now` also renders a terminal nowview band beneath the
    // active-states list. `--bare` suppresses it (e.g. for piping). The
    // band defaults to a 1-hour window, paralleling the SVG `nowview`
    // surface but in cells instead of pixels.
    if !args.bare {
        print_now_band(&args, db);
    }
    Ok(())
}

/// Render the terminal nowview band beneath `datum now`. Mirrors the old
/// `datum nowview --terminal` path that lived in commands/nowview.rs.
fn print_now_band(args: &NowArgs, db: &Db) {
    use std::io::IsTerminal;

    use datum_layout::nowview_terminal::{render_nowview_terminal, TerminalConfig};

    use crate::commands::nowview::{build_fields, format_local_hh_mm};

    const HISTORY_MINUTES: f64 = 60.0;
    const NOW_WIDTH_MINUTES: f64 = 5.0;

    let end = Utc::now();
    let fields = match build_fields(db, end, HISTORY_MINUTES) {
        Ok(f) => f,
        Err(_) => return, // never block the state list on a band-render error
    };
    let cols = args.cols.unwrap_or_else(detect_terminal_cols);
    let use_color = !args.no_color
        && std::env::var_os("NO_COLOR").is_none()
        && std::io::stdout().is_terminal();
    let cfg = TerminalConfig {
        end_ms: end.timestamp_millis(),
        history_minutes: HISTORY_MINUTES,
        now_width_minutes: NOW_WIDTH_MINUTES,
        cols,
        use_color,
        current_time_label: format_local_hh_mm(end),
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
fn emit_stale_sync_warning(db: &Db) {
    let threshold = chrono::Duration::hours(1);

    // Active DB name for per-db config lookup. Bare paths have no name.
    let db_name = std::env::var("DATUM_DB").ok().filter(|v| {
        !v.is_empty() && !config::looks_like_path(v)
    });
    let cfg = match config::load_raw_config_or_default() {
        Ok(c) => c,
        Err(_) => return,
    };
    if config::resolve_sync_server(&cfg, db_name.as_deref()).is_none() {
        return;
    }

    let daemon_running =
        crate::commands::sync::daemon_status::read().is_running();
    let last_pull =
        datum_sync_client::sync_state::get_last_pull_at(db).ok().flatten();
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
            println!(
                "⚠ sync stale: {label} has never pulled (daemon running)"
            );
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
