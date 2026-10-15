//! The spool (da-9q3fr): a write that cannot take the database's lock is
//! never lost.
//!
//! A command that finds another connection holding the write lock runs as
//! it would on a SNAPSHOT of the database ([`Db::spool_open`]): a copy in
//! memory, read under WAL's snapshot and no lock, with the queued changes
//! before it already landed. It reads what is there, decides what it would
//! do — the entry a log makes, which entry an undo takes back, a retime's
//! new time — and signs ops as this copy, which it prints as it would print
//! the write (with QUEUED for its action word). Those ops are its RESOLVED
//! effect, and [`Spool::append`] appends them, one JSON line per command,
//! to `<db>.spool`, fsynced, before the command exits 0.
//!
//! `<db>.spool.lock` is held while a snapshot reads the queued lines, while
//! a line is appended, and by a drain from its read of the file to its
//! rewrite, so no line is appended to a file a drain then renames over. It
//! is NOT held while the command runs: one may wait on its person (`edit`'s
//! editor, a question about a stale `_LAST`), and another spooler would wait
//! with it.
//!
//! [`Db::drain_spool`] lands the lines in order, one transaction each, at
//! the start of the next command and on a sync daemon's tick: each op
//! signed again as this copy's next with the `deps` it was made with, so a
//! change that landed meanwhile, which it never saw, meets it under the
//! fold's merge and conflict rules as a late op from another copy would
//! (`sync::apply_spooled`). `_meta` `spool_drained` names the last line
//! committed, in the line's own transaction, so a drain cut off between a
//! commit and rewriting the spool skips what landed and a second drain
//! writes nothing more. A line leaves the file only after it committed.
//!
//! Native only in use: the browser's database is one process's at a time.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use super::{sync, Db};
use crate::error::{DatumError, Result};

/// How long a drain waits for the write lock before leaving the lines for
/// the next: it runs before every command, which must not wait on it.
const DRAIN_WAIT: Duration = Duration::from_millis(100);

/// Is `e`, or what caused it, SQLite finding the database locked?
pub fn busy(e: &(dyn std::error::Error + 'static)) -> bool {
    use rusqlite::ErrorCode::{DatabaseBusy, DatabaseLocked};
    let mut cause = Some(e);
    while let Some(c) = cause {
        let code = match c.downcast_ref::<DatumError>() {
            Some(DatumError::Db(e)) => e.sqlite_error_code(),
            _ => c
                .downcast_ref::<rusqlite::Error>()
                .and_then(rusqlite::Error::sqlite_error_code),
        };
        if matches!(code, Some(DatabaseBusy | DatabaseLocked)) {
            return true;
        }
        cause = c.source();
    }
    false
}

fn beside(db: &Path, ext: &str) -> PathBuf {
    let mut p = db.as_os_str().to_owned();
    p.push(ext);
    p.into()
}

/// The lock every spooler and drainer of `db` takes: a file of its own,
/// since the spool itself is replaced whole.
fn lock(db: &Path, wait: bool) -> Result<Option<File>> {
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(beside(db, ".spool.lock"))?;
    match wait {
        true => f.lock().map(|_| Some(f)).map_err(Into::into),
        false => match f.try_lock() {
            Ok(()) => Ok(Some(f)),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
        },
    }
}

/// The spool's lines, each with the id it was appended under.
fn lines(spool: &Path) -> Result<Vec<(String, Value)>> {
    let mut text = String::new();
    match File::open(spool) {
        Ok(mut f) => f.read_to_string(&mut text)?,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l)?;
            let id = v["id"].as_str().unwrap_or_default().to_string();
            Ok((id, v))
        })
        .collect()
}

/// The lines `conn` has not committed: those after `spool_drained`.
fn pending(conn: &Connection, all: Vec<(String, Value)>) -> Result<Vec<(String, Value)>> {
    let drained: Option<String> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'spool_drained'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let landed = drained.and_then(|d| all.iter().position(|(id, _)| *id == d));
    Ok(all.into_iter().skip(landed.map_or(0, |i| i + 1)).collect())
}

