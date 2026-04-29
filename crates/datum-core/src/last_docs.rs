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
        assert!(within_protection_window(
            now - Duration::minutes(10),
            now
        ));
    }

    #[test]
    fn protection_window_old_pointer_is_outside() {
        let now = Utc::now();
        assert!(!within_protection_window(
            now - Duration::minutes(20),
            now
        ));
    }
}
