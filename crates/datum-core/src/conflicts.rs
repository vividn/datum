//! Non-blocking conflict queue (`sync_conflicts` table).
//!
//! D5 principle: the system never refuses data entry. When a user logs
//! something inconsistent — a `start` on a field that's already open, a
//! `stop` for a field with no open block, two overlapping `block`s —
//! both entries land in the `entries` table and a pending row is filed
//! here for later review.
//!
//! Detection runs post-insert (see [`detect_for_entry`]); resolution is
//! either explicit via `datum conflicts resolve` or applied automatically
//! by [`auto_resolve_pending`] once enough time has passed that the user
//! hasn't self-corrected.
//!
//! This module is also the backing store for C4e's sync-pull conflict
//! detection — the `detected_by` and `conflict_kind` columns are how the
//! two detection sources share the same queue.
//!
//! # Column meaning
//! - `id`: full 12-char base36 token.
//! - `hid`: 5-char prefix suitable for the CLI.
//! - `entry_slug`: the triggering (newer) entry's slug.
//! - `local_entry`: JSON of the triggering entry.
//! - `remote_entry`: JSON of the counterpart — the older conflicting local
//!   entry for `local_validator` rows; a received remote entry for
//!   `sync_pull`. `NULL` when no counterpart exists (orphan_end).
//! - `detected_at` / `resolved_at`: ISO 8601.
//! - `resolution`: NULL while pending; filled on resolve (auto or manual).
//! - `detected_by`: `local_validator` | `sync_pull`.
//! - `conflict_kind`: `double_open` | `orphan_end` | `overlap`.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::entry::{new_hid, Entry, EntryShape, Verb, SHORT_HID_LEN};
use crate::error::{DatumError, Result};

/// Source that filed a conflict row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedBy {
    /// Post-write detection on a local insert (D5).
    LocalValidator,
    /// Sync-pull reconciliation (C4e).
    SyncPull,
}

impl DetectedBy {
    pub fn as_str(&self) -> &'static str {
        match self {
            DetectedBy::LocalValidator => "local_validator",
            DetectedBy::SyncPull => "sync_pull",
        }
    }
}

impl std::str::FromStr for DetectedBy {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "local_validator" => Ok(DetectedBy::LocalValidator),
            "sync_pull" => Ok(DetectedBy::SyncPull),
            other => Err(format!("unknown detected_by: '{other}'")),
        }
    }
}

/// Shape of the inconsistency a conflict row describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// A second `start` for a field that already has a live ongoing entry.
    DoubleOpen,
    /// A `stop`/`end` with no matching prior ongoing entry.
    OrphanEnd,
    /// Two `block`-shaped entries on the same field with overlapping
    /// `[start_time, end_time]` intervals.
    Overlap,
}

impl ConflictKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConflictKind::DoubleOpen => "double_open",
            ConflictKind::OrphanEnd => "orphan_end",
            ConflictKind::Overlap => "overlap",
        }
    }
}

impl std::fmt::Display for ConflictKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ConflictKind {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "double_open" => Ok(ConflictKind::DoubleOpen),
            "orphan_end" => Ok(ConflictKind::OrphanEnd),
            "overlap" => Ok(ConflictKind::Overlap),
            other => Err(format!("unknown conflict_kind: '{other}'")),
        }
    }
}

/// Resolution labels written to `sync_conflicts.resolution` when a row is
/// closed. These strings are user-visible through `datum conflicts list`
/// so keep them stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Auto-resolver closed the older `ongoing` entry at the newer's start.
    AutoCloseOlder,
    /// User asked to keep both entries; no auto-merge will run.
    KeepBoth,
    /// User asked to close the older entry (manual form of AutoCloseOlder).
    CloseOlder,
}

impl Resolution {
    pub fn as_str(&self) -> &'static str {
        match self {
            Resolution::AutoCloseOlder => "auto_close_older",
            Resolution::KeepBoth => "keep_both",
            Resolution::CloseOlder => "close_older",
        }
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row from `sync_conflicts` rehydrated for callers.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub id: String,
    pub hid: String,
    pub entry_slug: String,
    pub local_entry: Entry,
    pub remote_entry: Option<Entry>,
    pub detected_at: DateTime<Utc>,
    pub resolution: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub detected_by: DetectedBy,
    pub conflict_kind: ConflictKind,
}

