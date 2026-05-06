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
//! - `conflict_kind`: `double_open` | `orphan_end` | `overlap`
//!                    | `same_state_repeat` | `last_state_mismatch`.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::entry::{new_hid, Entry, Verb, SHORT_HID_LEN};
use crate::error::{DatumError, Result};
use crate::ids::Slug;

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
    /// Special case of [`Self::SameStateRepeat`] where the new state is `true`;
    /// retained as a distinct kind for the auto-resolver's close-older rule.
    DoubleOpen,
    /// A `stop`/`end` with no matching prior ongoing entry.
    OrphanEnd,
    /// Two `block`-shaped entries on the same field with overlapping
    /// `[start_time, end_time]` intervals.
    Overlap,
    /// A state-affecting entry whose effective state equals the field's
    /// computed active state immediately before the entry (S6). Generalizes
    /// double-start to arbitrary values — e.g. switching to the same string
    /// twice in a row.
    SameStateRepeat,
    /// An entry carried an explicit `--last-state=<v>` assertion whose
    /// normalized value differs from the state computed from history (S6).
    LastStateMismatch,
}

impl ConflictKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConflictKind::DoubleOpen => "double_open",
            ConflictKind::OrphanEnd => "orphan_end",
            ConflictKind::Overlap => "overlap",
            ConflictKind::SameStateRepeat => "same_state_repeat",
            ConflictKind::LastStateMismatch => "last_state_mismatch",
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
            "same_state_repeat" => Ok(ConflictKind::SameStateRepeat),
            "last_state_mismatch" => Ok(ConflictKind::LastStateMismatch),
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
    /// User chose the local version as authoritative — for sync_pull,
    /// rewrite the entry row to match `local_entry`. For local_validator,
    /// behaves like keep_both (both rows remain; resolution is recorded).
    TakeLocal,
    /// User chose the remote/counterpart version — rewrite the entry row
    /// to match `remote_entry`. No-op when `remote_entry IS NULL`.
    TakeRemote,
    /// User edited a merged version; the supplied entry replaces the
    /// matching row in `entries` (matched by slug).
    Edit,
}

impl Resolution {
    pub fn as_str(&self) -> &'static str {
        match self {
            Resolution::AutoCloseOlder => "auto_close_older",
            Resolution::KeepBoth => "keep_both",
            Resolution::CloseOlder => "close_older",
            Resolution::TakeLocal => "take_local",
            Resolution::TakeRemote => "take_remote",
            Resolution::Edit => "edit",
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
    pub entry_slug: Slug,
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
    // da-2d8: same-state-repeat / last-state-mismatch detection adapted to
    // the new (occur_time, state, dur, following_state, verb) shape.
    // Overlap / double-open / orphan-end remain on the to-do list — the
    // pre-pivot SQL keyed on `entry_shape` and `end_time`, neither of which
    // exists in the v20 entries table, and the current Verb set has no
    // Start/Stop variants. Switch (and the trailing -L mismatch check) is
    // what concurrent_states_cmd / last_state_and_moment exercises today.
    let mut filed = Vec::new();

    if matches!(trigger.verb, Verb::Switch) {
        if let Some(hit) = detect_same_state_repeat(conn, trigger)? {
            filed.push(file_conflict(
                conn,
                trigger,
                Some(&hit),
                detected_by,
                ConflictKind::SameStateRepeat,
            )?);
        }
    }

    if let Some(mismatch) = detect_last_state_mismatch(conn, trigger)? {
        filed.push(file_conflict(
            conn,
            trigger,
            mismatch.as_ref(),
            detected_by,
            ConflictKind::LastStateMismatch,
        )?);
    }

    Ok(filed)
}

/// S6/S12: detect a switch whose normalized value shares at least one
/// element with the field's computed active state immediately before the
/// trigger. Scalar↔scalar, scalar↔array, array↔array overlap all count.
/// Disjoint array→array transitions are clean. Returns the most recent
/// state-defining counterpart entry for display.
fn detect_same_state_repeat(conn: &Connection, trigger: &Entry) -> Result<Option<Entry>> {
    let Some(ts) = &trigger.occur_time else {
        return Ok(None);
    };
    let probe = ts.utc - chrono::Duration::microseconds(1);
    let prior =
        crate::state::last_state_at(conn, &trigger.field, probe, Some(trigger.hid.as_str()))?;
    let Some(prior_state) = prior else {
        return Ok(None);
    };
    let new_norm = trigger
        .state
        .as_ref()
        .map(crate::state::normalize_state)
        .unwrap_or(serde_json::Value::Null);
    if !states_share_element(&new_norm, &prior_state) {
        return Ok(None);
    }
    let at_str = probe.to_rfc3339();
    conn.query_row(
        r#"SELECT * FROM entries
           WHERE field = ?1
             AND deleted_at IS NULL
             AND verb IN ('switch', 'block-switch', 'point-switch')
             AND occur_time IS NOT NULL
             AND occur_time <= ?2
             AND hid != ?3
           ORDER BY occur_time DESC, create_time DESC
           LIMIT 1"#,
        params![trigger.field, at_str, trigger.hid.as_str()],
        crate::db::row_to_entry_public,
    )
    .optional()
    .map_err(DatumError::Db)
}

/// S12: true when two (normalized) states share at least one element. Scalar
/// values are treated as one-element sets; arrays are treated as sets of
/// their elements. Elements compare by canonical JSON serialization so that
/// e.g. `"foo"` and `{"id":"foo"}` (after normalize) still match correctly.
fn states_share_element(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let sa = state_as_set(a);
    let sb = state_as_set(b);
    sa.intersection(&sb).next().is_some()
}

/// S12: set-aware equality — arrays compare order-insensitive (same
/// multiset of elements), everything else compares via `PartialEq`.
fn states_equal_set_aware(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Array(xa), serde_json::Value::Array(xb)) if xa.len() == xb.len() => {
            let mut sa: Vec<String> = xa.iter().map(|x| x.to_string()).collect();
            let mut sb: Vec<String> = xb.iter().map(|x| x.to_string()).collect();
            sa.sort();
            sb.sort();
            sa == sb
        }
        _ => a == b,
    }
}

