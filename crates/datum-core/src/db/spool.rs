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
//! A line that does not read is set aside, never in the way (da-29om3): the
//! last one, with no newline after it, to `<db>.spool.torn` (an append cut
//! off by a crash), any other to `<db>.spool.rejected`, each with the time
//! it was found. An append starts on a fresh line, so a torn one never
//! glues itself to the next: a torn line an append has followed is no
//! longer last, and goes to `.rejected`.
//!
//! Native only in use: the browser's database is one process's at a time.

use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
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

/// A queued line, with the id it was appended under.
type Line = (String, Value);

/// The spool's lines, each with the id it was appended under, and where
/// each line that did not read was set aside: the spool is written again
/// without them. Taken under the spool's lock.
fn lines(spool: &Path) -> Result<(Vec<Line>, Vec<PathBuf>)> {
    let bytes = match std::fs::read(spool) {
        Ok(b) => b,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Default::default()),
        Err(e) => return Err(e.into()),
    };
    let found = chrono::Utc::now().to_rfc3339();
    let (mut read, mut kept, mut aside) = (Vec::new(), Vec::new(), Vec::new());
    for (l, line, last) in pieces(&bytes) {
        match line {
            Some(line) => {
                kept.extend_from_slice(l);
                kept.push(b'\n');
                read.push(line);
            }
            None => {
                let to = beside(spool, if last { ".torn" } else { ".rejected" });
                append_line(&to, &[found.as_bytes(), b"\t", l].concat())?;
                aside.push(to);
            }
        }
    }
    if !aside.is_empty() {
        replace(spool, &kept)?;
    }
    Ok((read, aside))
}

/// Each piece of `bytes` between newlines that is not blank, the line it
/// reads as if it does, and whether it is the last, with no newline after
/// it. Reads only: [`lines`] sets aside what does not read.
fn pieces(bytes: &[u8]) -> impl Iterator<Item = (&[u8], Option<Line>, bool)> {
    let mut all = bytes.split(|b| *b == b'\n').peekable();
    std::iter::from_fn(move || loop {
        let l = all.next()?;
        if l.trim_ascii().is_empty() {
            continue;
        }
        let line = serde_json::from_slice::<Value>(l)
            .ok()
            .filter(|v| v["id"].is_string())
            .map(|v| (v["id"].as_str().unwrap_or_default().to_string(), v));
        return Some((l, line, all.peek().is_none()));
    })
}

/// Append `line` to `path`, fsynced, on a line of its own. Its callers hold
/// the spool's lock, so nothing appends between reading the last byte and
/// writing after it.
fn append_line(path: &Path, line: &[u8]) -> Result<()> {
    let created = !path.exists();
    let mut f = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(path)?;
    let mut end = *b"\n";
    if f.metadata()?.len() > 0 {
        f.seek(SeekFrom::End(-1))?;
        f.read_exact(&mut end)?;
    }
    let fresh: &[u8] = if end[0] == b'\n' { b"" } else { b"\n" };
    f.write_all(&[fresh, line, b"\n"].concat())?;
    f.sync_all()?;
    if created {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            File::open(dir)?.sync_all()?;
        }
    }
    Ok(())
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

/// How many lines of `spool` `conn` has not committed: each that reads and
/// is after `conn`'s mark of the last one committed, every one that reads
/// when that mark will not, and each that does not read, as a purge meets a
/// damaged file. Reads only, so it needs no lock.
fn queued(conn: &Connection, spool: &Path) -> usize {
    let bytes = std::fs::read(spool).unwrap_or_default();
    let mut unread = 0;
    let read: Vec<Line> = pieces(&bytes)
        .filter_map(|(_, line, _)| {
            unread += usize::from(line.is_none());
            line
        })
        .collect();
    let n = read.len();
    pending(conn, read).map_or(n, |p| p.len()) + unread
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
fn replace(path: &Path, text: impl AsRef<[u8]>) -> Result<()> {
    let text = text.as_ref();
    if text.is_empty() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        };
    }
    let tmp = beside(path, ".tmp");
    let mut f = File::create(&tmp)?;
    f.write_all(text)?;
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
    set_aside: Vec<PathBuf>,
    short: Option<DatumError>,
}

