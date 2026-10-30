//! What `datum check` found at its last run and what changed since
//! (da-3zq6g): the tables and triggers are `check_kept.sql`'s, which of a
//! checker's findings it keeps and when it may read only those is
//! `datum-commands`' `commands/check/kept.rs`.

use rusqlite::{params, OptionalExtension};

use crate::entry::Entry;
use crate::error::{DatumError, Result};

use super::row_mapper::row_to_entry;
use super::{Db, WriteBatch};

/// The tables and triggers (`check_kept.sql`).
pub(crate) const CHECK_KEPT_SQL: &str = include_str!("check_kept.sql");

/// One checker's last run: what it ran as, the change it read through, and
/// the keys of what it found.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckKept {
    pub stamp: String,
    pub through: i64,
    pub kept: Vec<String>,
}

impl Db {
    /// The last change written, which a check reads through: the log's
    /// sequence, which a prune that empties the log leaves where it was
    /// (vremya, PR #1400). An error where the file holds no `check_changes`
    /// (a reader's, laid by no open).
    pub fn check_through(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT coalesce(
                 (SELECT seq FROM sqlite_sequence WHERE name = 'check_changes'),
                 (SELECT max(seq) FROM check_changes),
                 0)",
            [],
            |r| r.get(0),
        )?)
    }

    /// The rows changed after `through`, `(hid, field, occur_time)`, an
    /// op's field and time `''`. `None` where some of them are gone: the
    /// log's cap dropped them (da-4a3cv), or a prune under another stamp did,
    /// which passes this mark as a prune under its own never does (da-tuhig).
    /// So the row after the mark is there, or with the log empty the sequence
    /// is at the mark, unless they went. One statement for the rows, so no
    /// prune or cap lands between them and that answer; the sequence only
    /// rises, so reading it after can only read from scratch for nothing.
    pub fn check_changed(&self, through: i64) -> Result<Option<Vec<(String, String, String)>>> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT seq, hid, field, at FROM check_changes WHERE seq > ?1")?;
        let mut rows = statement.query(params![through])?;
        let mut changed = Vec::new();
        while let Some(row) = rows.next()? {
            if changed.is_empty() && row.get::<_, i64>(0)? != through + 1 {
                return Ok(None);
            }
            changed.push((row.get(1)?, row.get(2)?, row.get(3)?));
        }
        if changed.is_empty() && through < self.check_through()? {
            return Ok(None);
        }
        Ok(Some(changed))
    }

    /// `checker`'s last run, `None` where it has kept none.
    pub fn check_kept(&self, checker: &str) -> Result<Option<CheckKept>> {
        let row: Option<(String, i64, String)> = self
            .conn
            .query_row(
                "SELECT stamp, through, kept FROM check_kept WHERE checker = ?1",
                params![checker],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        row.map(|(stamp, through, kept)| {
            Ok(CheckKept {
                stamp,
                through,
                kept: serde_json::from_str(&kept)?,
            })
        })
        .transpose()
    }

    /// Keep what `checker` found reading through `through`, and forget the
    /// changes every checker that ran as `stamp` has read. A run that read
    /// through less than one kept since keeps nothing: the log it would read
    /// on from may be gone.
    pub fn check_keep(
        &self,
        checker: &str,
        stamp: &str,
        through: i64,
        kept: &[String],
    ) -> Result<()> {
        let mut batch = WriteBatch::default();
        batch.push(
            "INSERT INTO check_kept (checker, stamp, through, kept)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (checker) DO UPDATE
                SET stamp = excluded.stamp, through = excluded.through, kept = excluded.kept
              WHERE excluded.stamp <> check_kept.stamp OR excluded.through >= check_kept.through",
            vec![
                checker.to_string().into(),
                stamp.to_string().into(),
                through.into(),
                serde_json::to_string(kept)?.into(),
            ],
        );
        batch.push(
            "DELETE FROM check_changes
              WHERE seq <= (SELECT min(through) FROM check_kept WHERE stamp = ?1)",
            vec![stamp.to_string().into()],
        );
        self.write(&batch).map(drop)
    }

    /// The hids of `field`'s rows, tombstones among them, off its index.
    pub fn check_field_hids(&self, field: &str) -> Result<Vec<String>> {
        self.conn
            .prepare_cached("SELECT hid FROM entries WHERE field = ?1")?
            .query_map(params![field], |r| r.get(0))?
            .map(|r| r.map_err(DatumError::Db))
            .collect()
    }

    /// The rows of `hids`, tombstones among them, in slug order as
    /// [`Db::list_including_deleted`] gives every row.
    pub fn entries_of(&self, hids: &[String]) -> Result<Vec<Entry>> {
        self.conn
            .prepare(
                "SELECT * FROM entries WHERE hid IN (SELECT value FROM json_each(?1))
                 ORDER BY slug ASC",
            )?
            .query_map(params![serde_json::to_string(hids)?], row_to_entry)?
            .map(|r| r.map_err(DatumError::Db))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run that read through less than one kept since leaves that one's
    /// mark: the changes between were pruned when it kept (PR #1400 review).
    #[test]
    fn a_mark_never_moves_back() {
        let db = Db::open_in_memory().unwrap();
        db.check_keep("tz", "s", 100, &["a".into()]).unwrap();
        db.check_keep("tz", "s", 95, &["b".into()]).unwrap();
        let kept = db.check_kept("tz").unwrap().unwrap();
        assert_eq!((kept.through, kept.kept), (100, vec!["a".to_string()]));
        // Another datum's run starts afresh, whatever its mark.
        db.check_keep("tz", "t", 7, &[]).unwrap();
        assert_eq!(db.check_kept("tz").unwrap().unwrap().through, 7);
    }

    /// A keep that reads the whole log empties it, and the mark is still the
    /// last change, not 0 (vremya, PR #1400).
    #[test]
    fn an_empty_log_keeps_its_mark() {
        let db = Db::open_in_memory().unwrap();
        let t = crate::time::DatumTime::now();
        let e = Entry::new(
            "tea",
            crate::entry::Verb::Point,
            None,
            None,
            Some(t),
            None,
            "d",
        );
        db.insert_entry(&e).unwrap();
        let through = db.check_through().unwrap();
        assert!(through > 0);
        db.check_keep("tz", "s", through, &[]).unwrap();
        assert_eq!(db.check_changed(through).unwrap(), Some(Vec::new()));
        assert_eq!(db.check_through().unwrap(), through);
    }

    /// `n` more rows in the log, as the triggers append them.
    fn append(db: &Db, n: i64) {
        db.conn()
            .execute(
                "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < ?1)
                 INSERT INTO check_changes (hid, field, at) SELECT 'h', 'f', '' FROM c",
                params![n],
            )
            .unwrap();
    }

    fn logged(db: &Db) -> i64 {
        db.conn()
            .query_row("SELECT count(*) FROM check_changes", [], |r| r.get(0))
            .unwrap()
    }

    /// With no check ever run the log stays within the cap and a chunk, and
    /// a real write reaches the cap's trigger through its own (da-4a3cv).
    #[test]
    fn the_log_stays_bounded() {
        const CAP: i64 = 32_768;
        const CHUNK: i64 = 1_024;
        let db = Db::open_in_memory().unwrap();
        append(&db, CAP);
        assert_eq!(logged(&db), CAP, "the cap takes nothing it need not");
        for _ in 0..4 {
            append(&db, CAP - 1);
            assert!(logged(&db) < CAP + CHUNK, "{}", logged(&db));
        }
        // One row short of the next chunk, and an entry's insert is its last.
        let through = db.check_through().unwrap();
        append(&db, CHUNK - through % CHUNK - 1);
        let before = logged(&db);
        assert!(before >= CAP + CHUNK - 1, "{before}");
        let t = crate::time::DatumTime::now();
        let e = Entry::new(
            "tea",
            crate::entry::Verb::Point,
            None,
            None,
            Some(t),
            None,
            "d",
        );
        db.insert_entry(&e).unwrap();
        assert!(logged(&db) <= CAP + 1, "{}", logged(&db));
    }

    /// A mark the cap has passed is told so, at one row past it and not at
    /// the row before; a prune that empties the log down to the mark is not
    /// that.
    #[test]
    fn a_mark_the_cap_passed_reads_from_scratch() {
        let db = Db::open_in_memory().unwrap();
        append(&db, 40_960);
        // The chunk at 40,960 dropped through 8,192.
        assert!(db.check_changed(8_191).unwrap().is_none());
        let at_edge = db.check_changed(8_192).unwrap().unwrap();
        assert_eq!(at_edge.len(), 32_768);
        assert_eq!(db.check_changed(40_960).unwrap(), Some(Vec::new()));
    }

    /// A prune under another stamp that empties the log is told too, with no
    /// row to tell by (da-tuhig); the mark it pruned down to reads nothing.
    #[test]
    fn a_mark_another_stamp_pruned_past_reads_from_scratch() {
        let db = Db::open_in_memory().unwrap();
        append(&db, 10);
        db.check_keep("tz", "one", 4, &[]).unwrap();
        db.check_keep("integrity", "other", 10, &[]).unwrap();
        assert_eq!(logged(&db), 0);
        assert!(db.check_changed(4).unwrap().is_none());
        assert_eq!(db.check_changed(10).unwrap(), Some(Vec::new()));
    }
}
