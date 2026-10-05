//! Last-accessed entry tracking — backs the `_LAST` / `_LAST_WITH_PROTECTION`
//! quickId tokens.
//!
//! Mirrors datum's `lastDocs` doc (see
//! `datum/src/documentControl/lastDocs.ts`). Every command that targets a
//! specific entry — add, get, update, delete, edit, retime — stamps the
//! resulting hid(s) here so a follow-up bare command can default to those
//! same entries.
//!
//! Storage: a single-row table keyed on a constant id. The hids are stored
//! as a JSON array string; the timestamp is the standard ISO 8601 UTC form
//! used elsewhere in the schema.
//!
//! `_LAST_WITH_PROTECTION` enforces a recency window (15 minutes by
//! default, matching datum). `_LAST` has no time guard. Neither token
//! prompts on its own — the destructive-command y/N prompt is layered on
//! top by the command implementation.
//!
//! [`STALE_WARNING_MINUTES`] is the softer, ten-minute line `undo` and
//! `redo` use instead: they never refuse on age, they only say how old the
//! change they are about to move was.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension};

use crate::db::Db;
use crate::error::{DatumError, Result};
use crate::ids::Hid;

/// Singleton row id for the `last_docs` table.
const LAST_DOCS_ROW: i64 = 1;

/// How stale `_LAST_WITH_PROTECTION` may be before it refuses to resolve.
/// Matches datum's `LAST_DOCS_TOO_OLD_MINUTES` (15 min).
pub const PROTECTION_WINDOW_MINUTES: i64 = 15;

/// In-memory shape of the persisted last-docs row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastDocsRef {
    pub hids: Vec<Hid>,
    pub time: DateTime<Utc>,
}

/// Replace the last-docs pointer with the given hids. An empty `hids`
/// slice clears the row (a follow-up `_LAST` resolves to nothing).
pub fn set_last_docs(db: &Db, hids: &[Hid]) -> Result<()> {
    let json = serde_json::to_string(hids)?;
    let now = Utc::now().to_rfc3339();
    db.conn().execute(
        "INSERT INTO last_docs (id, hids, time) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET hids = excluded.hids, time = excluded.time",
        params![LAST_DOCS_ROW, json, now],
    )?;
    Ok(())
}

/// Where `entry_history` stands before a batch command writes: the mark
/// [`set_last_to_batch`] reads the batch back from.
pub fn history_mark(db: &Db) -> Result<i64> {
    Ok(db.conn().query_row(
        "SELECT COALESCE(MAX(rowid), 0) FROM entry_history",
        [],
        |r| r.get(0),
    )?)
}