impl Conflict {
    /// True when `resolution IS NULL` — the row still needs attention.
    pub fn is_pending(&self) -> bool {
        self.resolution.is_none()
    }
}

/// Detect overlap-style problems caused by `trigger`. Runs inside the
/// caller's transaction so detection rows land atomically with the insert
/// that produced the triggering entry. Returns the filed conflicts (if
/// any). A `None` return from a particular kind is normal — most inserts
/// don't create conflicts.
///
/// Callers should run this after the triggering entry has been inserted
/// (so queries that read the `entries` table see it).
pub fn detect_for_entry(
    conn: &Connection,
    trigger: &Entry,
    detected_by: DetectedBy,
) -> Result<Vec<Conflict>> {
    // Add / Occur shapes don't affect state-timeline consistency; skip
    // them so freeform `datum add` and bare `datum occur` writes don't
    // ever file a conflict.
    if !trigger_is_state_affecting(trigger) {
        return Ok(vec![]);
    }

    let mut filed = Vec::new();

    match trigger.verb {
        Verb::Start => {
            if trigger.entry_shape == EntryShape::Ongoing {
                if let Some(older) = find_other_ongoing(conn, trigger)? {
                    filed.push(file_conflict(
                        conn,
                        trigger,
                        Some(&older),
                        detected_by,
                        ConflictKind::DoubleOpen,
                    )?);
                }
            } else if trigger.entry_shape == EntryShape::Block {
                if let Some(other) = find_overlapping_block(conn, trigger)? {
                    filed.push(file_conflict(
                        conn,
                        trigger,
                        Some(&other),
                        detected_by,
                        ConflictKind::Overlap,
                    )?);
                }
            }
        }
        Verb::Block => {
            if let Some(other) = find_overlapping_block(conn, trigger)? {
                filed.push(file_conflict(
                    conn,
                    trigger,
                    Some(&other),
                    detected_by,
                    ConflictKind::Overlap,
                )?);
            }
        }
        Verb::Stop => {
            let counterpart = find_ongoing_at_or_before(conn, trigger)?;
            if counterpart.is_none() {
                filed.push(file_conflict(
                    conn,
                    trigger,
                    None,
                    detected_by,
                    ConflictKind::OrphanEnd,
                )?);
            }
        }
        _ => {}
    }

    Ok(filed)
}

fn trigger_is_state_affecting(entry: &Entry) -> bool {
    matches!(entry.verb, Verb::Start | Verb::Stop | Verb::Block)
}

/// Any other live `ongoing`-shape entry on the same field. An ongoing
/// entry is one with `end_time IS NULL` that has not been closed by a
/// later stop on the same field; for detection purposes the simpler
/// "still has no end_time" query is sufficient — a subsequent stop would
/// close the block by inserting an orphan_end, which users can resolve.
fn find_other_ongoing(conn: &Connection, trigger: &Entry) -> Result<Option<Entry>> {
    conn.query_row(
        r#"SELECT * FROM entries
           WHERE field = ?1
             AND deleted_at IS NULL
             AND entry_shape = 'ongoing'
             AND end_time IS NULL
             AND hid != ?2
           ORDER BY start_time DESC
           LIMIT 1"#,
        params![trigger.field, trigger.hid],
        crate::db::row_to_entry_public,
    )
    .optional()
    .map_err(DatumError::Db)
}

/// A live `block`-shape entry on the same field whose `[start, end]`
/// overlaps `trigger`'s interval. Returns the highest-overlap candidate
/// (most recent start) when several match.
fn find_overlapping_block(conn: &Connection, trigger: &Entry) -> Result<Option<Entry>> {
    let (Some(ts), Some(te)) = (&trigger.start_time, &trigger.end_time) else {
        return Ok(None);
    };
    let ts_str = ts.utc.to_rfc3339();
    let te_str = te.utc.to_rfc3339();
    conn.query_row(
        r#"SELECT * FROM entries
           WHERE field = ?1
             AND deleted_at IS NULL
             AND entry_shape = 'block'
             AND hid != ?2
             AND start_time IS NOT NULL
             AND end_time   IS NOT NULL
             AND start_time < ?4
             AND end_time   > ?3
           ORDER BY start_time DESC
           LIMIT 1"#,
        params![trigger.field, trigger.hid, ts_str, te_str],
        crate::db::row_to_entry_public,
    )
    .optional()
    .map_err(DatumError::Db)
}

