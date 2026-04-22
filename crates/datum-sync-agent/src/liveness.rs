//! PID-file based single-instance guard.
//!
//! The daemon writes its PID to `$XDG_RUNTIME_DIR/datum-sync-agent.pid` on
//! startup and removes it on graceful shutdown. A second instance trying
//! to acquire the same file first checks whether the stored PID is still
//! alive:
//!
//! - Alive → bail. "Already running".
//! - Dead (leftover file from a crash) → overwrite with our PID and proceed.
//!
//! The alive check uses `kill(pid, 0)` on unix: returns 0 if the target
//! exists (whether we can signal it or not), ESRCH if not.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Default pid-file path:
/// - `$XDG_RUNTIME_DIR/datum-sync-agent.pid` if set
/// - `/tmp/datum-sync-agent-<uid>.pid` otherwise (matches systemd fallback)
pub fn default_pid_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("datum-sync-agent.pid");
        }
    }
    // Fallback: /tmp, keyed by uid so two users on one box don't collide.
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

/// Owns the pid-file for the lifetime of the daemon process. Removes the
/// file on drop so a clean shutdown leaves no stale state.
#[derive(Debug)]
pub struct PidFile {
    path: PathBuf,
    /// `true` once we've successfully written our pid. Used to decide
    /// whether to clean up the file on drop — we don't want to delete a
    /// file that another instance owns.
    owned: bool,
}

impl PidFile {
    /// Acquire the pid-file at `path`, or fail if another live process
    /// already owns it.
    ///
    /// Leftover files from a crashed daemon (pid no longer alive) are
    /// treated as abandoned and reclaimed with our pid.
    pub fn acquire<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create pid-file dir {parent:?}"))?;
            }
        }

        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(pid) = contents.trim().parse::<i32>() {
                if pid > 0 && is_pid_alive(pid) {
                    anyhow::bail!(
                        "datum-sync-agent already running (pid {pid}, pid-file {})",
                        path.display()
                    );
                }
            }
        }

        let mut f = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("open pid-file {}", path.display()))?;
        writeln!(f, "{}", std::process::id())
            .with_context(|| format!("write pid-file {}", path.display()))?;
        Ok(Self { path, owned: true })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        if self.owned {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// True if `pid` is alive. On unix: `kill(pid, 0)` returns 0 if the
/// target process exists, -1 otherwise. We treat any non-zero return as
/// "not alive" — the daemon always runs as the user that owns the
/// data dir, so the cross-user EPERM edge case doesn't apply in practice.
pub fn is_pid_alive(pid: i32) -> bool {
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

/// Read the pid recorded in `path`, returning `Ok(None)` when the file is
/// absent or unparseable. Used by `datum sync status` for the daemon
/// liveness readout — the file being there doesn't prove the process is
/// alive, so callers chain this with [`is_pid_alive`].
pub fn read_pid_file<P: AsRef<Path>>(path: P) -> io::Result<Option<i32>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents.trim().parse::<i32>().ok().filter(|p| *p > 0)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// The canonical `pid_file_prevents_double_start` acceptance test:
    /// a live pid-file must refuse a second acquire.
    #[test]
    fn pid_file_prevents_double_start() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pid");

        let _first = PidFile::acquire(&path).unwrap();
        // Our own pid IS alive → second acquire must bail.
        let err = PidFile::acquire(&path).expect_err("expected double-start bail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("already running"),
            "unexpected error: {msg}",
        );
    }

    /// A pid-file left behind by a crashed daemon (PID no longer alive)
    /// must not block a fresh start.
    #[test]
    fn stale_pid_file_is_reclaimed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pid");
        // Pick a pid extremely unlikely to be live (INT_MAX) — if it
        // happens to exist, the test gets a false positive but not a
        // false negative; the failure mode is "reclaim fails".
        fs::write(&path, "2147483646\n").unwrap();
        let f = PidFile::acquire(&path).expect("stale pid should reclaim");
        assert!(f.path().exists());
    }

    #[test]
    fn drop_removes_pid_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pid");
        {
            let _f = PidFile::acquire(&path).unwrap();
            assert!(path.exists());
        }
        assert!(!path.exists());
    }

    #[test]
    fn read_pid_file_round_trips() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pid");
        assert_eq!(read_pid_file(&path).unwrap(), None);
        fs::write(&path, "4242\n").unwrap();
        assert_eq!(read_pid_file(&path).unwrap(), Some(4242));
        fs::write(&path, "garbage").unwrap();
        assert_eq!(read_pid_file(&path).unwrap(), None);
    }
}
