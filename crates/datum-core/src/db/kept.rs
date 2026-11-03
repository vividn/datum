//! The connections a long-lived datum keeps open between its commands
//! (da-jm4sv): datum.day's database worker runs every command in one
//! instance, and a connection kept there keeps its page cache and its
//! prepared statements, where a process opened the file afresh each time.
//!
//! A command still opens its database as it always has, [`Db::open`], migration and all;
//! the open takes the kept connection instead of a new one, and the handle's drop hands it
//! back. Nothing is kept unless [`keep_open`] names the file, so a native process keeps nothing.
//!
//! A connection is kept UNLOCKED. On wasm it holds its lock for its life
//! (`locking_mode = EXCLUSIVE`, da-o3sns), trusting its cache, which is right
//! for one command and wrong across two: another tab's process may write the
//! file in between. So it gives the lock up as it is handed back, and its next
//! read takes it again as any connection's first read does, reading the file's
//! header and emptying its cache if another connection changed the file
//! meanwhile. Unlocked, its next write moves the header's change counter
//! again too, which is how the worker tells a write.
//!
//! [`Db::open`]: super::Db::open

use std::cell::RefCell;
use std::collections::HashMap;

use rusqlite::Connection;

thread_local! {
    /// The files whose connection is kept, and the connection while no handle has it.
    static KEPT: RefCell<HashMap<String, Option<Connection>>> = RefCell::default();
}

/// Keep the connection to each of `paths`, named as [`Db::open`] is given
/// them, open between commands; any other kept connection closes.
///
/// [`Db::open`]: super::Db::open
pub fn keep_open(paths: &[String]) {
    KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        kept.retain(|path, _| paths.contains(path));
        for path in paths {
            kept.entry(path.clone()).or_default();
        }
    });
}

/// The connection kept for `path`, if one waits, and whether a connection to
/// it is kept at all.
pub(super) fn take(path: &str) -> (Option<Connection>, bool) {
    KEPT.with(|kept| match kept.borrow_mut().get_mut(path) {
        Some(slot) => (slot.take(), true),
        None => (None, false),
    })
}

/// Keep `conn` for `path`, unlocked, unless one is kept already or it cannot
/// let go of its lock: then it closes.
pub(super) fn give_back(path: &str, conn: Connection) {
    let unlocked = conn.is_autocommit()
        && conn
            .query_row("PRAGMA locking_mode = NORMAL", [], |_| Ok(()))
            .and_then(|()| conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(())))
            .is_ok();
    if !unlocked {
        return;
    }
    KEPT.with(|kept| {
        if let Some(slot @ None) = kept.borrow_mut().get_mut(path) {
            *slot = Some(conn);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    /// The connection a handle had, by its SQLite handle's address.
    fn handle(db: &Db) -> usize {
        // SAFETY: read as a number, never dereferenced.
        unsafe { db.conn().handle() as usize }
    }

    #[test]
    fn a_named_file_keeps_its_connection_and_a_file_gone_starts_afresh() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kept.db").to_string_lossy().into_owned();
        keep_open(std::slice::from_ref(&path));
        let first = handle(&Db::open(&path).unwrap());
        let db = Db::open(&path).unwrap();
        assert_eq!(handle(&db), first, "the kept connection");
        // Out while one handle has it: a second open is a connection of its own.
        assert_ne!(handle(&Db::open(&path).unwrap()), first);
        db.conn()
            .execute("INSERT INTO _meta (key, value) VALUES ('kept', 'x')", [])
            .unwrap();
        drop(db);

        std::fs::remove_file(&path).unwrap();
        let db = Db::open(&path).unwrap();
        let kept: Option<String> = db
            .conn()
            .query_row("SELECT value FROM _meta WHERE key = 'kept'", [], |r| {
                r.get(0)
            })
            .ok();
        assert_eq!(kept, None, "the file made anew, not the removed one read");
        drop(db);

        keep_open(&[]);
        assert!(
            !Db::open(&path).unwrap().keep,
            "a file not named keeps nothing"
        );
    }
}