/// Most recent live ongoing entry on the same field that started at or
/// before `trigger.start_time`. Used to detect orphan-end: a stop whose
/// matching start cannot be found.
fn find_ongoing_at_or_before(conn: &Connection, trigger: &Entry) -> Result<Option<Entry>> {
    let at = match &trigger.start_time {
        Some(t) => t.utc.to_rfc3339(),
        None => return Ok(None),
    };
    conn.query_row(
        r#"SELECT * FROM entries
           WHERE field = ?1
             AND deleted_at IS NULL
             AND entry_shape = 'ongoing'
             AND end_time IS NULL
             AND start_time IS NOT NULL
             AND start_time <= ?2
             AND hid != ?3
           ORDER BY start_time DESC
           LIMIT 1"#,
        params![trigger.field, at, trigger.hid],
        crate::db::row_to_entry_public,
    )
    .optional()
    .map_err(DatumError::Db)
}

fn file_conflict(
    conn: &Connection,
    trigger: &Entry,
    counterpart: Option<&Entry>,
    detected_by: DetectedBy,
    kind: ConflictKind,
) -> Result<Conflict> {
    let id = new_hid();
    let hid: String = id.chars().take(SHORT_HID_LEN).collect();
    let local_json = serde_json::to_string(trigger)?;
    let remote_json = match counterpart {
        Some(e) => Some(serde_json::to_string(e)?),
        None => None,
    };
    let now = Utc::now();
    let now_str = now.to_rfc3339();
    conn.execute(
        r#"INSERT INTO sync_conflicts (
            id, hid, entry_slug, local_entry, remote_entry,
            detected_at, resolution, resolved_at, detected_by, conflict_kind
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, ?7, ?8)"#,
        params![
            id,
            hid,
            trigger.slug,
            local_json,
            remote_json,
            now_str,
            detected_by.as_str(),
            kind.as_str(),
        ],
    )?;
    Ok(Conflict {
        id,
        hid,
        entry_slug: trigger.slug.clone(),
        local_entry: trigger.clone(),
        remote_entry: counterpart.cloned(),
        detected_at: now,
        resolution: None,
        resolved_at: None,
        detected_by,
        conflict_kind: kind,
    })
}

/// Filter for [`list`]. `None` matches any row of that axis.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// Only rows of this conflict kind.
    pub kind: Option<ConflictKind>,
    /// `Some(true)` → pending only (resolution IS NULL).
    /// `Some(false)` → resolved only.
    /// `None` → both.
    pub pending: Option<bool>,
}

/// Read rows from `sync_conflicts`, newest first.
pub fn list(conn: &Connection, filter: &ListFilter) -> Result<Vec<Conflict>> {
    let mut sql = String::from(
        "SELECT id, hid, entry_slug, local_entry, remote_entry, detected_at,
                resolution, resolved_at, detected_by, conflict_kind
         FROM sync_conflicts",
    );
    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    if let Some(kind) = filter.kind {
        clauses.push(format!("conflict_kind = ?{}", binds.len() + 1));
        binds.push(kind.as_str().to_string());
    }
    match filter.pending {
        Some(true) => clauses.push("resolution IS NULL".to_string()),
        Some(false) => clauses.push("resolution IS NOT NULL".to_string()),
        None => {}
    }
    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY detected_at DESC, id DESC");

    let mut stmt = conn.prepare(&sql)?;
    let params_slice: Vec<&dyn rusqlite::ToSql> = binds
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    let rows = stmt.query_map(params_slice.as_slice(), row_to_conflict)?;
    rows.map(|r| r.map_err(DatumError::Db)).collect()
}

