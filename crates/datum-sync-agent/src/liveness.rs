//! PID-file based single-instance guard.
//!
//! The daemon takes an exclusive OS lock ([`File::try_lock`] — `flock` on
//! unix) on `$XDG_RUNTIME_DIR/datum-sync-agent.pid` at startup and writes
//! its PID into the locked file for the `datum sync status` readout. The
//! lock — not PID liveness — is the source of truth for "already running":
//!
//! - Atomic: no check-then-act window, so two simultaneous starts can't
//!   both win (the old read/`kill(pid, 0)`/rewrite dance could).
//! - Auto-released on process death: a crashed daemon leaves a stale file
//!   but no lock, so reclaim needs no liveness probe and a recycled PID
//!   can never read as a live daemon.
//!
//! `kill(pid, 0)` survives only in [`is_pid_alive`], a best-effort helper
//! for the status readout.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Default pid-file path:
/// - `$XDG_RUNTIME_DIR/datum-sync-agent.pid` if set
/// - `/tmp/datum-sync-agent-<uid>.pid` otherwise (matches systemd fallback)
///
/// The canonical definition lives in `datum-commands` (the `datum sync
/// status` readout reads the same file); re-exported here so the writer
/// and reader of the pid-file can never drift.
pub use datum_commands::commands::sync::daemon_status::default_pid_path;

/// Owns the pid-file for the lifetime of the daemon process. Removes the
/// file on drop so a clean shutdown leaves no stale state.
#[derive(Debug)]
pub struct PidFile {
    path: PathBuf,
    /// Open handle holding the exclusive lock. The kernel releases the
    /// lock when the handle closes — including on crash — which is what
    /// makes stale-file reclaim free. Declared after `path` so it drops
    /// (= unlocks) after the unlink in `Drop::drop`; see the inode
    /// re-check in [`PidFile::acquire`] for why that order matters.
    _lock: File,
}

impl PidFile {
    /// Acquire the pid-file at `path`, or fail if another live process
    /// already holds its lock.
    ///
    /// Leftover files from a crashed daemon carry no lock and are
    /// reclaimed with our pid — even if the recorded pid has been
    /// recycled to an unrelated live process.
    pub fn acquire<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create pid-file dir {parent:?}"))?;
            }
        }

        // Bounded retry: a daemon shutting down unlinks the file between
        // our open and our lock, leaving us holding a lock on a deleted
        // inode that guards nothing — re-check and reopen the fresh path.
        for _ in 0..5 {
            let file = fs::OpenOptions::new()
                .create(true)
                // No truncate here: we must not clobber a live holder's pid.
                // The winner truncates via `set_len(0)` after taking the lock.
                .truncate(false)
                .read(true)
                .write(true)
                .open(&path)
                .with_context(|| format!("open pid-file {}", path.display()))?;

            match file.try_lock() {
                Ok(()) => {}
                Err(fs::TryLockError::WouldBlock) => {
                    let holder = read_pid_file(&path)
                        .ok()
                        .flatten()
                        .map_or_else(String::new, |pid| format!("pid {pid}, "));
                    anyhow::bail!(
                        "datum-sync-agent already running ({holder}pid-file {})",
                        path.display()
                    );
                }
                Err(fs::TryLockError::Error(e)) => {
                    return Err(e).with_context(|| format!("lock pid-file {}", path.display()));
                }
            }

            if !path_is_same_file(&file, &path)? {
                continue; // raced an unlink — locked inode is orphaned
            }

            file.set_len(0)
                .and_then(|()| writeln!(&file, "{}", std::process::id()))
                .with_context(|| format!("write pid-file {}", path.display()))?;
            return Ok(Self { path, _lock: file });
        }

        anyhow::bail!(
            "could not acquire pid-file {} (kept racing concurrent instances)",
            path.display()
        )
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        // Unlink while still holding the lock (`_lock` closes after this
        // body): any racer that already opened this inode will see the
        // path change under it and retry on the fresh file.
        let _ = fs::remove_file(&self.path);
    }
}

/// True if `path` still names the same inode as the open `file` — i.e.
/// nobody unlinked/replaced the pid-file between our open and lock.
#[cfg(unix)]
fn path_is_same_file(file: &File, path: &Path) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let locked = file.metadata().context("stat locked pid-file")?;
    match fs::metadata(path) {
        Ok(m) => Ok(m.dev() == locked.dev() && m.ino() == locked.ino()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).context("re-stat pid-file path"),
    }
}

#[cfg(not(unix))]
fn path_is_same_file(_file: &File, _path: &Path) -> Result<bool> {
    // Windows file locks block the unlink itself, so the race can't occur.
    Ok(true)
}

/// True if `pid` is alive. On unix: `kill(pid, 0)` returns 0 if the
/// target process exists, -1 otherwise. We treat any non-zero return as
/// "not alive" — the daemon always runs as the user that owns the
/// data dir, so the cross-user EPERM edge case doesn't apply in practice.
///
/// Best-effort `datum sync status` readout only — [`PidFile::acquire`]
/// decides "already running" by the file lock, never by this probe
/// (which a recycled pid can fool).
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
        assert!(msg.contains("already running"), "unexpected error: {msg}",);
    }

    /// A pid-file left behind by a crashed daemon (PID no longer alive)
    /// must not block a fresh start.
    #[test]
    fn stale_pid_file_is_reclaimed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pid");
        fs::write(&path, "2147483646\n").unwrap();
        let f = PidFile::acquire(&path).expect("stale pid should reclaim");
        assert!(f.path().exists());
    }

    /// PID-reuse hardening: an unlocked leftover file whose recorded pid
    /// happens to be a LIVE (unrelated) process must still be reclaimed —
    /// the lock, not pid liveness, decides. Our own pid is the one pid
    /// guaranteed alive.
    #[test]
    fn unlocked_file_with_live_pid_is_reclaimed() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pid");
        fs::write(&path, format!("{}\n", std::process::id())).unwrap();
        PidFile::acquire(&path).expect("no lock held → reclaim despite live pid");
    }

    /// TOCTOU regression test: many simultaneous acquires on the same
    /// path must produce exactly one winner. The old read→alive-check→
    /// truncate-write sequence let several racers all pass the check and
    /// all believe they owned the file.
    #[test]
    fn concurrent_acquire_single_winner() {
        use std::sync::{Arc, Barrier};

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("pid");
        const N: usize = 8;
        let barrier = Arc::new(Barrier::new(N));
        let handles: Vec<_> = (0..N)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    PidFile::acquire(&path).ok()
                })
            })
            .collect();
        // Collect guards before dropping any, so losers can't sneak in
        // after the winner releases.
        let winners: Vec<PidFile> = handles
            .into_iter()
            .filter_map(|h| h.join().unwrap())
            .collect();
        assert_eq!(winners.len(), 1, "exactly one acquire must win");
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
