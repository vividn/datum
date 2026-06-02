//! Per-entry modification trail (`entry_history` table).
//!
//! Every mutating operation on the `entries` table writes a companion row
//! here in the same transaction. Version numbering is monotonic per entry:
//! version=1 is the original create, version=2 is the first edit, and so on.
//!
//! `snapshot` stores the full entry JSON. For version=1 (`create`) it holds
//! the newly-created entry; for later versions it holds the **pre-operation**
//! state so a revert can restore that earlier shape without consulting the
//! live row.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::entry::Entry;
use crate::error::{DatumError, Result};

/// Classification of the operation that produced a history row.
///
/// The string form is what gets stored in the `change_kind` column and is
/// also what `datum history <hid>` prints — so these strings are part of
/// the user-visible surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// First-ever write of the entry.
    Create,
    /// User-driven mutation (update / merge / edit / delete / undelete).
    UserEdit,
    /// Batch time shift via `datum retime`.
    Retime,
    /// Non-trivial apply of a remote changeset (content actually changed).
    SyncMerge,
    /// Explicit resolution of a sync conflict.
    SyncConflictResolve,
    /// Revert to a prior version via `datum undo`.
    Undo,
}

impl ChangeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChangeKind::Create => "create",
            ChangeKind::UserEdit => "user_edit",
            ChangeKind::Retime => "retime",
            ChangeKind::SyncMerge => "sync_merge",
            ChangeKind::SyncConflictResolve => "sync_conflict_resolve",
            ChangeKind::Undo => "undo",
        }
    }
}

impl std::fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ChangeKind {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "create" => Ok(ChangeKind::Create),
            "user_edit" => Ok(ChangeKind::UserEdit),
            "retime" => Ok(ChangeKind::Retime),
            "sync_merge" => Ok(ChangeKind::SyncMerge),
            "sync_conflict_resolve" => Ok(ChangeKind::SyncConflictResolve),
            "undo" => Ok(ChangeKind::Undo),
            other => Err(format!("unknown change_kind: '{other}'")),
        }
    }
}

/// A single row from `entry_history`, with the snapshot rehydrated as an
/// [`Entry`]. The snapshot for version=1 is the newly-created entry; for
/// later versions it is the pre-operation state.
#[derive(Debug, Clone)]
pub struct HistoryVersion {
    pub entry_hid: String,
    pub version: u32,
    pub snapshot: Entry,
    pub modify_time: DateTime<Utc>,
    pub raw_input: Option<String>,
    pub alias_chain: Option<JsonValue>,
    pub device_id: String,
    pub change_kind: ChangeKind,
}