/// Fetch a single conflict by its hid or full id. Prefix match on `hid`
/// so the user can type the 5-char form.
pub fn get_by_hid_or_id(conn: &Connection, token: &str) -> Result<Option<Conflict>> {
    conn.query_row(
        "SELECT id, hid, entry_slug, local_entry, remote_entry, detected_at,
                resolution, resolved_at, detected_by, conflict_kind
         FROM sync_conflicts
         WHERE id = ?1 OR hid = ?1
         LIMIT 1",
        params![token],
        row_to_conflict,
    )
    .optional()
    .map_err(DatumError::Db)
}

/// Mark a conflict resolved. Caller owns the transaction when applying any
/// side effects (e.g. closing the older ongoing block for `AutoCloseOlder`).
pub fn mark_resolved(
    conn: &Connection,
    id: &str,
    resolution: Resolution,
    when: DateTime<Utc>,
) -> Result<()> {
    let changed = conn.execute(
        r#"UPDATE sync_conflicts
           SET resolution = ?2, resolved_at = ?3
           WHERE id = ?1 AND resolution IS NULL"#,
        params![id, resolution.as_str(), when.to_rfc3339()],
    )?;
    if changed == 0 {
        return Err(DatumError::NotFound(format!(
            "no pending conflict with id {id}"
        )));
    }
    Ok(())
}

fn row_to_conflict(row: &rusqlite::Row<'_>) -> rusqlite::Result<Conflict> {
    let local_json: String = row.get("local_entry")?;
    let local_entry: Entry = serde_json::from_str(&local_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let remote_entry: Option<Entry> = row
        .get::<_, Option<String>>("remote_entry")?
        .map(|s| serde_json::from_str::<Entry>(&s))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?;
    let detected_at: DateTime<Utc> = row
        .get::<_, String>("detected_at")?
        .parse()
        .map_err(|e: chrono::ParseError| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
        })?;
    let resolved_at: Option<DateTime<Utc>> = row
        .get::<_, Option<String>>("resolved_at")?
        .map(|s| s.parse::<DateTime<Utc>>())
        .transpose()
        .map_err(|e: chrono::ParseError| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
        })?;
    let detected_by_str: String = row.get("detected_by")?;
    let detected_by: DetectedBy = detected_by_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;
    let kind_str: String = row.get("conflict_kind")?;
    let conflict_kind: ConflictKind = kind_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;
    Ok(Conflict {
        id: row.get("id")?,
        hid: row.get("hid")?,
        entry_slug: row.get("entry_slug")?,
        local_entry,
        remote_entry,
        detected_at,
        resolution: row.get("resolution")?,
        resolved_at,
        detected_by,
        conflict_kind,
    })
}

/// Default delay (in seconds) before a pending conflict becomes eligible
/// for the auto-resolver — gives the user time to self-correct before
/// the heuristic fires. Overridable on `datum conflicts auto-resolve`.
pub const AUTO_RESOLVE_MIN_AGE_SECS: i64 = 60;

/// Run auto-resolution on pending conflicts older than `min_age_secs`.
/// Returns the conflicts whose `resolution` was set by this pass.
///
/// Rules:
/// - `DoubleOpen`: close the older ongoing entry at the newer one's
///   start_time. Records a `user_edit` on the older entry and marks the
///   conflict `auto_close_older`.
/// - `OrphanEnd`: if there's a recent ongoing on the same field (within
///   `ORPHAN_END_WINDOW_SECS`) from the same device, pair it off —
///   close that ongoing at the orphan stop's start_time and resolve
///   the conflict as `auto_close_older`.
/// - `Overlap`: no auto-rule (both sides kept; user must resolve).
pub fn auto_resolve_pending(
    conn: &Connection,
    now: DateTime<Utc>,
    min_age_secs: i64,
) -> Result<Vec<Conflict>> {
    let threshold = (now - chrono::Duration::seconds(min_age_secs)).to_rfc3339();
    let pending = list_pending_older_than(conn, &threshold)?;
    let mut applied = Vec::new();
    for c in pending {
        match c.conflict_kind {
            ConflictKind::DoubleOpen => {
                if let Some(older) = c.remote_entry.as_ref() {
                    // Safety: only auto-close if the older entry is
                    // genuinely recent. Very old ongoing blocks (>24h)
                    // likely describe a different activity the user
                    // forgot to close; force a manual decision there.
                    let age = now - older.start_time.as_ref().map(|t| t.utc).unwrap_or(now);
                    if age > chrono::Duration::hours(24) {
                        continue;
                    }
                    close_older_at(conn, older, c.local_entry.start_time.as_ref())?;
                    crate::conflicts::mark_resolved(
                        conn,
                        &c.id,
                        Resolution::AutoCloseOlder,
                        now,
                    )?;
                    applied.push(c);
                }
            }
            ConflictKind::OrphanEnd => {
                // A recent ongoing on the same field from the same
                // device is almost certainly the missing open block.
                // We already know (from detection) that at the time of
                // the stop there was no ongoing at-or-before; re-check
                // now with a short look-back window in case one was
                // retimed in since.
                if let Some(nearby) =
                    nearby_ongoing_for_orphan(conn, &c.local_entry, ORPHAN_END_WINDOW_SECS)?
                {
                    close_older_at(conn, &nearby, c.local_entry.start_time.as_ref())?;
                    crate::conflicts::mark_resolved(
                        conn,
                        &c.id,
                        Resolution::AutoCloseOlder,
                        now,
                    )?;
                    applied.push(c);
                }
            }
            ConflictKind::Overlap => {
                // No auto-rule. User decides via `datum conflicts resolve`.
            }
        }
    }
    Ok(applied)
}

