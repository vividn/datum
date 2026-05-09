//! `datum sync status` — render the per-DB sync state.
//!
//! Output is human-friendly, not machine-readable. Per the bead:
//!
//! ```text
//! Server: https://sync.example.com
//! User:   vividn@vividn.dev
//!
//! main          ↑ 2026-04-21 15:00 (3m ago)   ↓ 2026-04-21 15:00 (3m ago)   ✓
//! personal      ↑ 2026-04-21 14:32 (31m ago)  ↓ 2026-04-21 14:32 (31m ago)  ⚠ 2 conflicts
//! scratch       sync disabled
//! ```
//!
//! Current scope: render the *active* DB only. Multi-DB enumeration is
//! trivial to add once the DB registry surfaces a "list all named DBs"
//! API (C6 territory).

use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use datum_core::db::Db;
use datum_sync_client::sync_state;

use crate::args::SyncStatusArgs;
use crate::commands::sync::daemon_status;
use crate::commands::sync::key::load_token;
use crate::config::{db_path, load_raw_config_or_default, resolve_db, resolve_sync_server};

pub fn run(args: SyncStatusArgs) -> Result<()> {
    let cfg = load_raw_config_or_default()?;
    let db_name = active_db_name();
    let effective_server = args
        .server
        .clone()
        .or_else(|| resolve_sync_server(&cfg, db_name.as_deref()));

    let (label_server, user) = match effective_server.as_deref() {
        Some(url) => {
            let user = load_token(url)
                .ok()
                .flatten()
                .and_then(|t| t.user_id)
                .unwrap_or_else(|| "(not logged in)".to_string());
            (url.to_string(), user)
        }
        None => ("(disabled)".to_string(), "(n/a)".to_string()),
    };
    println!("Server: {label_server}");
    println!("User:   {user}");
    println!("Daemon: {}", daemon_line());
    println!();

    // Render the active DB's row.
    let active_label = db_name.clone().unwrap_or_else(|| "(default)".to_string());
    let (line, disabled) = render_db_row(&active_label, effective_server.as_deref())?;
    println!("{line}");
    if !disabled {
        // Nothing else to print for now — multi-DB enumeration is a later
        // iteration. Keep the output predictable for tests.
    }
    Ok(())
}

/// Compose the `Daemon:` row. Values:
/// - `● running (pid N) started <ts>` when the agent is alive
/// - `○ not running — run \`datum sync install-daemon\`` when it's not
fn daemon_line() -> String {
    let state = daemon_status::read();
    if state.is_running() {
        let pid = state.pid.unwrap_or(0);
        let started = state
            .started_at
            .map(|t| t.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "?".to_string());
        format!("● running (pid {pid}) started {started}")
    } else {
        "○ not running — run `datum sync install-daemon`".to_string()
    }
}

fn active_db_name() -> Option<String> {
    std::env::var("DATUM_DB")
        .ok()
        .filter(|v| !v.is_empty() && !crate::config::looks_like_path(v))
}

/// Compose the `<name>  ↑ <ts>  ↓ <ts>  <flag>` line for a single DB.
/// Returns `(line, disabled)` so the caller can skip the trailing legend
/// when sync is off for this DB.
fn render_db_row(label: &str, server: Option<&str>) -> Result<(String, bool)> {
    if server.is_none() {
        return Ok((format!("{:<12}  sync disabled", label), true));
    }

    // Open the active DB so we can read watermarks + pending conflicts.
    // Failing to open it is non-fatal for `status` — we still print the
    // header so users can diagnose config issues.
    let Ok(resolved) = resolve_db(None) else {
        return Ok((format!("{:<12}  (db unavailable)", label), false));
    };
    let _ = db_path()?; // side-effect: ensures data dir exists
    let db = match Db::open(&resolved.path.to_string_lossy()) {
        Ok(d) => d,
        Err(_) => return Ok((format!("{:<12}  (db unavailable)", label), false)),
    };

    let last_push = sync_state::get_last_push_at(&db)?;
    let last_pull = sync_state::get_last_pull_at(&db)?;
    let pending = pending_conflicts(&db)?;

    let flag = match pending {
        0 => "✓".to_string(),
        n => format!("⚠ {n} conflict{}", if n == 1 { "" } else { "s" }),
    };
    Ok((
        format!(
            "{:<12}  ↑ {}  ↓ {}  {}",
            label,
            fmt_timestamp(last_push),
            fmt_timestamp(last_pull),
            flag,
        ),
        false,
    ))
}

fn fmt_timestamp(ts: Option<DateTime<Utc>>) -> String {
    match ts {
        None => "never                   ".to_string(),
        Some(dt) => {
            let local = dt.with_timezone(&Local);
            let ago = time_ago(dt);
            format!("{} ({ago})", local.format("%Y-%m-%d %H:%M"))
        }
    }
}

fn time_ago(past: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now.signed_duration_since(past);
    if delta.num_seconds() < 60 {
        return "just now".to_string();
    }
    let mins = delta.num_minutes();
    if mins < 60 {
        return format!("{mins}m ago");
    }
    let hours = delta.num_hours();
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = delta.num_days();
    format!("{days}d ago")
}

fn pending_conflicts(db: &Db) -> Result<usize> {
    let n: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM sync_conflicts WHERE resolution IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(n as usize)
}