/// Smallest currently-unused version number for the given entry: `last + 1`,
/// or `1` if no rows exist yet.
pub fn next_version(conn: &Connection, entry_hid: &str) -> Result<u32> {
    let max: Option<i64> = conn
        .query_row(
            "SELECT MAX(version) FROM entry_history WHERE entry_hid = ?1",
            params![entry_hid],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(match max {
        Some(n) => (n as u32) + 1,
        None => 1,
    })
}

/// Insert a history row for `snapshot` under the next available version.
/// Returns the stored version. The `executor` parameter lets this run inside
/// a caller-owned transaction (recommended — the companion `entries` mutation
/// should live in the same transaction to keep the trail in lockstep).
pub fn record(executor: &Connection, snapshot: &Entry, change_kind: ChangeKind) -> Result<u32> {
    let version = next_version(executor, &snapshot.hid)?;
    let snapshot_json = serde_json::to_string(snapshot)?;
    let alias_chain_json = snapshot.alias_chain.as_ref().map(|v| v.to_string());
    executor.execute(
        r#"INSERT INTO entry_history (
            entry_hid, version, snapshot, modify_time,
            raw_input, alias_chain, device_id, change_kind
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
        params![
            snapshot.hid,
            version,
            snapshot_json,
            snapshot.modify_time.to_rfc3339(),
            // §5 #11: see `db::insert` for the same bind compat note —
            // empty-string-when-None preserves migration-walk
            // compatibility against historical NOT NULL versions.
            snapshot.raw_input.as_deref().unwrap_or(""),
            alias_chain_json,
            snapshot.device_id,
            change_kind.as_str(),
        ],
    )?;
    Ok(version)
}

/// List every history row for `entry_hid`. When `newest_first` is true the
/// highest version comes first (natural `datum history` ordering); otherwise
/// rows are returned in chronological / version-ascending order.
pub fn list(conn: &Connection, entry_hid: &str, newest_first: bool) -> Result<Vec<HistoryVersion>> {
    let sql = if newest_first {
        "SELECT entry_hid, version, snapshot, modify_time, raw_input,
                alias_chain, device_id, change_kind
         FROM entry_history
         WHERE entry_hid = ?1
         ORDER BY version DESC"
    } else {
        "SELECT entry_hid, version, snapshot, modify_time, raw_input,
                alias_chain, device_id, change_kind
         FROM entry_history
         WHERE entry_hid = ?1
         ORDER BY version ASC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![entry_hid], row_to_version)?;
    rows.map(|r| r.map_err(DatumError::Db)).collect()
}

/// Fetch the row for a specific (entry_hid, version) tuple, if present.
pub fn get(conn: &Connection, entry_hid: &str, version: u32) -> Result<Option<HistoryVersion>> {
    conn.query_row(
        "SELECT entry_hid, version, snapshot, modify_time, raw_input,
                alias_chain, device_id, change_kind
         FROM entry_history
         WHERE entry_hid = ?1 AND version = ?2",
        params![entry_hid, version],
        row_to_version,
    )
    .optional()
    .map_err(DatumError::Db)
}

/// Highest recorded version for `entry_hid`, or `None` if no history exists
/// (pre-D4 rows, or an unknown hid).
pub fn latest_version(conn: &Connection, entry_hid: &str) -> Result<Option<u32>> {
    let max: Option<i64> = conn
        .query_row(
            "SELECT MAX(version) FROM entry_history WHERE entry_hid = ?1",
            params![entry_hid],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(max.map(|n| n as u32))
}

/// Reconstruct the entry state *as of* version N.
///
/// Rule: for version=1 the snapshot itself is the post-create state. For any
/// later version, the state that existed after version N's operation equals
/// version (N+1)'s pre-operation snapshot. When N is the latest recorded
/// version, the live `entries` row (passed in as `live`) holds that state —
/// there's no successor snapshot to consult.
pub fn state_at_version(
    conn: &Connection,
    entry_hid: &str,
    version: u32,
    live: Option<&Entry>,
) -> Result<Option<Entry>> {
    // v1 is the only version where the stored snapshot is the post-op state.
    if version == 1 {
        if let Some(v1) = get(conn, entry_hid, 1)? {
            return Ok(Some(v1.snapshot));
        }
        return Ok(None);
    }
    // For later versions, prefer the next version's snapshot (= post-vN state).
    if let Some(next) = get(conn, entry_hid, version + 1)? {
        return Ok(Some(next.snapshot));
    }
    // No successor recorded — vN is the latest, so the live row holds its state.
    Ok(live.cloned())
}

fn row_to_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryVersion> {
    let snapshot_json: String = row.get("snapshot")?;
    let snapshot: Entry = serde_json::from_str(&snapshot_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let modify_time: DateTime<Utc> =
        row.get::<_, String>("modify_time")?
            .parse()
            .map_err(|e: chrono::ParseError| {
                rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
            })?;
    let alias_chain: Option<JsonValue> = row
        .get::<_, Option<String>>("alias_chain")?
        .and_then(|s| serde_json::from_str(&s).ok());
    let change_kind_str: String = row.get("change_kind")?;
    let change_kind: ChangeKind = change_kind_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;
    Ok(HistoryVersion {
        entry_hid: row.get("entry_hid")?,
        version: row.get::<_, i64>("version")? as u32,
        snapshot,
        modify_time,
        // §5 #11: empty-string sentinel → None at the boundary; mirrors
        // `crate::db::row_mapper::row_to_entry`.
        raw_input: row
            .get::<_, Option<String>>("raw_input")?
            .filter(|s| !s.is_empty()),
        alias_chain,
        device_id: row.get("device_id")?,
        change_kind,
    })
}
