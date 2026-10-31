//! The one place a transaction opens (da-8i7f0).
//!
//! A write takes SQLite's write lock, and while it holds it every other
//! writer waits: the sync daemon, another command, a tab. So the lock is
//! held for the writes and nothing else. What a write depends on is read
//! and computed first, on a [`View`]: a read transaction, which under WAL
//! takes no lock a writer waits on. Then [`Db::write`] opens the IMMEDIATE
//! transaction, checks the database is still as the view read it
//! (`PRAGMA data_version`, which moves when another connection commits),
//! runs the batch's statements and commits. A batch computed on a database
//! that moved since is stale, and [`Db::compute_then_write`] computes it
//! again. After [`TRIES`] stale tries the computation runs inside the
//! transaction, still on a read-only view, so a write beside a busy writer
//! still lands.
//!
//! The `_with` forms run the caller's closure under the lock: the writes of
//! sites not yet reduced to a [`WriteBatch`], whose closures still read the
//! rows they write as they write them. A test counts them, and the count
//! only goes down (`unbatched_writes_only_shrink`).
//!
//! Every transaction opener of rusqlite is a disallowed method
//! (`clippy.toml`), allowed in this module alone, and a raw `BEGIN` in the
//! crates' sources is refused by `no_raw_begin_outside_db_write`.

use rusqlite::types::Value;
use rusqlite::{Connection, Transaction, TransactionBehavior};

use super::Db;
use crate::error::{DatumError, Result};

/// How many times a batch is computed outside the lock before it is
/// computed under it.
pub const TRIES: usize = 3;

/// The database as a computation read it: its `PRAGMA data_version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen(i64);

/// A read-only snapshot of the database: one read transaction, with
/// `query_only` on, so a write through it is an error.
pub struct View<'a> {
    conn: &'a Connection,
    seen: Seen,
}

impl<'a> View<'a> {
    /// The connection, to read through.
    pub fn conn(&self) -> &'a Connection {
        self.conn
    }

    /// What this view read: a batch computed on it is [`WriteBatch::on`] it.
    pub fn seen(&self) -> Seen {
        self.seen
    }
}

/// The statements one write runs, in order, and what they were computed on.
#[derive(Debug, Default)]
pub struct WriteBatch {
    seen: Option<Seen>,
    statements: Vec<(String, Vec<Value>)>,
}

impl WriteBatch {
    /// A batch that lands only on the database `seen` read.
    pub fn on(seen: Seen) -> Self {
        WriteBatch {
            seen: Some(seen),
            statements: Vec::new(),
        }
    }

    /// Add a statement.
    pub fn push(&mut self, sql: impl Into<String>, params: Vec<Value>) {
        self.statements.push((sql.into(), params));
    }

    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }
}

fn data_version(conn: &Connection) -> Result<Seen> {
    Ok(Seen(
        conn.query_row("PRAGMA data_version", [], |r| r.get(0))?,
    ))
}

#[allow(clippy::disallowed_methods)] // the module that opens them
fn begin(conn: &Connection, behavior: TransactionBehavior) -> Result<Transaction<'_>> {
    Ok(Transaction::new_unchecked(conn, behavior)?)
}

/// `f` on a read-only view of what `conn`'s open transaction sees.
fn viewing<T, E: From<DatumError>>(
    conn: &Connection,
    f: impl FnOnce(&View) -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    let seen = data_version(conn)?;
    conn.execute_batch("PRAGMA query_only = ON")
        .map_err(DatumError::from)?;
    let out = f(&View { conn, seen });
    conn.execute_batch("PRAGMA query_only = OFF")
        .map_err(DatumError::from)?;
    out
}

fn apply(tx: &Connection, batch: &WriteBatch) -> Result<()> {
    for (sql, params) in &batch.statements {
        tx.prepare_cached(sql)?
            .execute(rusqlite::params_from_iter(params))?;
    }
    Ok(())
}

/// One transaction over a whole-database change, by design: a migration,
/// whose steps are all-or-nothing, and `db purge`, whose drops and new
/// schema are one (da-dfmg8). Nothing else may use it.
pub(crate) fn whole<T>(conn: &Connection, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let tx = begin(conn, TransactionBehavior::Immediate)?;
    let out = f(&tx)?;
    tx.commit()?;
    Ok(out)
}