fn state_as_set(v: &serde_json::Value) -> std::collections::HashSet<String> {
    match v {
        serde_json::Value::Array(arr) => arr.iter().map(|x| x.to_string()).collect(),
        other => std::iter::once(other.to_string()).collect(),
    }
}

/// S6/S12: compare the user's explicit `-L` assertion against the computed
/// last-state. `Ok(None)` when there's no assertion or the assertion
/// matches; `Ok(Some(_))` on mismatch — the inner Option is a counterpart
/// entry for display (may be None if no prior entry exists).
fn detect_last_state_mismatch(conn: &Connection, trigger: &Entry) -> Result<Option<Option<Entry>>> {
    let Some(asserted_raw) = trigger.user_asserted_last_state.as_ref() else {
        return Ok(None);
    };
    let Some(ts) = &trigger.occur_time else {
        return Ok(None);
    };
    let probe = ts.utc - chrono::Duration::microseconds(1);
    let asserted = crate::state::normalize_state(asserted_raw);
    let actual =
        crate::state::last_state_at(conn, &trigger.field, probe, Some(trigger.hid.as_str()))?
            .unwrap_or(serde_json::Value::Null);
    if states_equal_set_aware(&asserted, &actual) {
        return Ok(None);
    }
    let at_str = probe.to_rfc3339();
    let counterpart = conn
        .query_row(
            r#"SELECT * FROM entries
               WHERE field = ?1
                 AND deleted_at IS NULL
                 AND verb IN ('switch', 'block', 'block-switch', 'point-switch')
                 AND occur_time IS NOT NULL
                 AND occur_time <= ?2
                 AND hid != ?3
               ORDER BY occur_time DESC, create_time DESC
               LIMIT 1"#,
            params![trigger.field, at_str, trigger.hid.as_str()],
            crate::db::row_to_entry_public,
        )
        .optional()
        .map_err(DatumError::Db)?;
    Ok(Some(counterpart))
}

fn file_conflict(
    conn: &Connection,
    trigger: &Entry,
    counterpart: Option<&Entry>,
    detected_by: DetectedBy,
    kind: ConflictKind,
) -> Result<Conflict> {
    let id: String = new_hid().into_string();
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
    let params_slice: Vec<&dyn rusqlite::ToSql> =
        binds.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
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
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let detected_at: DateTime<Utc> =
        row.get::<_, String>("detected_at")?
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
    _conn: &Connection,
    _now: DateTime<Utc>,
    _min_age_secs: i64,
) -> Result<Vec<Conflict>> {
    // da-5ns sub-bead 1 stub: detection is disabled (see
    // `detect_for_entry`), so there is nothing to auto-resolve. Sub-bead
    // 5 reintroduces both detection and the matching auto-resolution
    // rules under the new (occur_time, dur, state, following_state)
    // shape.
    Ok(vec![])
}

/// Stub — see `auto_resolve_pending`. Kept on the public surface so
/// `Db::resolve_conflict` keeps compiling; sub-bead 5 reimplements.
pub(crate) fn close_older_at(
    _conn: &Connection,
    _older: &Entry,
    _at: Option<&crate::time::DatumTime>,
) -> Result<()> {
    Ok(())
}

/// Count of live "open" entries for `field`. Pre-pivot this counted
/// ongoing blocks (entry_shape='ongoing' AND end_time IS NULL); under
/// the new shape an "open" event is a switch with no matching trailing
/// switch — tracking that requires the state-compilation work that
/// sub-bead 5 lands. Until then this returns 0; the only caller is the
/// `datum now` overlap banner which will go silent until detection
/// comes back.
pub fn open_entries_for_field(_conn: &Connection, _field: &str) -> Result<u32> {
    Ok(0)
}