/// What a drain did: how many lines landed, where each line it could not
/// read was set aside, and why the line it stopped at did not land.
#[derive(Debug, Default)]
pub struct Drained {
    pub landed: usize,
    pub set_aside: Vec<PathBuf>,
    pub stuck: Option<DatumError>,
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
    /// the snapshot holds every line before it and none twice. A spool it
    /// cannot read or land holds no write back (da-29om3): the snapshot
    /// stops short of it, and the command still queues its own line.
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
            refold_failed: Default::default(),
            path: None,
        };
        db.migrate(None)?;
        let spool = beside(file, ".spool");
        let (queued, set_aside, mut short) = match lines(&spool) {
            Ok((queued, set_aside)) => (queued, set_aside, None),
            Err(e) => (Vec::new(), Vec::new(), Some(e)),
        };
        for line in pending(&db.conn, queued)? {
            if let Err(e) = land(&db, &line) {
                short = Some(e);
                break;
            }
        }
        let spooled = Spool {
            db: file.to_path_buf(),
            path: spool,
            seq: sync::own_seq(&db.conn)?,
            last: last_docs(&db.conn)?,
            set_aside,
            short,
        };
        Ok((db, spooled))
    }

    /// Are changes queued beside this database, not landed yet?
    pub fn spool_waiting(&self) -> bool {
        self.path
            .as_ref()
            .is_some_and(|p| beside(p, ".spool").exists())
    }

    /// How many changes are queued beside this database, not landed yet.
    pub fn spool_queued(&self) -> usize {
        self.path
            .as_ref()
            .map_or(0, |p| queued(&self.conn, &beside(p, ".spool")))
    }

    /// Run `wipe` holding the spool, so no drain lands it meanwhile, and
    /// once it succeeded remove the spool: a change queued before a purge
    /// is the old copy's data, and landed after it would put back part of
    /// what was purged (da-owy96). What `wipe` gave, and how many queued
    /// changes went.
    pub fn spool_discard_after<T, E: From<DatumError>>(
        &self,
        wipe: impl FnOnce() -> std::result::Result<T, E>,
    ) -> std::result::Result<(T, usize), E> {
        // No file (`:memory:`), or the browser's, which is one process's
        // at a time and has no file locks: nothing spools beside it.
        let Some(path) = self
            .path
            .as_ref()
            .filter(|p| !cfg!(target_family = "wasm") && p.exists())
        else {
            return wipe().map(|t| (t, 0));
        };
        // Held with no spool too: a write the wipe's lock turns away
        // snapshots only once the wipe is done, and so queues nothing of
        // the old copy.
        let _held = lock(path, true)?;
        let spool = beside(path, ".spool");
        let n = queued(&self.conn, &spool);
        let out = wipe()?;
        replace(&spool, "")?;
        Ok((out, n))
    }

    /// Land the changes queued beside this database, in order, until the
    /// write lock is another's. A line that fails for any other reason is
    /// `stuck`, and it and the lines after it wait.
    pub fn drain_spool(&self) -> Result<Drained> {
        let Some(path) = &self.path else {
            return Ok(Drained::default());
        };
        let spool = beside(path, ".spool");
        if !spool.exists() {
            return Ok(Drained::default());
        }
        // A spooler or a drainer at work: the next drain lands it.
        let Some(_held) = lock(path, false)? else {
            return Ok(Drained::default());
        };
        let (queued, set_aside) = lines(&spool)?;
        let todo = pending(&self.conn, queued)?;
        let mut landed = 0;
        let stuck = waiting(&self.conn, DRAIN_WAIT, || {
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
        Ok(Drained {
            landed,
            set_aside,
            stuck,
        })
    }

    /// What waits beside this database (da-3j6g7): when each change queued
    /// and not landed was queued, oldest first, and the files holding lines
    /// set aside, which never land on their own. Nothing is landed.
    pub fn spool_queue(&self) -> Result<(Vec<String>, Vec<PathBuf>)> {
        let Some(path) = &self.path else {
            return Ok(Default::default());
        };
        let _held = lock(path, true)?;
        let waiting = pending(&self.conn, lines(&beside(path, ".spool"))?.0)?
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
        let mut todo = pending(&self.conn, lines(&spool)?.0)?.into_iter();
        let Some((_, first)) = todo.next() else {
            return Ok(None);
        };
        // Kept before it leaves the spool: cut off between the two, it is
        // in both, and lands at the next drain, rather than in neither.
        let to = beside(path, ".spool.rejected");
        append_line(
            &to,
            format!("{}\t{first}", chrono::Utc::now().to_rfc3339()).as_bytes(),
        )?;
        replace(
            &spool,
            todo.map(|(_, v)| format!("{v}\n")).collect::<String>(),
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
        append_line(&self.path, line.to_string().as_bytes())?;
        Ok(true)
    }

    /// Where each queued line [`Db::spool_open`] could not read was set aside.
    pub fn set_aside(&self) -> &[PathBuf] {
        &self.set_aside
    }

    /// Why the snapshot stops short of the queued lines, if it does: the
    /// spool could not be read, or a line in it could not land.
    pub fn short(&self) -> Option<&DatumError> {
        self.short.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An append after a torn line starts a fresh one, so the line it adds
    /// reads, and only the torn one is set aside (da-29om3).
    #[test]
    fn an_append_after_a_torn_line_starts_a_fresh_one() {
        let dir = tempfile::tempdir().unwrap();
        let spool = dir.path().join("x.db.spool");
        std::fs::write(&spool, "{\"id\":\"a\"}\n{\"id\":\"to").unwrap();
        append_line(&spool, b"{\"id\":\"b\"}").unwrap();
        let (read, aside) = lines(&spool).unwrap();
        let ids: Vec<&str> = read.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(aside, [dir.path().join("x.db.spool.rejected")]);
        let rejected = std::fs::read_to_string(&aside[0]).unwrap();
        assert!(rejected.ends_with("\t{\"id\":\"to\n"), "{rejected}");
        assert_eq!(
            std::fs::read_to_string(&spool).unwrap(),
            "{\"id\":\"a\"}\n{\"id\":\"b\"}\n"
        );
    }
}