impl Db {
    /// `f` on one read-only snapshot, which holds no lock a writer waits on.
    pub fn read<T, E: From<DatumError>>(
        &self,
        f: impl FnOnce(&View) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let tx = begin(&self.conn, TransactionBehavior::Deferred)?;
        let out = viewing(&tx, f)?;
        tx.commit().map_err(DatumError::from)?;
        Ok(out)
    }

    /// Land `batch`: false, writing nothing, when the database moved since
    /// the view it was computed on.
    pub fn write(&self, batch: &WriteBatch) -> Result<bool> {
        if batch.is_empty() {
            return Ok(true);
        }
        let tx = begin(&self.conn, TransactionBehavior::Immediate)?;
        if let Some(seen) = batch.seen {
            if data_version(&tx)? != seen {
                return Ok(false);
            }
        }
        apply(&tx, batch)?;
        tx.commit()?;
        Ok(true)
    }

    /// Compute a batch on a view and land it, computing it again while the
    /// database moves under it ([`TRIES`]).
    pub fn compute_then_write<T, E: From<DatumError>>(
        &self,
        mut compute: impl FnMut(&View) -> std::result::Result<(WriteBatch, T), E>,
    ) -> std::result::Result<T, E> {
        for _ in 1..TRIES {
            let (batch, out) = self.read(&mut compute)?;
            if self.write(&batch)? {
                return Ok(out);
            }
        }
        // Computed under the lock, on the database the lock holds: the
        // batch's `seen` is this view's, and nothing can move it.
        let tx = begin(&self.conn, TransactionBehavior::Immediate)?;
        let (batch, out) = viewing(&tx, compute)?;
        apply(&tx, &batch)?;
        tx.commit().map_err(DatumError::from)?;
        Ok(out)
    }

    /// UNBATCHED: [`Db::compute_then_write`] whose write is a closure, run
    /// under the lock with what `compute` read outside it.
    pub fn compute_then_write_with<P, T, E: From<DatumError>>(
        &self,
        compute: impl FnMut(&View) -> std::result::Result<P, E>,
        write: impl FnOnce(&Connection, P) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        self.locked(true, compute, write)
    }

    /// UNBATCHED: [`Db::compute_then_write_with`] rolled back — a dry run,
    /// whose counts are the real run's by construction.
    pub fn rehearse_with<P, T, E: From<DatumError>>(
        &self,
        compute: impl FnMut(&View) -> std::result::Result<P, E>,
        write: impl FnOnce(&Connection, P) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        self.locked(false, compute, write)
    }

    /// UNBATCHED: one write that a dry run rolls back: `commit` picks
    /// [`Db::compute_then_write_with`] or [`Db::rehearse_with`].
    pub fn write_or_rehearse_with<P, T, E: From<DatumError>>(
        &self,
        commit: bool,
        compute: impl FnMut(&View) -> std::result::Result<P, E>,
        write: impl FnOnce(&Connection, P) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        self.locked(commit, compute, write)
    }

    /// UNBATCHED: `f` under the lock, for a write that reads nothing but
    /// the rows it writes.
    pub fn write_with<T, E: From<DatumError>>(
        &self,
        f: impl FnOnce(&Connection) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let tx = begin(&self.conn, TransactionBehavior::Immediate)?;
        crate::integrity::forget_moved();
        let out = f(&tx)?;
        crate::integrity::detect_moved(&tx, crate::integrity::DetectedBy::LocalValidator)?;
        tx.commit().map_err(DatumError::from)?;
        Ok(out)
    }

    /// Take the write lock and let it go at once: whether a write would
    /// land now ([`Db::can_write`]).
    pub(crate) fn take_the_lock(&self) -> Result<()> {
        begin(&self.conn, TransactionBehavior::Immediate).map(drop)
    }

