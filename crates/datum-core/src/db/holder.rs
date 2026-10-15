//! Who holds a database's write lock (da-hyem1).
//!
//! A write that waited out `busy_timeout` said only «database is locked»,
//! and nothing named the holder. So a datum connection that takes the lock
//! — at the `BEGIN IMMEDIATE` of `db::write`, or a deferred transaction's
//! first write — writes
//! its pid, the moment and its command to `<db>.writing`, and removes the
//! file before it lets the lock go, at commit or rollback. A write that
//! times out reads it ([`lock_holder`]). The hold is also timed, and rows
//! written counted per table, for a connection that asks to hear of long
//! ones ([`super::Db::on_long_hold`]).
//!
//! What it costs: a small file written and removed per transaction, never
//! synced; a callback per statement, which asks the transaction's state;
//! and an uncontended mutex per row written. Measured before and after
//! (#783): a `datum occur` on a 103 MB database, one transaction, 27.0 ms
//! against 28.3 ms median over 200 interleaved runs each on a box at load
//! 45-60, inside the spread (p10 24.6 / 24.5); the write and removal alone
//! are ~70 µs on ext4. The 2,000-op apply test, 7.42 s against 7.43 s.
//!
//! The lock is seen taken where a statement has run (SQLite's profile trace)
//! and left the connection in a write transaction (`sqlite3_txn_state`),
//! whatever its text: one hook on the connection, which no write path can
//! go round. Not on wasm, where the browser's own lock keeps tabs apart.

use std::ffi::{c_int, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{ffi, Connection};

type LongHold = Box<dyn Fn(Duration, &str) + Send>;
/// A hold in progress: when it began, and rows written per table.
type Hold = (Instant, Vec<(String, u64)>);

pub(crate) struct Watch {
    record: PathBuf,
    held: Mutex<Option<Hold>>,
    long: Mutex<Option<(Duration, LongHold)>>,
}

/// The file beside `db_path` naming the connection that holds its write lock.
fn record(db_path: &Path) -> PathBuf {
    let mut p = db_path.as_os_str().to_owned();
    p.push(".writing");
    p.into()
}

/// This process's command, its arguments joined by spaces: for a person to
/// read, not shell-quoted.
fn command() -> &'static str {
    static COMMAND: OnceLock<String> = OnceLock::new();
    COMMAND.get_or_init(|| {
        let mut args = std::env::args();
        let bin = args.next().unwrap_or_default();
        let bin = Path::new(&bin)
            .file_name()
            .map_or(bin.clone(), |n| n.to_string_lossy().into());
        std::iter::once(bin)
            .chain(args)
            .collect::<Vec<_>>()
            .join(" ")
    })
}

impl Watch {
    fn begin(&self) {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        if held.is_some() {
            return;
        }
        let since = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let line = format!("{} {} {}", std::process::id(), since.as_millis(), command());
        let _ = std::fs::write(&self.record, line);
        *held = Some((Instant::now(), Vec::new()));
    }

    fn wrote(&self, table: &str) {
        if let Some((_, rows)) = self.held.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            match rows.iter_mut().find(|(t, _)| t == table) {
                Some((_, n)) => *n += 1,
                None => rows.push((table.to_string(), 1)),
            }
        }
    }

    fn end(&self) {
        let Some((since, mut rows)) = self.held.lock().unwrap_or_else(|e| e.into_inner()).take()
        else {
            return;
        };
        let _ = std::fs::remove_file(&self.record);
        let held = since.elapsed();
        if let Some((after, log)) = &*self.long.lock().unwrap_or_else(|e| e.into_inner()) {
            if held >= *after {
                rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
                let rows: Vec<String> = rows.iter().map(|(t, n)| format!("{t} {n}")).collect();
                log(
                    held,
                    &if rows.is_empty() {
                        "no rows written".into()
                    } else {
                        rows.join(", ")
                    },
                );
            }
        }
    }

    pub(crate) fn on_long_hold(&self, after: Duration, log: LongHold) {
        *self.long.lock().unwrap_or_else(|e| e.into_inner()) = Some((after, log));
    }
}

/// SQLite's profile trace: a statement has run. One that left the
/// connection in a write transaction took the write lock, or holds it.
unsafe extern "C" fn profiled(
    _: c_uint,
    ctx: *mut c_void,
    stmt: *mut c_void,
    _: *mut c_void,
) -> c_int {
    let db = ffi::sqlite3_db_handle(stmt.cast::<ffi::sqlite3_stmt>());
    if ffi::sqlite3_txn_state(db, std::ptr::null()) == ffi::SQLITE_TXN_WRITE {
        (*ctx.cast::<Watch>()).begin();
    }
    0
}