/// Point `_LAST` at the whole batch: every entry written since `mark`, then
/// `also` (writes that leave no history row, like `v2import`'s bulk path), so
/// `_LAST` names all of it (da-yyjob). Rows a sync applied are not
/// this invocation's writes. A batch that wrote nothing leaves the pointer.
pub fn set_last_to_batch(db: &Db, mark: i64, also: &[Hid]) -> Result<()> {
    let mut stmt = db.conn().prepare(
        "SELECT entry_hid FROM entry_history
         WHERE rowid > ?1 AND change_kind NOT IN ('sync_merge', 'sync_conflict_resolve')
         GROUP BY entry_hid ORDER BY MIN(rowid)",
    )?;
    let written = stmt
        .query_map(params![mark], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut seen = std::collections::HashSet::new();
    let hids: Vec<Hid> = written
        .into_iter()
        .map(Hid::from)
        .chain(also.iter().cloned())
        .filter(|h| seen.insert(h.clone()))
        .collect();
    if !hids.is_empty() {
        set_last_docs(db, &hids)?;
    }
    Ok(())
}

/// Read the last-docs pointer. `Ok(None)` means no command has stamped it
/// yet (fresh DB, or it was cleared by a write of an empty hid list).
pub fn get_last_docs(db: &Db) -> Result<Option<LastDocsRef>> {
    let row: Option<(String, String)> = db
        .conn()
        .query_row(
            "SELECT hids, time FROM last_docs WHERE id = ?1",
            params![LAST_DOCS_ROW],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(DatumError::Db)?;

    let Some((hids_json, time_str)) = row else {
        return Ok(None);
    };
    let hids: Vec<Hid> = serde_json::from_str(&hids_json)?;
    let time = DateTime::parse_from_rfc3339(&time_str)
        .map_err(|e| DatumError::Other(format!("bad last_docs time: {e}")))?
        .with_timezone(&Utc);
    Ok(Some(LastDocsRef { hids, time }))
}

/// Has the pointer been touched within the protection window?
pub fn within_protection_window(reference_time: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now - reference_time <= Duration::minutes(PROTECTION_WINDOW_MINUTES)
}

/// How old the pointer may get before `undo` / `redo` SAY so. Not a gate:
/// those commands never refuse on age (da-s6no — undo is reversible now that
/// `redo` exists, so refusing costs more than it saves). Past this the command
/// still runs and names the age on stderr.
pub const STALE_WARNING_MINUTES: i64 = 10;

/// The pointer's age when it is past [`STALE_WARNING_MINUTES`], or `None`
/// while it is fresh. The caller words the warning; this decides whether
/// there is one.
pub fn stale_warning_age(reference_time: DateTime<Utc>, now: DateTime<Utc>) -> Option<Duration> {
    let age = now - reference_time;
    (age > Duration::minutes(STALE_WARNING_MINUTES)).then_some(age)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn get_returns_none_on_fresh_db() {
        let db = fresh_db();
        assert!(get_last_docs(&db).unwrap().is_none());
    }

    #[test]
    fn set_then_get_round_trips_hids() {
        let db = fresh_db();
        set_last_docs(&db, &[Hid::from("abc"), Hid::from("def")]).unwrap();
        let got = get_last_docs(&db).unwrap().unwrap();
        assert_eq!(got.hids, vec![Hid::from("abc"), Hid::from("def")]);
    }

    #[test]
    fn set_overwrites_previous_pointer() {
        let db = fresh_db();
        set_last_docs(&db, &[Hid::from("aaa")]).unwrap();
        set_last_docs(&db, &[Hid::from("bbb"), Hid::from("ccc")]).unwrap();
        let got = get_last_docs(&db).unwrap().unwrap();
        assert_eq!(got.hids, vec![Hid::from("bbb"), Hid::from("ccc")]);
    }

    #[test]
    fn protection_window_recent_pointer_is_inside() {
        let now = Utc::now();
        assert!(within_protection_window(now, now));
        assert!(within_protection_window(now - Duration::minutes(10), now));
    }

    #[test]
    fn stale_warning_is_silent_inside_ten_minutes() {
        let now = Utc::now();
        assert!(stale_warning_age(now, now).is_none());
        assert!(stale_warning_age(now - Duration::minutes(9), now).is_none());
    }

    #[test]
    fn stale_warning_names_the_age_past_ten_minutes() {
        let now = Utc::now();
        let age = stale_warning_age(now - Duration::minutes(23), now).expect("past the line");
        assert_eq!(age.num_minutes(), 23);
    }

    fn write(db: &Db, field: &str) -> Hid {
        let entry = crate::entry::Entry::new_legacy(
            field,
            crate::entry::Verb::Point,
            None,
            None,
            Some(crate::time::DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();
        entry.hid
    }

    /// A batch is every entry written since its mark, and a row a sync
    /// applied meanwhile is not one of them (da-yyjob).
    #[test]
    fn the_batch_is_what_was_written_since_the_mark_less_what_a_sync_applied() {
        let db = fresh_db();
        write(&db, "before");
        let mark = history_mark(&db).unwrap();
        let a = write(&db, "a");
        let b = write(&db, "b");
        let synced = write(&db, "synced");
        db.conn()
            .execute(
                "UPDATE entry_history SET change_kind = 'sync_merge' WHERE entry_hid = ?1",
                params![synced.as_str()],
            )
            .unwrap();
        set_last_to_batch(&db, mark, &[]).unwrap();
        assert_eq!(get_last_docs(&db).unwrap().unwrap().hids, vec![a, b]);
    }

    #[test]
    fn protection_window_old_pointer_is_outside() {
        let now = Utc::now();
        assert!(!within_protection_window(now - Duration::minutes(20), now));
    }
}
