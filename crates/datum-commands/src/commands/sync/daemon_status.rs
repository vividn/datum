//! Daemon-liveness readout consumed by `datum sync status`.
//!
//! The agent writes `$XDG_RUNTIME_DIR/datum-sync-agent.pid` (or a
//! `/tmp` fallback) on startup; we read it here to decide whether to
//! show `● running` or `○ not running`.
//!
//! This module owns the canonical [`default_pid_path`];
//! `datum-sync-agent::liveness` re-exports it, so writer and reader
//! agree by construction.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{DateTime, Utc};

/// Default pid-file path used by the agent. Canonical single definition —
/// `datum-sync-agent::liveness` re-exports this.
///
/// - `$XDG_RUNTIME_DIR/datum-sync-agent.pid` if set
/// - `/tmp/datum-sync-agent-<uid>.pid` otherwise (matches systemd fallback,
///   keyed by uid so two users on one box don't collide)
pub fn default_pid_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("datum-sync-agent.pid");
        }
    }
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/datum-sync-agent-{uid}.pid"))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from("/tmp/datum-sync-agent.pid")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonState {
    pub pid_file: PathBuf,
    pub pid: Option<i32>,
    pub alive: bool,
    pub started_at: Option<DateTime<Utc>>,
}

impl DaemonState {
    pub fn is_running(&self) -> bool {
        self.alive
    }
}

pub fn read() -> DaemonState {
    read_from(default_pid_path())
}

pub fn read_from(path: PathBuf) -> DaemonState {
    let pid = match fs::read_to_string(&path) {
        Ok(s) => s.trim().parse::<i32>().ok().filter(|p| *p > 0),
        Err(_) => None,
    };
    let alive = pid.map(is_pid_alive).unwrap_or(false);
    let started_at = fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(system_time_to_utc);
    DaemonState {
        pid_file: path,
        pid,
        alive,
        started_at,
    }
}

/// True if `pid` is alive. Treats any non-zero `kill(pid, 0)` (including
/// EPERM) as "not alive" — the daemon runs as the same user as the CLI,
/// so the cross-user EPERM edge case doesn't apply in practice.
fn is_pid_alive(pid: i32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn system_time_to_utc(t: SystemTime) -> Option<DateTime<Utc>> {
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    DateTime::<Utc>::from_timestamp(dur.as_secs() as i64, dur.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_pid_file_reports_not_running() {
        let tmp = tempfile::TempDir::new().unwrap();
        let s = read_from(tmp.path().join("nope.pid"));
        assert_eq!(s.pid, None);
        assert!(!s.is_running());
    }

    #[test]
    fn own_pid_is_alive() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("pid");
        fs::write(&path, format!("{}\n", std::process::id())).unwrap();
        let s = read_from(path);
        assert!(s.alive);
        assert!(s.started_at.is_some());
    }

    #[test]
    fn dead_pid_reports_not_running() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("pid");
        fs::write(&path, "2147483646\n").unwrap();
        let s = read_from(path);
        assert!(!s.alive);
        assert_eq!(s.pid, Some(2147483646));
    }
}