/// How far back (in seconds) the orphan-end auto-resolver will look for a
/// matching ongoing entry.
const ORPHAN_END_WINDOW_SECS: i64 = 5 * 60;

fn list_pending_older_than(conn: &Connection, threshold_rfc3339: &str) -> Result<Vec<Conflict>> {
    let mut stmt = conn.prepare(
        "SELECT id, hid, entry_slug, local_entry, remote_entry, detected_at,
                resolution, resolved_at, detected_by, conflict_kind
         FROM sync_conflicts
         WHERE resolution IS NULL AND detected_at <= ?1
         ORDER BY detected_at ASC, id ASC",
    )?;
    let rows = stmt.query_map(params![threshold_rfc3339], row_to_conflict)?;
    rows.map(|r| r.map_err(DatumError::Db)).collect()
}

fn nearby_ongoing_for_orphan(
    conn: &Connection,
    orphan: &Entry,
    window_secs: i64,
) -> Result<Option<Entry>> {
    let at = match &orphan.start_time {
        Some(t) => t.utc,
        None => return Ok(None),
    };
    let lower = (at - chrono::Duration::seconds(window_secs)).to_rfc3339();
    let at_str = at.to_rfc3339();
    conn.query_row(
        r#"SELECT * FROM entries
           WHERE field = ?1
             AND deleted_at IS NULL
             AND entry_shape = 'ongoing'
             AND end_time IS NULL
             AND start_time IS NOT NULL
             AND start_time >= ?2
             AND start_time <= ?3
             AND device_id = ?4
           ORDER BY start_time DESC
           LIMIT 1"#,
        params![orphan.field, lower, at_str, orphan.device_id],
        crate::db::row_to_entry_public,
    )
    .optional()
    .map_err(DatumError::Db)
}

/// Write an `end_time` onto an existing entry identified by `older.hid`,
/// using the provided [`crate::time::DatumTime`]. The update bumps
/// `modify_time` and records a `user_edit` history row for the
/// pre-mutation state. Used by the auto-resolver and
/// [`crate::db::Db::resolve_conflict`].
pub(crate) fn close_older_at(
    conn: &Connection,
    older: &Entry,
    at: Option<&crate::time::DatumTime>,
) -> Result<()> {
    let Some(end_time) = at else {
        return Ok(());
    };
    // Re-fetch the current row so history records the live pre-state,
    // not the JSON snapshot that was copied into `remote_entry`. If the
    // row is gone (rare: user deleted it), silently skip.
    let pre: Option<Entry> = conn
        .query_row(
            "SELECT * FROM entries WHERE hid = ?1 AND deleted_at IS NULL",
            params![older.hid],
            crate::db::row_to_entry_public,
        )
        .optional()?;
    let Some(pre) = pre else {
        return Ok(());
    };
    crate::history::record(conn, &pre, crate::history::ChangeKind::UserEdit)?;
    let modify = Utc::now().to_rfc3339();
    let end_str = end_time.utc.to_rfc3339();
    conn.execute(
        r#"UPDATE entries
           SET end_time = ?1,
               end_time_offset = ?2,
               end_time_tz = ?3,
               modify_time = ?4,
               entry_shape = 'block',
               content_hash = ?5
           WHERE hid = ?6"#,
        params![
            end_str,
            end_time.offset_minutes,
            end_time.timezone,
            modify,
            // content_hash must reflect the new (field, start, end, value, data).
            recompute_content_hash_with_end(&pre, end_time),
            pre.hid,
        ],
    )?;
    Ok(())
}

