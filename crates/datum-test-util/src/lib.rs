//! Shared harness for tests that mutate process-global state — env vars
//! (`NO_COLOR`, `FORCE_COLOR`, `DATUM_HOME`, `TZ`, …) and other
//! per-process globals.
//!
//! `cargo test` runs the tests inside one binary as parallel *threads* of
//! a single process, so any test that calls `set_var`/`remove_var` races
//! every concurrent reader of that var. Before this crate existed the
//! workspace had grown 15 hand-rolled `static …: Mutex<()>` locks and
//! half a dozen copies of an RAII restore guard, diverging in quality
//! (some poison-tolerant, some restoring prior values, some neither) —
//! and per-module locks within one binary don't serialize against each
//! *other* (da-3ywd). This crate provides the one canonical copy:
//!
//! - [`env_lock`] — the process-wide, poison-tolerant lock. One static
//!   per test binary (= per process), so every module in that binary
//!   serializes through the same lock. Separate integration-test
//!   binaries are separate processes with separate envs; each gets its
//!   own instance, which is exactly right.
//! - [`ScopedEnv`] — RAII override that restores the prior value (or
//!   absence) on drop. Deliberately does NOT acquire the lock, so many
//!   guards can coexist inside one locked scope without deadlocking on
//!   the non-reentrant mutex. Acquire [`env_lock`] first.
//! - [`no_color_scope`] / [`color_scope`] and the closure forms
//!   [`with_no_color`] / [`with_color`] — lock + `NO_COLOR`/`FORCE_COLOR`
//!   override in one guard, for the very common color-gate toggle.
//! - [`TmpDb`] — a database path in a directory of its own, removed on drop.

use std::ffi::{OsStr, OsString};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the process-wide env lock, held for the duration of the guard.
///
/// Poison-tolerant: a panicking test does not cascade into "lock
/// poisoned" failures across the rest of the suite.
pub fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// RAII env-var override: sets (or removes) on construction, restores the
/// prior value — including prior *absence* — on drop.
///
/// Does not lock; hold [`env_lock`] across the guard's whole lifetime.
/// Drop guards before releasing the lock (declaration order does this
/// for struct fields; statement order does it in a function body, since
/// locals drop in reverse).
pub struct ScopedEnv {
    key: &'static str,
    prior: Option<OsString>,
}

impl ScopedEnv {
    /// Override `key` with `value`, restoring the prior state on drop.
    pub fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
        let prior = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, prior }
    }

    /// Remove `key`, restoring the prior state on drop.
    pub fn remove(key: &'static str) -> Self {
        let prior = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, prior }
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        match self.prior.take() {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

/// Composite guard: the process-wide lock plus `FORCE_COLOR`/`NO_COLOR`
/// overrides. Field order matters — env restores before the lock drops.
pub struct ColorScope {
    _force: ScopedEnv,
    _no: ScopedEnv,
    _lock: MutexGuard<'static, ()>,
}

/// Force colors OFF (`NO_COLOR=1`, `FORCE_COLOR` unset) so assertions don't
/// depend on whether the test binary happens to have a TTY.
pub fn no_color_scope() -> ColorScope {
    let lock = env_lock();
    ColorScope {
        _force: ScopedEnv::remove("FORCE_COLOR"),
        _no: ScopedEnv::set("NO_COLOR", "1"),
        _lock: lock,
    }
}

/// Force colors ON (`FORCE_COLOR=1`, `NO_COLOR` unset).
pub fn color_scope() -> ColorScope {
    let lock = env_lock();
    ColorScope {
        _force: ScopedEnv::set("FORCE_COLOR", "1"),
        _no: ScopedEnv::remove("NO_COLOR"),
        _lock: lock,
    }
}

/// Run `f` with colors forced OFF. See [`no_color_scope`].
pub fn with_no_color<T>(f: impl FnOnce() -> T) -> T {
    let _scope = no_color_scope();
    f()
}

/// Run `f` with colors forced ON. See [`color_scope`].
pub fn with_color<T>(f: impl FnOnce() -> T) -> T {
    let _scope = color_scope();
    f()
}

/// A database path in a fresh directory under the temp dir, which is removed
/// with everything datum wrote beside the file (`-wal`, `-shm`, `.spool`, …)
/// when the guard drops — on a failing test too. The temp dir is a tmpfs on
/// the town's host, so a test that leaks fills its RAM and swap (da-yqc4v).
pub struct TmpDb {
    dir: PathBuf,
    path: PathBuf,
}

impl TmpDb {
    /// `prefix` names the directory, so a leftover says which test made it.
    pub fn new(prefix: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir); // a dead run's, with our pid
        std::fs::create_dir(&dir).expect("create the test's temp dir");
        let path = dir.join(format!("{prefix}.db"));
        Self { dir, path }
    }
}

impl Deref for TmpDb {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.path
    }
}

impl AsRef<Path> for TmpDb {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<OsStr> for TmpDb {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}

impl Drop for TmpDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_db_removes_its_directory_and_what_is_beside_the_file() {
        let db = TmpDb::new("datum-test-util-probe");
        let dir = db.parent().unwrap().to_path_buf();
        std::fs::write(&*db, "").unwrap();
        std::fs::write(db.with_extension("db-wal"), "").unwrap();
        drop(db);
        assert!(!dir.exists(), "{} survived its guard", dir.display());
    }

    #[test]
    fn scoped_env_restores_prior_value_and_absence() {
        let _lock = env_lock();
        std::env::set_var("DATUM_TEST_UTIL_PROBE", "before");
        {
            let _g = ScopedEnv::set("DATUM_TEST_UTIL_PROBE", "during");
            assert_eq!(
                std::env::var("DATUM_TEST_UTIL_PROBE").as_deref(),
                Ok("during")
            );
        }
        assert_eq!(
            std::env::var("DATUM_TEST_UTIL_PROBE").as_deref(),
            Ok("before")
        );
        std::env::remove_var("DATUM_TEST_UTIL_PROBE");
        {
            let _g = ScopedEnv::set("DATUM_TEST_UTIL_PROBE", "during");
        }
        assert!(std::env::var_os("DATUM_TEST_UTIL_PROBE").is_none());
        {
            let _g = ScopedEnv::remove("DATUM_TEST_UTIL_PROBE");
            assert!(std::env::var_os("DATUM_TEST_UTIL_PROBE").is_none());
        }
        assert!(std::env::var_os("DATUM_TEST_UTIL_PROBE").is_none());
    }

    #[test]
    fn color_scopes_toggle_and_restore() {
        let prior_force = std::env::var_os("FORCE_COLOR");
        let prior_no = std::env::var_os("NO_COLOR");
        with_no_color(|| {
            assert!(std::env::var_os("FORCE_COLOR").is_none());
            assert_eq!(std::env::var("NO_COLOR").as_deref(), Ok("1"));
        });
        with_color(|| {
            assert_eq!(std::env::var("FORCE_COLOR").as_deref(), Ok("1"));
            assert!(std::env::var_os("NO_COLOR").is_none());
        });
        assert_eq!(std::env::var_os("FORCE_COLOR"), prior_force);
        assert_eq!(std::env::var_os("NO_COLOR"), prior_no);
    }
}