/// Watch `conn`'s holds of the write lock on the file at `db_path`. The
/// connection must close before the returned `Watch` drops: it holds a
/// pointer to it.
pub(crate) fn watch(conn: &Connection, db_path: &Path) -> Option<Arc<Watch>> {
    if cfg!(target_family = "wasm") {
        return None;
    }
    let watch = Arc::new(Watch {
        record: record(db_path),
        held: Mutex::new(None),
        long: Mutex::new(None),
    });
    // SAFETY: SQLite calls `profiled` only from a step on this connection,
    // and the hooks below each own a clone of the Arc until rusqlite removes
    // them, at close, before `sqlite3_close`: the pointer is live for every
    // call, whatever drops first.
    unsafe {
        let ctx = Arc::as_ptr(&watch) as *mut c_void;
        ffi::sqlite3_trace_v2(
            conn.handle(),
            ffi::SQLITE_TRACE_PROFILE as c_uint,
            Some(profiled),
            ctx,
        );
    }
    let w = watch.clone();
    conn.update_hook(Some(move |_, _: &str, table: &str, _| w.wrote(table)));
    let w = watch.clone();
    conn.commit_hook(Some(move || {
        w.end();
        false
    }));
    let w = watch.clone();
    conn.rollback_hook(Some(move || w.end()));
    Some(watch)
}

/// Who holds the write lock on the database at `db_path`, as a datum
/// connection recorded it: «`datum sync run` (pid 1234) for 7.2 s». `None`
/// when no datum transaction holds it, or the process that recorded one is
/// gone — which is checked on Linux alone. Elsewhere a process killed
/// mid-transaction leaves its record until the next datum transaction on the
/// database replaces and removes it, and its «for» says how old it is.
pub fn lock_holder(db_path: &Path) -> Option<String> {
    let line = std::fs::read_to_string(record(db_path)).ok()?;
    let mut parts = line.splitn(3, ' ');
    let pid: u32 = parts.next()?.parse().ok()?;
    let since: u64 = parts.next()?.parse().ok()?;
    let command = parts.next().unwrap_or("?");
    if cfg!(target_os = "linux") && !Path::new(&format!("/proc/{pid}")).exists() {
        return None;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let held = now
        .saturating_sub(Duration::from_millis(since))
        .as_secs_f64();
    Some(format!("`{command}` (pid {pid}) for {held:.1} s"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::error::DatumError;

    /// A handle holding the lock is named by what it recorded, for as long as
    /// it holds it, and not once it lets go — by commit or by rollback.
    #[test]
    fn the_holder_is_named_while_it_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("d.db");
        let holder = Db::open(path.to_str().unwrap()).unwrap();
        holder.conn().execute_batch("CREATE TABLE t(x)").unwrap();
        let waiter = Db::open(path.to_str().unwrap()).unwrap();
        waiter.conn().busy_timeout(Duration::ZERO).unwrap();
        assert_eq!(lock_holder(&path), None);

        for commit in [true, false] {
            let held = holder.write_with(|tx| {
                tx.execute("INSERT INTO t VALUES (1)", [])?;
                let e = waiter
                    .conn()
                    .execute("INSERT INTO t VALUES (2)", [])
                    .unwrap_err();
                assert!(e.to_string().contains("database is locked"), "{e}");
                let named = lock_holder(&path).unwrap();
                let pid = format!("(pid {}) for 0.", std::process::id());
                assert!(
                    named.starts_with(&format!("`{}", command())) && named.contains(&pid),
                    "{named}"
                );
                match commit {
                    true => Ok(()),
                    false => Err(DatumError::NotFound("rolled back".into())),
                }
            });
            assert_eq!(held.is_ok(), commit);
            assert_eq!(lock_holder(&path), None, "after commit: {commit}");
            assert!(!record(&path).exists());
        }
        // A read holds no write lock, and a write outside a transaction
        // holds it for its one statement: neither is recorded.
        holder
            .read(|_| {
                assert_eq!(lock_holder(&path), None);
                Ok::<_, DatumError>(())
            })
            .unwrap();
        holder
            .conn()
            .execute("INSERT INTO t VALUES (3)", [])
            .unwrap();
        assert!(!record(&path).exists());
    }

    /// A hold at least as long as asked for is reported with its rows per
    /// table, most first; a shorter one is not.
    #[test]
    fn a_long_hold_is_reported_with_what_it_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path().join("d.db").to_str().unwrap()).unwrap();
        db.conn()
            .execute_batch("CREATE TABLE t(x); CREATE TABLE u(x)")
            .unwrap();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let h = heard.clone();
        db.on_long_hold(Duration::from_millis(50), move |held, what| {
            h.lock().unwrap().push((held, what.to_string()))
        });
        db.write_with(|tx| {
            tx.execute_batch("INSERT INTO t VALUES (1)")
                .map_err(DatumError::from)
        })
        .unwrap();
        assert!(heard.lock().unwrap().is_empty());

        db.write_with(|tx| {
            tx.execute_batch("INSERT INTO u VALUES (1); INSERT INTO t VALUES (1), (2)")?;
            std::thread::sleep(Duration::from_millis(60));
            Ok::<_, DatumError>(())
        })
        .unwrap();
        let heard = heard.lock().unwrap();
        assert_eq!(heard.len(), 1);
        assert!(heard[0].0 >= Duration::from_millis(50));
        assert_eq!(heard[0].1, "t 2, u 1");
    }
}
