//! Shared `--watch` polling helper for `dayview` and `nowview`.
//!
//! Replaces the original "sleep 5 minutes between renders" approximation
//! with a fast poll on SQLite's `PRAGMA data_version`. data_version is
//! incremented whenever *another* connection commits, so when the user
//! runs `datum occur …` in a second shell, the watching renderer sees
//! the change within one poll interval (~500ms).
//!
//! The same loop also re-renders on a periodic-refresh tick so views
//! that show a "now" indicator (nowview's clock, dayview's now-line)
//! still advance even when the database is idle.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use datum_core::db::Db;

/// Poll interval — kept short so the bead's "~1s after a write" target
/// holds with margin. SELECT on `PRAGMA data_version` is a single in-WAL
/// header read; 500ms is well below any cost concern.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Read SQLite's `data_version`. Per the SQLite docs this counter
/// changes when *any other connection* modifies the database — exactly
/// the signal `--watch` needs (the writer is `datum occur` running in a
/// separate process, hence a separate connection).
pub fn data_version(db: &Db) -> Result<i64> {
    db.conn()
        .pragma_query_value(None, "data_version", |r| r.get::<_, i64>(0))
        .context("read PRAGMA data_version")
}

/// Run `render` immediately, then loop: re-render on every external DB
/// write (detected via `data_version`) or every `periodic_refresh`,
/// whichever comes first.
///
/// Returns `Err` if `render` returns `Err`. The default Ctrl-C handler
/// (process exit) terminates the loop cleanly — no signal handler is
/// installed so the user gets the usual instant SIGINT teardown.
pub fn watch_loop<F>(db: &Db, periodic_refresh: Duration, mut render: F) -> Result<()>
where
    F: FnMut() -> Result<()>,
{
    render()?;
    let mut last_version = data_version(db)?;
    let mut last_render = Instant::now();
    loop {
        std::thread::sleep(POLL_INTERVAL);
        // If the pragma read fails transiently, fall back to the
        // periodic-refresh tick rather than aborting the watch session.
        let version = data_version(db).unwrap_or(last_version);
        let changed = version != last_version;
        let stale = last_render.elapsed() >= periodic_refresh;
        if changed || stale {
            render()?;
            last_version = version;
            last_render = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use datum_core::entry::{Entry, Verb};
    use datum_core::time::DatumTime;
    use serde_json::Value as JsonValue;
    use tempfile::TempDir;

    /// External-connection writes bump `data_version`. The render thread
    /// (this connection) sees the change without committing anything of
    /// its own.
    #[test]
    fn data_version_advances_after_external_write() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("watch_test.db");
        let path_str = path.to_str().unwrap();

        // Open two independent connections, mirroring "watcher reads
        // while another `datum` process writes".
        let reader = Db::open(path_str).unwrap();
        let writer = Db::open(path_str).unwrap();

        let v0 = data_version(&reader).unwrap();
        let utc = Utc::now();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        writer
            .insert_entry(&Entry::new(
                "watch_field",
                Verb::Point,
                Some(JsonValue::String("x".into())),
                None,
                None,
                Some(dt),
                None,
                "test",
            ))
            .unwrap();
        let v1 = data_version(&reader).unwrap();
        assert_ne!(v0, v1, "data_version must advance after external write");
    }

    /// Same-connection writes don't bump our own data_version (SQLite
    /// docs). That's fine — `--watch` is read-only, the writer is always
    /// a different process. The test pins the assumption.
    #[test]
    fn data_version_stable_for_own_writes() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("watch_self.db");
        let db = Db::open(path.to_str().unwrap()).unwrap();
        let v0 = data_version(&db).unwrap();
        let utc = Utc::now();
        let dt = DatumTime::from_utc_and_tz(utc, "UTC").unwrap();
        db.insert_entry(&Entry::new(
            "self_write",
            Verb::Point,
            None,
            None,
            None,
            Some(dt),
            None,
            "test",
        ))
        .unwrap();
        let v1 = data_version(&db).unwrap();
        assert_eq!(v0, v1);
    }

    /// `watch_loop` exits via the render closure when it returns Err —
    /// covers the "render error stops the loop" path without needing a
    /// real signal.
    #[test]
    fn watch_loop_propagates_render_error_on_first_call() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("watch_err.db");
        let db = Db::open(path.to_str().unwrap()).unwrap();
        let res: Result<()> = watch_loop(&db, Duration::from_secs(60), || {
            anyhow::bail!("render exploded")
        });
        assert!(res.is_err());
    }
}
