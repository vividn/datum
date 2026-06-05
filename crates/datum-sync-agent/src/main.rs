//! `datum-sync-agent` binary entry.
//!
//! Runs one worker task per auto-sync DB, driven by the pure
//! [`Scheduler`](datum_sync_agent::scheduler::Scheduler). Writes land in
//! the scheduler via a `notify` filesystem watcher; sync operations run
//! on the blocking pool because the underlying `reqwest` client is
//! blocking and the `rusqlite` connection is `!Send`-friendly only
//! through `spawn_blocking`.
//!
//! A single-pass graceful shutdown on SIGTERM / SIGINT drains all
//! in-flight operations and writes a final "clean shutdown" log line.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ::notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use anyhow::{bail, Context, Result};
use datum_sync_agent::liveness::{default_pid_path, PidFile};
use datum_sync_agent::log as agent_log;
use datum_sync_agent::notify as agent_notify;
use datum_sync_agent::notify::Notifier;
use datum_sync_agent::scheduler::{Action, Scheduler};
use datum_sync_agent::targets::{self, SyncTarget};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tracing::{info, warn};

#[derive(Debug, Default)]
struct Args {
    /// Override the pid-file location (default: $XDG_RUNTIME_DIR/datum-sync-agent.pid).
    pid_file: Option<PathBuf>,
    /// Verbose logging (debug level).
    verbose: bool,
    /// Run one poll cycle and exit (for systemd-less smoke tests).
    one_shot: bool,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut out = Args::default();
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--pid-file" => {
                    let v = it.next().context("--pid-file requires a value")?;
                    out.pid_file = Some(PathBuf::from(v));
                }
                s if s.starts_with("--pid-file=") => {
                    out.pid_file = Some(PathBuf::from(&s["--pid-file=".len()..]));
                }
                "--verbose" | "-v" => out.verbose = true,
                "--one-shot" => out.one_shot = true,
                "--help" | "-h" => {
                    println!(
                        "datum-sync-agent — background daemon that keeps datum DBs in sync\n\
                         \n\
                         USAGE: datum-sync-agent [--pid-file PATH] [-v | --verbose] [--one-shot]"
                    );
                    std::process::exit(0);
                }
                other => bail!("unexpected argument: {other}"),
            }
        }
        Ok(out)
    }
}

fn main() -> Result<()> {
    let args = Args::parse()?;
    agent_log::init(args.verbose);

    let pid_path = args.pid_file.unwrap_or_else(default_pid_path);
    // `_pid_guard` removes the pid-file on scope exit (including panics).
    let _pid_guard = PidFile::acquire(&pid_path)?;
    info!(pid = %std::process::id(), pid_file = %pid_path.display(), "datum-sync-agent started");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(async move { run_forever(args.one_shot).await })
}

