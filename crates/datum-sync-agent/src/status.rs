//! Daemon-liveness readout consumed by `datum sync status`.
//!
//! `datum-sync-agent` writes `$XDG_RUNTIME_DIR/datum-sync-agent.pid` on
//! startup; the CLI reads it (via [`read`]) to render the top-line
//! `Daemon: ● running (pid N) started <ts>` row.

use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{DateTime, Utc};

use crate::liveness::{default_pid_path, is_pid_alive, read_pid_file};

/// Snapshot of what the status command cares about. All fields are
/// derived from the pid-file and its metadata — cheap, no HTTP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonState {
    pub pid_file: PathBuf,
    pub pid: Option<i32>,
    pub alive: bool,
    /// Modification time of the pid-file = when the agent last wrote it.
    /// The daemon writes once at startup, so this doubles as "started at".
    pub started_at: Option<DateTime<Utc>>,
}

impl DaemonState {
    /// True if we believe a daemon is running right now.
    pub fn is_running(&self) -> bool {
        self.alive
    }
}

/// Read the pid-file from its default location and translate to a
/// [`DaemonState`]. Never fails — missing / unreadable files just
/// surface as "not running".
pub fn read() -> DaemonState {
    read_from(default_pid_path())
}

pub fn read_from(path: PathBuf) -> DaemonState {
    let pid = read_pid_file(&path).ok().flatten();
    let alive = pid.map(is_pid_alive).unwrap_or(false);
    let started_at = std::fs::metadata(&path)
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

fn system_time_to_utc(t: SystemTime) -> Option<DateTime<Utc>> {
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    DateTime::<Utc>::from_timestamp(dur.as_secs() as i64, dur.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn missing_pid_file_reports_not_running() {
        let dir = TempDir::new().unwrap();
        let s = read_from(dir.path().join("nope.pid"));
        assert_eq!(s.pid, None);
        assert!(!s.is_running());
    }

    #[test]
    fn own_pid_file_reports_running() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pid");
        fs::write(&path, format!("{}\n", std::process::id())).unwrap();
        let s = read_from(path);
        assert!(s.alive);
        assert!(s.is_running());
        assert!(s.started_at.is_some());
    }

    #[test]
    fn dead_pid_reports_not_running() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pid");
        fs::write(&path, "2147483646\n").unwrap();
        let s = read_from(path);
        assert!(!s.alive);
        assert_eq!(s.pid, Some(2147483646));
    }
}