fn recompute_content_hash_with_end(pre: &Entry, new_end: &crate::time::DatumTime) -> String {
    let mut patched = pre.clone();
    patched.end_time = Some(new_end.clone());
    patched.expected_content_hash()
}

/// Count of live ongoing entries for `field`. Cheap — used by `datum
/// now` to prepend an overlap warning when the field has >1 open start.
pub fn open_entries_for_field(conn: &Connection, field: &str) -> Result<u32> {
    let n: i64 = conn.query_row(
        r#"SELECT COUNT(*) FROM entries
           WHERE field = ?1
             AND deleted_at IS NULL
             AND entry_shape = 'ongoing'
             AND end_time IS NULL"#,
        params![field],
        |row| row.get(0),
    )?;
    Ok(n as u32)
}

#[cfg(test)]
mod tests {
    //! D5 core-level tests:
    //! - `insert_always_succeeds_regardless_of_state_consistency`
    //! - `post_write_hook_detects_overlap`

    use super::*;
    use crate::db::Db;
    use crate::entry::Verb;
    use crate::test_util::entry_builder;
    use crate::time::DatumTime;
    use chrono::{Duration, Utc};

    fn ts(seconds_ago: i64) -> DatumTime {
        DatumTime {
            utc: Utc::now() - Duration::seconds(seconds_ago),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        }
    }

    #[test]
    fn insert_always_succeeds_regardless_of_state_consistency() {
        // The invariant D5 protects: every insert lands, even patterns
        // that violate "normal" state consistency.
        let db = Db::open_in_memory().unwrap();

        // double-open
        let a = entry_builder("workout")
            .verb(Verb::Start)
            .time_start(ts(300))
            .build();
        db.insert_entry(&a).expect("first start lands");
        let b = entry_builder("workout").verb(Verb::Start).build();
        db.insert_entry(&b).expect("double start lands");

        // orphan stop
        let c = entry_builder("cooking").verb(Verb::Stop).build();
        db.insert_entry(&c).expect("orphan stop lands");

        // overlapping block
        let start = ts(600);
        let end = ts(300);
        let d = entry_builder("meeting")
            .verb(Verb::Block)
            .time_start(start.clone())
            .time_end(end.clone())
            .build();
        db.insert_entry(&d).expect("first block lands");
        let e = entry_builder("meeting")
            .verb(Verb::Block)
            .time_start(ts(500))
            .time_end(ts(200))
            .build();
        db.insert_entry(&e).expect("overlapping block lands");

        // All five entries should be in the table.
        let entries = db.all_entries().unwrap();
        assert_eq!(entries.len(), 5);
    }

    #[test]
    fn post_write_hook_detects_overlap() {
        // Writing an overlapping block must file an `overlap` conflict
        // row via the post-write hook.
        let db = Db::open_in_memory().unwrap();
        let a = entry_builder("meeting")
            .verb(Verb::Block)
            .time_start(ts(600))
            .time_end(ts(300))
            .build();
        db.insert_entry(&a).unwrap();
        let b = entry_builder("meeting")
            .verb(Verb::Block)
            .time_start(ts(500))
            .time_end(ts(200))
            .build();
        db.insert_entry(&b).unwrap();

        let rows = db
            .list_conflicts(&ListFilter {
                kind: Some(ConflictKind::Overlap),
                pending: Some(true),
            })
            .unwrap();
        assert_eq!(rows.len(), 1, "exactly one overlap conflict filed");
        let c = &rows[0];
        assert_eq!(c.conflict_kind, ConflictKind::Overlap);
        assert_eq!(c.detected_by, DetectedBy::LocalValidator);
        assert!(c.remote_entry.is_some(), "overlap has a counterpart");
    }
}
