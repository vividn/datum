//! Shared `--watch` polling helper for `dayview`, `nowview`, `now`, `view` and
//! a kit command, and the clear a watch redraws in place with on a terminal.
//!
//! Replaces the original "sleep 5 minutes between renders" approximation
//! with a fast poll on SQLite's `PRAGMA data_version`. data_version is
//! incremented whenever *another* connection commits, so when the user
//! runs `datum occur …` in a second shell, the watching renderer sees
//! the change within one poll interval (~500ms).
//!
//! The same loop optionally re-renders on a periodic-refresh tick so views
//! that show a "now" indicator (nowview's clock, dayview's now-line)
//! still advance even when the database is idle. Callers that visualize a
//! purely historical window pass `None` to skip the tick.

use std::io::IsTerminal;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use datum_core::db::Db;

/// Poll interval — kept short so the bead's "~1s after a write" target
/// holds with margin. SELECT on `PRAGMA data_version` is a single in-WAL
/// header read; 500ms is well below any cost concern.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Clear and home: what a watch prints before a frame on a terminal, so it
/// redraws in place. No alternate screen, so the scrollback stays.
pub const CLEAR: &str = "\x1b[2J\x1b[H";

/// [`CLEAR`] when stdout is a terminal, and nothing piped: there frames
/// follow one another, and a tsv, csv or json stream holds no escapes.
pub fn clear_on_terminal() -> &'static str {
    if std::io::stdout().is_terminal() {
        CLEAR
    } else {
        ""
    }
}

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
/// write (detected via `data_version`) and — if `periodic_refresh` is
/// `Some` — at most once per that duration so a "now" indicator keeps
/// advancing on an idle DB. `None` disables the time tick; the loop only
/// wakes on a real data change.
///
/// `render` is given the handle to read: a file opened `immutable` is never
/// read again by SQLite, so on its change the loop opens it again, read-only
/// as before, and renders from that (da-xwde8); a render a write met is
/// drawn again at the next poll, the file having changed. A reopen that
/// fails is tried at the next poll, and a second failure in a row ends the
/// watch.
///
/// Returns `Err` if `render` returns `Err`. The default Ctrl-C handler
/// (process exit) terminates the loop cleanly — no signal handler is
/// installed so the user gets the usual instant SIGINT teardown.
pub fn watch_loop<F>(db: &Db, periodic_refresh: Option<Duration>, mut render: F) -> Result<()>
where
    F: FnMut(&Db) -> Result<()>,
{
    render(db)?;
    let mut watched = Watched::new(db);
    let mut last_version = data_version(db)?;
    let mut last_render = Instant::now();
    loop {
        std::thread::sleep(POLL_INTERVAL);
        match watched.poll()? {
            Poll::Reopened => {
                render(watched.db())?;
                last_version = data_version(watched.db())?;
                last_render = Instant::now();
                continue;
            }
            Poll::Wait => continue,
            Poll::Same => {}
        }
        let held = watched.db();
        // If the pragma read fails transiently, fall back to the
        // periodic-refresh tick rather than aborting the watch session.
        let version = data_version(held).unwrap_or(last_version);
        let changed = version != last_version;
        let stale = periodic_refresh.is_some_and(|d| last_render.elapsed() >= d);
        if changed || stale {
            render(held)?;
            last_version = version;
            last_render = Instant::now();
        }
    }
}

/// What a watch's poll found of its file.
pub enum Poll {
    /// Read on with the handle held.
    Same,
    /// The file changed under an `immutable` open and was opened again:
    /// read the new handle, whose `data_version` counts afresh.
    Reopened,
    /// Opening it again failed: skip this poll and try at the next.
    Wait,
}

/// A watch's handle on its database (da-xwde8, da-cvvz7): a file opened
/// `immutable` is never read again by SQLite, so once it changes the watch
/// opens it again, read-only as before. A reopen that fails is tried at the
/// next poll, and a second failure in a row ends the watch.
pub struct Watched<'a> {
    first: &'a Db,
    reopened: Option<Db>,
    failed: bool,
}

impl<'a> Watched<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self {
            first: db,
            reopened: None,
            failed: false,
        }
    }

    /// The handle to read.
    pub fn db(&self) -> &Db {
        self.reopened.as_ref().unwrap_or(self.first)
    }

    pub fn poll(&mut self) -> Result<Poll> {
        let held = self.db();
        // Only a handle opened at a path says its file changed.
        let Some(path) = held.path().filter(|_| held.changed_while_read()) else {
            return Ok(Poll::Same);
        };
        match Db::open_to_read(&path.to_string_lossy()) {
            Ok(fresh) => {
                self.failed = false;
                self.reopened = Some(fresh);
                Ok(Poll::Reopened)
            }
            Err(e) if self.failed => Err(e).context("open the changed database again to read it"),
            Err(_) => {
                self.failed = true;
                Ok(Poll::Wait)
            }
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
        let res: Result<()> = watch_loop(&db, Some(Duration::from_secs(60)), |_| {
            anyhow::bail!("render exploded")
        });
        assert!(res.is_err());
    }

    /// `periodic_refresh = None` still propagates the first render's error
    /// — disabling the time tick must not change error semantics.
    #[test]
    fn watch_loop_propagates_render_error_with_no_periodic_refresh() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("watch_err_none.db");
        let db = Db::open(path.to_str().unwrap()).unwrap();
        let res: Result<()> = watch_loop(&db, None, |_| anyhow::bail!("render exploded"));
        assert!(res.is_err());
    }
}