/// Land one line, in its own transaction: ops computed already, so the
/// lock is held for their writes (da-8i7f0).
fn land(db: &Db, (id, line): &(String, Value)) -> Result<()> {
    db.write_with(|tx| {
        sync::apply_spooled(tx, line["ops"].as_array().map_or(&[], Vec::as_slice))?;
        if let Some(last) = line.get("last").filter(|l| !l.is_null()) {
            tx.execute(
                "INSERT INTO last_docs (id, hids, time) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET hids = excluded.hids, time = excluded.time",
                params![
                    crate::last_docs::LAST_DOCS_ROW,
                    last.to_string(),
                    line["at"].as_str().unwrap_or_default()
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO _meta (key, value) VALUES ('spool_drained', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [id],
        )?;
        Ok(())
    })
}

fn last_docs(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT hids FROM last_docs WHERE id = ?1",
            [crate::last_docs::LAST_DOCS_ROW],
            |r| r.get(0),
        )
        .optional()?)
}

/// Write `text` as the whole of `path`, or remove it when empty: a whole
/// new file renamed over it, so a cut write leaves the old one.
fn replace(path: &Path, text: &str) -> Result<()> {
    if text.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    let tmp = beside(path, ".tmp");
    let mut f = File::create(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Run `f` with `conn` waiting `wait` for a lock, and then as long as before.
fn waiting<T>(conn: &Connection, wait: Duration, f: impl FnOnce() -> T) -> Result<T> {
    let before: u64 = conn.query_row("PRAGMA busy_timeout", [], |r| r.get(0))?;
    conn.busy_timeout(wait)?;
    let out = f();
    conn.busy_timeout(Duration::from_millis(before))?;
    Ok(out)
}

/// A command run on a snapshot: its database's file, and where the
/// snapshot stood before it ran.
pub struct Spool {
    db: PathBuf,
    path: PathBuf,
    seq: i64,
    last: Option<String>,
}

impl Db {
    /// Take the write lock within `wait`, and let it go: whether a write
    /// now would land.
    pub fn can_write(&self, wait: Duration) -> Result<bool> {
        match waiting(&self.conn, wait, || self.take_the_lock())? {
            Ok(()) => Ok(true),
            Err(e) if busy(&e) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// The database at `path` as it stands, in memory and writable, with
    /// its queued changes landed: what a command that cannot take the
    /// write lock runs on. Its endpoint is the file's, so what it signs is
    /// this copy's. The spool is held while it reads the queued lines, so
    /// the snapshot holds every line before it and none twice.
    pub fn spool_open(path: &str) -> Result<(Db, Spool)> {
        let file = Path::new(path);
        let _held = lock(file, true)?.expect("a waiting lock is taken");
        Self::choose_vfs();
        let from = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        // A WAL reader rarely waits (a checkpoint restarting the log, its
        // index being rebuilt), but a backup answered BUSY is a lost write.
        from.busy_timeout(Duration::from_secs(5))?;
        let mut conn = Connection::open_in_memory()?;
        // Every page in one step: one read transaction, one snapshot.
        rusqlite::backup::Backup::new(&from, &mut conn)?.run_to_completion(
            i32::MAX,
            Duration::ZERO,
            None,
        )?;
        drop(from);
        Self::configure_connection(&mut conn)?;
        // `sync::endpoint` takes the stored key only for the path it was
        // minted at, or a blank one, and an in-memory copy's path is "":
        // left as the file's, the snapshot would mint a key of its own.
        let _ = conn.execute("UPDATE sync_local SET path = ''", []);
        let db = Db {
            conn,
            holds: None,
            spec_alias_cache: Default::default(),
            client_id: Default::default(),
            apply_run: Default::default(),
            last_migration: None,
            path: None,
        };
        db.migrate(None)?;
        let spool = beside(file, ".spool");
        for line in pending(&db.conn, lines(&spool)?)? {
            land(&db, &line)?;
        }
        let spooled = Spool {
            db: file.to_path_buf(),
            path: spool,
            seq: sync::own_seq(&db.conn)?,
            last: last_docs(&db.conn)?,
        };
        Ok((db, spooled))
    }

    /// Are changes queued beside this database, not landed yet?
    pub fn spool_waiting(&self) -> bool {
        self.path
            .as_ref()
            .is_some_and(|p| beside(p, ".spool").exists())
    }

    /// Land the changes queued beside this database, in order, until the
    /// write lock is another's: how many landed. A line that fails for any
    /// other reason is the error, and it and the lines after it wait.
    pub fn drain_spool(&self) -> Result<usize> {
        let Some(path) = &self.path else {
            return Ok(0);
        };
        let spool = beside(path, ".spool");
        if !spool.exists() {
            return Ok(0);
        }
        // A spooler or a drainer at work: the next drain lands it.
        let Some(_held) = lock(path, false)? else {
            return Ok(0);
        };
        let todo = pending(&self.conn, lines(&spool)?)?;
        let mut landed = 0;
        let failed = waiting(&self.conn, DRAIN_WAIT, || {
            for line in &todo {
                match land(self, line) {
                    Ok(()) => landed += 1,
                    Err(e) if busy(&e) => return None,
                    Err(e) => return Some(e),
                }
            }
            None
        })?;
        let rest: String = todo[landed..]
            .iter()
            .map(|(_, v)| format!("{v}\n"))
            .collect();
        replace(&spool, &rest)?;
        failed.map_or(Ok(landed), Err)
    }

    /// What waits beside this database (da-3j6g7): when each change queued
    /// and not landed was queued, oldest first, and the files holding lines
    /// set aside, which never land on their own. Nothing is landed.
    pub fn spool_queue(&self) -> Result<(Vec<String>, Vec<PathBuf>)> {
        let Some(path) = &self.path else {
            return Ok(Default::default());
        };
        let _held = lock(path, true)?;
        let waiting = pending(&self.conn, lines(&beside(path, ".spool"))?)?
            .into_iter()
            .map(|(_, v)| v["at"].as_str().unwrap_or_default().to_string())
            .collect();
        let aside = [".spool.torn", ".spool.rejected"]
            .into_iter()
            .map(|ext| beside(path, ext))
            .filter(|p| p.exists())
            .collect();
        Ok((waiting, aside))
    }

    /// Set the first change queued beside this database aside, appended to
    /// `<db>.spool.rejected` after the time it was set aside and a tab, so
    /// the lines after it can land: where it went, when one waited.
    pub fn spool_set_aside(&self) -> Result<Option<PathBuf>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        let _held = lock(path, true)?;
        let spool = beside(path, ".spool");
        let mut todo = pending(&self.conn, lines(&spool)?)?.into_iter();
        let Some((_, first)) = todo.next() else {
            return Ok(None);
        };
        // Kept before it leaves the spool: cut off between the two, it is
        // in both, and lands at the next drain, rather than in neither.
        let to = beside(path, ".spool.rejected");
        let mut f = OpenOptions::new().create(true).append(true).open(&to)?;
        writeln!(f, "{}\t{first}", chrono::Utc::now().to_rfc3339())?;
        f.sync_all()?;
        replace(
            &spool,
            &todo.map(|(_, v)| format!("{v}\n")).collect::<String>(),
        )?;
        Ok(Some(to))
    }
}

impl Spool {
    /// Append what `snapshot` signed since [`Db::spool_open`], fsynced:
    /// whether there was anything to queue.
    pub fn append(self, snapshot: &Db) -> Result<bool> {
        let ops = sync::signed_since(&snapshot.conn, self.seq)?;
        if ops.is_empty() {
            return Ok(false);
        }
        let last = last_docs(&snapshot.conn)?.filter(|l| Some(l) != self.last.as_ref());
        let now = chrono::Utc::now();
        let line = json!({
            // Compared only for equality with the last line committed: a
            // random hid, so no two lines can share one.
            "id": crate::entry::new_hid().to_string(),
            "at": now.to_rfc3339(),
            "ops": ops,
            "last": last.map(|l| serde_json::from_str::<Value>(&l)).transpose()?,
        });
        let _held = lock(&self.db, true)?.expect("a waiting lock is taken");
        let created = !self.path.exists();
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(format!("{line}\n").as_bytes())?;
        f.sync_all()?;
        if created {
            if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
                File::open(dir)?.sync_all()?;
            }
        }
        Ok(true)
    }
}