    fn locked<P, T, E: From<DatumError>>(
        &self,
        commit: bool,
        mut compute: impl FnMut(&View) -> std::result::Result<P, E>,
        write: impl FnOnce(&Connection, P) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let mut tries = 1;
        let (tx, read) = loop {
            let read = (tries < TRIES)
                .then(|| self.read(|view| Ok::<_, E>((view.seen(), compute(view)?))))
                .transpose()?;
            let tx = begin(&self.conn, TransactionBehavior::Immediate)?;
            match read {
                Some((seen, read)) if data_version(&tx)? == seen => break (tx, read),
                Some(_) => tries += 1,
                None => {
                    let read = viewing(&tx, &mut compute)?;
                    break (tx, read);
                }
            }
        };
        crate::integrity::forget_moved();
        let out = write(&tx, read)?;
        crate::integrity::detect_moved(&tx, crate::integrity::DetectedBy::LocalValidator)?;
        if commit {
            tx.commit().map_err(DatumError::from)?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (tempfile::TempDir, Db, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("w.db");
        let db = Db::open(path.to_str().unwrap()).unwrap();
        db.conn.execute_batch("CREATE TABLE t (x INTEGER)").unwrap();
        let other = Connection::open(&path).unwrap();
        other.busy_timeout(std::time::Duration::ZERO).unwrap();
        (dir, db, other)
    }

    fn lock_is_free(other: &Connection) -> bool {
        let free = other.execute_batch("BEGIN IMMEDIATE").is_ok();
        if free {
            other.execute_batch("ROLLBACK").unwrap();
        }
        free
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
            .unwrap()
    }

    /// The computation runs while another connection could take the lock.
    #[test]
    fn a_computation_holds_no_lock() {
        let (_d, db, other) = pair();
        let mut free = false;
        db.compute_then_write(|view| {
            free = lock_is_free(&other);
            let mut b = WriteBatch::on(view.seen());
            b.push("INSERT INTO t VALUES (1)", vec![]);
            Ok::<_, DatumError>((b, ()))
        })
        .unwrap();
        assert!(free, "the lock was held while the batch was computed");
        assert_eq!(count(&db.conn), 1);
    }

    /// A commit by another connection between the computation and the
    /// write makes the batch stale, and it is computed again on what the
    /// commit left.
    #[test]
    fn a_batch_computed_before_another_commit_is_computed_again() {
        let (_d, db, other) = pair();
        let mut computed = Vec::new();
        db.compute_then_write(|view| {
            let n = count(view.conn());
            computed.push(n);
            if computed.len() == 1 {
                other.execute("INSERT INTO t VALUES (0)", []).unwrap();
            }
            let mut b = WriteBatch::on(view.seen());
            b.push("INSERT INTO t VALUES (?1)", vec![Value::Integer(n + 10)]);
            Ok::<_, DatumError>((b, ()))
        })
        .unwrap();
        assert_eq!(computed, vec![0, 1]);
        let wrote: i64 = db
            .conn
            .query_row("SELECT MAX(x) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(wrote, 11, "the batch of the second computation landed");
    }

    /// A database that moves under every try is computed under the lock
    /// the last time, so the write still lands.
    #[test]
    fn a_batch_always_stale_lands_computed_under_the_lock() {
        let (_d, db, other) = pair();
        let (mut tries, mut held) = (0, false);
        db.compute_then_write(|view| {
            tries += 1;
            if tries < TRIES {
                other.execute("INSERT INTO t VALUES (0)", []).unwrap();
            } else {
                held = !lock_is_free(&other);
            }
            let mut b = WriteBatch::on(view.seen());
            b.push("INSERT INTO t VALUES (1)", vec![]);
            Ok::<_, DatumError>((b, ()))
        })
        .unwrap();
        assert_eq!(tries, TRIES);
        assert!(held, "the last try computes under the lock");
        assert_eq!(count(&db.conn), TRIES as i64);
    }

    /// A view is read-only.
    #[test]
    fn a_view_refuses_a_write() {
        let (_d, db, _other) = pair();
        let wrote = db.read(|view| {
            Ok::<_, DatumError>(view.conn().execute("INSERT INTO t VALUES (1)", []).is_ok())
        });
        assert!(!wrote.unwrap());
        db.conn.execute("INSERT INTO t VALUES (1)", []).unwrap();
        assert_eq!(count(&db.conn), 1, "query_only is off again after the view");
    }

    /// A computation that fails leaves the connection writable.
    #[test]
    fn a_failed_computation_leaves_the_connection_writable() {
        let (_d, db, _other) = pair();
        let failed =
            db.compute_then_write(|_| Err::<(WriteBatch, ()), _>(DatumError::Other("no".into())));
        assert!(failed.is_err());
        db.conn.execute("INSERT INTO t VALUES (1)", []).unwrap();
        assert_eq!(count(&db.conn), 1);
    }

    /// Every read a computation makes on the handle — through `Db`'s own
    /// methods, not only `view.conn()` — is on the view's snapshot: it is
    /// the same connection, inside the view's transaction.
    #[test]
    fn a_computation_reads_one_snapshot_through_the_handle() {
        let (_d, db, other) = pair();
        let seen = db
            .read(|_| {
                let before = count(&db.conn);
                other.execute("INSERT INTO t VALUES (0)", []).unwrap();
                Ok::<_, DatumError>((before, count(&db.conn)))
            })
            .unwrap();
        assert_eq!(seen, (0, 0), "a commit beside the view is not seen in it");
        assert_eq!(count(&db.conn), 1);
    }

    /// The closure form computes outside the lock too, and a rehearsal
    /// writes nothing.
    #[test]
    fn the_closure_forms_compute_outside_the_lock() {
        let (_d, db, other) = pair();
        let mut free = false;
        let n = db
            .rehearse_with(
                |_| {
                    free = lock_is_free(&other);
                    Ok::<_, DatumError>(())
                },
                |tx, ()| {
                    tx.execute("INSERT INTO t VALUES (1)", [])?;
                    Ok::<_, DatumError>(count(tx))
                },
            )
            .unwrap();
        assert!(free);
        assert_eq!((n, count(&db.conn)), (1, 0));
    }

    /// `stats`' refill reads days × entries: it does so on a view, and only
    /// its batch takes the lock.
    #[test]
    fn a_stats_refill_is_computed_outside_the_lock() {
        let (_d, db, other) = pair();
        let mut free = false;
        db.stats_cache_fill("mood", "stamp", |_, _| {
            free = lock_is_free(&other);
            Ok::<_, DatumError>(Some(crate::db::Refill {
                zone: None,
                from: None,
                days: Vec::new(),
                lo: String::new(),
                through: "2026-10-15".into(),
            }))
        })
        .unwrap();
        assert!(free, "the lock was held while the refill was computed");
        assert_eq!(
            db.stats_cache_through("mood").unwrap(),
            Some((String::new(), "2026-10-15".into()))
        );
    }

    /// The crates' sources, outside tests, that are not this module.
    fn sources() -> Vec<(std::path::PathBuf, String)> {
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut out = Vec::new();
        let mut dirs: Vec<_> = std::fs::read_dir(&crates)
            .unwrap()
            .map(|e| e.unwrap().path().join("src"))
            .collect();
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for path in entries.map(|e| e.unwrap().path()) {
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|x| x == "rs")
                    && !path.ends_with("db/write.rs")
                    && !path.ends_with("tests.rs")
                {
                    let text = std::fs::read_to_string(&path).unwrap();
                    // Up to a file's test module.
                    let text = text.split("#[cfg(test)]").next().unwrap().to_string();
                    out.push((path, text));
                }
            }
        }
        out
    }

    /// clippy's disallowed methods cannot see SQL: a `BEGIN` sent as a
    /// statement opens a transaction this module does not.
    #[test]
    fn no_raw_begin_outside_db_write() {
        let mut found = Vec::new();
        for (path, text) in sources() {
            for (n, line) in text.lines().enumerate() {
                let begins = [
                    "\"BEGIN\"",
                    "\"BEGIN;",
                    "\"BEGIN ",
                    "BEGIN IMMEDIATE",
                    "BEGIN DEFERRED",
                    "BEGIN EXCLUSIVE",
                    "BEGIN TRANSACTION",
                ]
                .iter()
                .any(|b| line.contains(b));
                // kit.rs lists SQL's keywords, which a kit may not name a
                // field.
                if begins && !line.trim().starts_with("//") && line.trim() != "\"BEGIN\"," {
                    found.push(format!("{}:{}", path.display(), n + 1));
                }
            }
        }
        assert!(
            found.is_empty(),
            "a transaction opened outside db::write: {found:?}"
        );
    }

    /// The writes that still run a closure under the lock (da-8i7f0). Each
    /// one reduced to a [`WriteBatch`] lowers this, and nothing raises it.
    const UNBATCHED: usize = 32;

    #[test]
    fn unbatched_writes_only_shrink() {
        let n: usize = sources()
            .iter()
            .map(|(_, text)| {
                [
                    "write_with(",
                    "compute_then_write_with(",
                    "rehearse_with(",
                    "write_or_rehearse_with(",
                ]
                .iter()
                .map(|f| text.matches(&format!(".{f}")).count())
                .sum::<usize>()
            })
            .sum();
        assert!(n <= UNBATCHED, "{n} writes run a closure under the lock, past the {UNBATCHED} allowed: compute a WriteBatch on a View and Db::write it");
        assert_eq!(
            n, UNBATCHED,
            "{n} writes run a closure under the lock: lower UNBATCHED to {n}"
        );
    }
}