async fn run_forever(one_shot: bool) -> Result<()> {
    let targets = targets::enumerate()?;
    if targets.is_empty() {
        info!("no auto-sync DBs configured; exiting");
        return Ok(());
    }
    info!(count = targets.len(), "enumerated sync targets");

    // One notifier shared across workers; resolved from config at boot
    // so every pull path uses the same setting. `enabled_default = true`
    // matches the bead spec: "feature-flagged; disabled with
    // `notifications: false`".
    let notifier = agent_notify::from_config(true);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut tasks: JoinSet<Result<()>> = JoinSet::new();

    for target in targets {
        let rx = shutdown_rx.clone();
        let n = notifier.clone();
        tasks.spawn(run_worker(target, rx, one_shot, n));
    }

    // Graceful-shutdown handler: first SIGTERM / SIGINT flips the
    // shutdown channel; workers finish their in-flight op and exit.
    let shutdown_signal = async {
        wait_for_shutdown_signal().await;
        info!("shutdown signal received; draining workers");
        let _ = shutdown_tx.send(true);
    };

    tokio::pin!(shutdown_signal);
    tokio::select! {
        _ = &mut shutdown_signal => {}
        // In --one-shot mode, workers return Ok(()) after one poll pass;
        // bail out as soon as all of them do.
        _ = async {
            while let Some(res) = tasks.join_next().await {
                match res {
                    Ok(Ok(())) => {},
                    Ok(Err(e)) => warn!(err = %e, "worker exited with error"),
                    Err(e) => warn!(err = %e, "worker task panicked"),
                }
            }
        } => {
            info!("all workers exited; shutting down");
            return Ok(());
        }
    }

    // After the shutdown signal, give each worker up to 5s to drain.
    let drain = async {
        while let Some(res) = tasks.join_next().await {
            if let Ok(Err(e)) = res {
                warn!(err = %e, "worker shutdown error");
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(5), drain).await;
    info!("clean shutdown");
    Ok(())
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM");
    let mut int = signal(SignalKind::interrupt()).expect("install SIGINT");
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// One worker per DB. Runs a `select!` loop over:
/// - fs-notify events (arm the debounce window)
/// - the scheduler's Sleep deadline (poll for next Action)
/// - the shutdown watch channel (drain and exit)
async fn run_worker(
    target: SyncTarget,
    mut shutdown: watch::Receiver<bool>,
    one_shot: bool,
    notifier: std::sync::Arc<dyn Notifier>,
) -> Result<()> {
    let (write_tx, mut write_rx) = mpsc::unbounded_channel::<()>();
    let _watcher = spawn_watcher(&target.path, write_tx)?;
    let target = Arc::new(target);
    let mut scheduler = Scheduler::new(Instant::now());

    info!(db = %target.name, path = %target.path.display(), server = %target.server,
          "worker started");

    loop {
        let now = Instant::now();
        let action = scheduler.poll(now);
        let sleep_for = match action {
            Action::Sleep(d) => d,
            _ => Duration::from_millis(0),
        };

        // Pick the next wakeup cause.
        let sleep_fut = tokio::time::sleep(sleep_for);
        tokio::pin!(sleep_fut);

        tokio::select! {
            biased;

            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    info!(db = %target.name, "worker received shutdown");
                    return Ok(());
                }
            }

            Some(()) = write_rx.recv() => {
                // Coalesce any queued writes in the channel in one go.
                while write_rx.try_recv().is_ok() {}
                scheduler.note_write(Instant::now());
                tracing::debug!(db = %target.name, "write observed");
                continue;
            }

            _ = &mut sleep_fut => {
                // Timer elapsed → fall through to execute the action.
            }
        }

        match action {
            Action::Push => {
                let t = target.clone();
                let res = tokio::task::spawn_blocking(move || do_push(&t)).await;
                match res {
                    Ok(Ok(n)) => {
                        scheduler.note_push_success();
                        info!(db = %target.name, pushed = n, "push ok");
                    }
                    Ok(Err(e)) => {
                        scheduler.note_push_failure(Instant::now());
                        warn!(db = %target.name, err = %e, "push failed");
                    }
                    Err(e) => {
                        scheduler.note_push_failure(Instant::now());
                        warn!(db = %target.name, err = %e, "push task panicked");
                    }
                }
            }
            Action::Pull => {
                let t = target.clone();
                let res = tokio::task::spawn_blocking(move || do_pull(&t)).await;
                match res {
                    Ok(Ok(out)) => {
                        scheduler.note_pull_success(Instant::now());
                        info!(
                            db = %target.name,
                            applied = out.applied,
                            queued = out.integrity_warnings_queued,
                            "pull ok"
                        );
                        // C4e: announce newly-queued integrity warnings.
                        agent_notify::announce_new_warnings(
                            notifier.as_ref(),
                            &target.name,
                            out.integrity_warnings_queued,
                        );
                    }
                    Ok(Err(e)) => {
                        scheduler.note_pull_failure(Instant::now());
                        warn!(db = %target.name, err = %e, "pull failed");
                    }
                    Err(e) => {
                        scheduler.note_pull_failure(Instant::now());
                        warn!(db = %target.name, err = %e, "pull task panicked");
                    }
                }
            }
            Action::Sleep(_) => {
                if one_shot {
                    return Ok(());
                }
            }
        }

        if one_shot {
            return Ok(());
        }
    }
}

/// Spawn a `notify` watcher that forwards every meaningful write event
/// on `path` as a `()` on `tx`. Keeping the watcher value alive for the
/// lifetime of the worker is important — dropping it stops the callback.
fn spawn_watcher(
    path: &std::path::Path,
    tx: mpsc::UnboundedSender<()>,
) -> Result<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any
            ) {
                let _ = tx.send(());
            }
        }
    })?;

    // Watch the DB file directly. If it doesn't exist yet, watch the
    // parent directory so we pick it up on create.
    if path.exists() {
        watcher
            .watch(path, RecursiveMode::NonRecursive)
            .with_context(|| format!("watch {}", path.display()))?;
    } else if let Some(parent) = path.parent() {
        watcher
            .watch(parent, RecursiveMode::NonRecursive)
            .with_context(|| format!("watch parent {}", parent.display()))?;
    }
    Ok(watcher)
}

/// Execute a push for `target`. Blocks; runs inside `spawn_blocking`.
///
/// Drives the sync cycle through `datum_sync_client::sync_once::push_once`
/// directly — `datum-commands` (clap + render) is only consulted for
/// preflight (token/key/device-id loading), which is local-state I/O,
/// not CLI surface.
fn do_push(target: &SyncTarget) -> Result<usize> {
    use datum_commands::commands::sync::{build_session, preflight_checks};
    use datum_core::db::Db;
    use datum_sync_client::sync_once;
    use std::time::Duration as StdDuration;

    let pre = preflight_checks(&target.server)?;
    let session = build_session(&target.server, &pre.token);
    let client = reqwest::blocking::Client::builder()
        .timeout(StdDuration::from_secs(30))
        .build()
        .context("build reqwest client")?;
    let db = Db::open(&target.path.to_string_lossy())
        .with_context(|| format!("open {}", target.path.display()))?;
    let out = sync_once::push_once(
        &target.server,
        &client,
        &session,
        &pre.device_id,
        &pre.master_key,
        &db,
        false,
    )?;
    if let Some(r) = &out.seq_rebased {
        warn!(
            db = %target.name, device_id = %r.device_id,
            predicted = r.predicted, expected = r.expected,
            "sync seq diverged; re-based to relay's seq and retried"
        );
    }
    Ok(out.pushed)
}

/// Execute a pull for `target`. Blocks; runs inside `spawn_blocking`.
/// Returns the full [`PullOutcome`] so the caller can both log
/// `applied` and decide whether to fire a desktop notification on
/// `integrity_warnings_queued > 0`.
fn do_pull(target: &SyncTarget) -> Result<datum_sync_client::sync_once::PullOutcome> {
    use datum_commands::commands::sync::{build_session, preflight_checks};
    use datum_core::db::Db;
    use datum_sync_client::sync_once;
    use std::time::Duration as StdDuration;

    let pre = preflight_checks(&target.server)?;
    let session = build_session(&target.server, &pre.token);
    let client = reqwest::blocking::Client::builder()
        .timeout(StdDuration::from_secs(30))
        .build()
        .context("build reqwest client")?;
    let db = Db::open(&target.path.to_string_lossy())
        .with_context(|| format!("open {}", target.path.display()))?;
    let out = sync_once::pull_once(
        &target.server,
        &client,
        &session,
        &pre.device_id,
        &pre.master_key,
        &db,
        false,
    )?;
    Ok(out)
}
