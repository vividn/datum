//! The agent runtime: one worker task per auto-sync DB, driven by the
//! pure [`Scheduler`](crate::scheduler::Scheduler). Writes land in the
//! scheduler via a `notify` filesystem watcher; sync operations run on
//! the blocking pool because the underlying `reqwest` client is blocking
//! and the `rusqlite` connection is `!Send`-friendly only through
//! `spawn_blocking`.
//!
//! A single-pass graceful shutdown on SIGTERM / SIGINT drains all
//! in-flight operations and writes a final "clean shutdown" log line.
//!
//! This lives in the library rather than the `datum-sync-agent` binary
//! so the `datum` CLI can host the daemon itself (`datum sync agent`).
//! The installed unit execs `datum`, which the upgrade path already
//! keeps current — a separately-installed agent binary went stale and
//! idled forever with nothing to do (da-ge3e).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::liveness::{default_pid_path, PidFile};
use crate::log as agent_log;
use crate::notifications::{self, Notifier};
use crate::scheduler::{Action, Scheduler};
use crate::targets::{self, SyncTarget};
use crate::watcher::spawn_watcher;

/// How the daemon was asked to run. Shared by the `datum-sync-agent`
/// binary and `datum sync agent`.
#[derive(Debug, Default)]
pub struct Options {
    /// Override the pid-file location (default: `$XDG_RUNTIME_DIR/datum-sync-agent.pid`).
    pub pid_file: Option<PathBuf>,
    /// Verbose logging (debug level).
    pub verbose: bool,
    /// Run one poll cycle and exit (for systemd-less smoke tests).
    pub one_shot: bool,
}

/// Boot the daemon: take the single-instance lock, build the tokio
/// runtime, and run until shutdown (or one pass, under `--one-shot`).
pub fn run(opts: Options) -> Result<()> {
    agent_log::init(opts.verbose);

    let pid_path = opts.pid_file.unwrap_or_else(default_pid_path);
    // `_pid_guard` removes the pid-file on scope exit (including panics).
    let _pid_guard = PidFile::acquire(&pid_path)?;
    info!(pid = %std::process::id(), pid_file = %pid_path.display(), "datum-sync-agent started");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(async move { run_forever(opts.one_shot).await })
}

async fn run_forever(one_shot: bool) -> Result<()> {
    let targets = targets::enumerate()?;
    if targets.is_empty() {
        // Loud, and it names the file it read. Under systemd this is the
        // hardest failure to see: the daemon exits 0, the unit goes
        // `inactive (dead)`, and `datum sync` by hand keeps working — so
        // it looks like the daemon simply isn't syncing (da-ge3e). A
        // user unit also reads config through a MINIMAL environment, so
        // "which file did you read" is the whole diagnosis.
        warn!(
            config = %datum_commands::config::toml_config_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|e| format!("<unresolvable: {e}>")),
            "no auto-sync DBs configured — nothing to sync; exiting. \
             Set a server with `datum sync config set-server <url>`, and \
             check that this config path is the one you edited"
        );
        return Ok(());
    }
    info!(count = targets.len(), "enumerated sync targets");

    // One notifier shared across workers; resolved from config at boot
    // so every pull path uses the same setting. `enabled_default = true`
    // matches the bead spec: "feature-flagged; disabled with
    // `notifications: false`".
    let notifier = notifications::from_config(true);

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
                        notifications::announce_new_warnings(
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
    if let Some(r) = &out.relay_rewound {
        warn!(
            db = %target.name, missing = r.missing,
            "sync server is missing changes this device already pushed \
             (replaced or restored relay); re-sent the full local history"
        );
    }
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
