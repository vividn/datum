//! Non-blocking data-integrity warnings queue (`integrity_warnings` table).
//!
//! D5 principle: the system never refuses data entry. When a user logs
//! something inconsistent on this device — a `start` on a field that's
//! already open, a `stop` for a field with no open block, two overlapping
//! `block`s — both entries land in the `entries` table and a pending row
//! is filed here for later review.
//!
//! These are *local data-integrity* findings, NOT cross-device sync
//! conflicts. A real sync-conflict (same entry edited differently on two
//! devices) will live in a separate table when that surface lands.
//!
//! Detection runs post-insert (see [`detect_for_entry`]) and the queue
//! is reported by `datum check`. Warnings clear naturally when the user
//! fixes the underlying data — the check re-runs against current state,
//! and a now-resolved warning no longer fires. [`auto_resolve_pending`]
//! remains as an internal helper for callers that need to clear obvious
//! cases (e.g. tests, future surfaces) once enough time has passed.
//!
//! This module is also the backing store for C4e's sync-pull integrity
//! detection — incoming remote entries are walked through the same
//! `detect_for_entry` and any inconsistencies file rows with
//! `detected_by = sync_pull`.
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
//! - `kind`: `double_open` | `orphan_end` | `overlap`
//!   | `same_state_repeat` | `last_state_mismatch` | `lww_overwrite`.
//!   `lww_overwrite` rows are filed by the sync-apply path when an
//!   incoming row wins last-write-wins over an existing local row;
//!   `local_entry` is the overwritten edit, `remote_entry` the winner.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::entry::{new_hid, Entry, Verb, SHORT_HID_LEN};
use crate::error::{DatumError, Result};
use crate::ids::Slug;
use crate::state::StateValue;

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
pub enum IntegrityKind {
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
    /// A last-write-wins sync merge overwrote a local edit: an incoming
    /// remote row with a newer `modify_time` replaced the existing local
    /// row for the same slug. `local_entry` is the overwritten local edit,
    /// `remote_entry` the winning remote edit. The loser is also preserved
    /// in `entry_history` as `sync_merge`; this row surfaces the overwrite
    /// to the user (C4e) instead of leaving it silent. Only ever filed with
    /// `detected_by = sync_pull` (da-t96a).
    LwwOverwrite,
    /// Two *distinct* entries (different `hid`) claim one identity the
    /// `entries` table can only give to one row: the same `slug`, or the
    /// same live `(field, occur_time)`. Since da-zlug the sync apply merges
    /// on `hid`, so these are genuinely two rows rather than two versions
    /// of one — both are kept, the deterministic loser is parked (slug
    /// suffixed with its own hid, and/or tombstoned when the pair cannot
    /// both be live), and this row names the pair. `local_entry` is the
    /// parked loser, `remote_entry` the row that kept the identity. Only
    /// ever filed with `detected_by = sync_pull`.
    IdentityCollision,
}

impl IntegrityKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            IntegrityKind::DoubleOpen => "double_open",
            IntegrityKind::OrphanEnd => "orphan_end",
            IntegrityKind::Overlap => "overlap",
            IntegrityKind::SameStateRepeat => "same_state_repeat",
            IntegrityKind::LastStateMismatch => "last_state_mismatch",
            IntegrityKind::LwwOverwrite => "lww_overwrite",
            IntegrityKind::IdentityCollision => "identity_collision",
        }
    }
}

impl std::fmt::Display for IntegrityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for IntegrityKind {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "double_open" => Ok(IntegrityKind::DoubleOpen),
            "orphan_end" => Ok(IntegrityKind::OrphanEnd),
            "overlap" => Ok(IntegrityKind::Overlap),
            "same_state_repeat" => Ok(IntegrityKind::SameStateRepeat),
            "last_state_mismatch" => Ok(IntegrityKind::LastStateMismatch),
            "lww_overwrite" => Ok(IntegrityKind::LwwOverwrite),
            "identity_collision" => Ok(IntegrityKind::IdentityCollision),
            other => Err(format!("unknown kind: '{other}'")),
        }
    }
}

/// Resolution labels written to `integrity_warnings.resolution` when a row is
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

/// One row from `integrity_warnings` rehydrated for callers.
#[derive(Debug, Clone)]
pub struct IntegrityWarning {
    pub id: String,
    pub hid: String,
    pub entry_slug: Slug,
    pub local_entry: Entry,
    pub remote_entry: Option<Entry>,
    pub detected_at: DateTime<Utc>,
    pub resolution: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub detected_by: DetectedBy,
    pub kind: IntegrityKind,
}

impl IntegrityWarning {
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
) -> Result<Vec<IntegrityWarning>> {
    detect_for_entries(conn, std::slice::from_ref(trigger), detected_by)
}

/// Batched [`detect_for_entry`]: same rules, same results, but the
/// per-field history each rule consults is read **once for the whole
/// batch** instead of once per trigger (da-j8cu).
///
/// The per-trigger form asks two full-field questions — "what was the
/// state just before this row?" and "does this block overlap another?"
/// — and each one reads the field's entire history. Run in a loop over
/// N incoming rows that is O(N²): a sync pull of a real history stops
/// looking slow and starts looking hung. Here the field snapshots are
/// loaded lazily on first use and reused, so a batch costs one read per
/// touched field. Measured on one field of switches (release):
/// 2k triggers 11.5s → 8ms, 4k triggers 46.4s → 21ms.
///
/// The snapshots are taken when a field is first seen and are NOT
/// refreshed as the batch proceeds, so **every trigger must already be
/// in the `entries` table before the call** — the same post-insert
/// contract [`detect_for_entry`] has, applied to the whole slice. That
/// snapshot is also what makes the batch order-independent: an
/// out-of-order arrival sees the same history as its neighbours.
pub fn detect_for_entries(
    conn: &Connection,
    triggers: &[Entry],
    detected_by: DetectedBy,
) -> Result<Vec<IntegrityWarning>> {
    let mut snaps = FieldSnapshots::default();
    let mut filed = Vec::new();
    for trigger in triggers {
        detect_one(conn, trigger, detected_by, &mut snaps, &mut filed)?;
    }
    Ok(filed)
}

/// Per-field history read once and reused across a batch of detections.
/// Both maps are lazily filled: a batch that carries no `Block` pays for
/// no block snapshot, and a field nobody triggers on is never read.
#[derive(Default)]
struct FieldSnapshots {
    /// State-transition events per field — the cheap form of the
    /// `last_state_at` question (fewer columns, no `Entry` materialisation).
    states: std::collections::HashMap<String, crate::state::FieldStateEvents>,
    /// Live Block / BlockSwitch rows per field, in canonical oldest-first
    /// order, for the overlap scan.
    blocks: std::collections::HashMap<String, Vec<Entry>>,
}

impl FieldSnapshots {
    /// Compiled state of `field` at `probe`, ignoring `exclude_hid` —
    /// the snapshot-backed equivalent of [`crate::state::last_state_at`].
    fn state_at(
        &mut self,
        conn: &Connection,
        field: &str,
        probe: DateTime<Utc>,
        exclude_hid: &str,
    ) -> Result<Option<serde_json::Value>> {
        if !self.states.contains_key(field) {
            let events = crate::state::FieldStateEvents::load_for_field(conn, field)?;
            self.states.insert(field.to_string(), events);
        }
        Ok(self.states[field].at(probe, Some(exclude_hid)))
    }

    fn blocks(&mut self, conn: &Connection, field: &str) -> Result<&[Entry]> {
        if !self.blocks.contains_key(field) {
            self.blocks
                .insert(field.to_string(), load_blocks_for_field(conn, field)?);
        }
        Ok(&self.blocks[field])
    }
}

fn detect_one(
    conn: &Connection,
    trigger: &Entry,
    detected_by: DetectedBy,
    snaps: &mut FieldSnapshots,
    filed: &mut Vec<IntegrityWarning>,
) -> Result<()> {
    // da-nm5b: detection under v3 verbs.
    // - Switch with state=Bool(true) on a field whose pre-trigger state is
    //   also Bool(true) → DoubleOpen.
    // - Switch with state=Bool(false) on a field with no prior history
    //   → OrphanEnd (no counterpart).
    // - Switch where (normalized) state shares an element with the prior
    //   state but is not the open/open case → SameStateRepeat.
    // - Block / BlockSwitch whose [occur, occur+dur] interval overlaps an
    //   existing Block/BlockSwitch on the same field → Overlap.
    // - LastStateMismatch (S6) is independent of the above.

    // Deliberately a local verb match, not a shape.rs predicate: detection
    // rules key on per-verb-family STORAGE semantics, and a new verb should
    // conservatively get no detection until a rule is written for it.
    if matches!(trigger.verb, Verb::Switch) {
        let new_norm = trigger
            .state
            .as_ref()
            .map(|v| crate::state::normalize_state(v).to_json())
            .unwrap_or(serde_json::Value::Null);

        let prior_at_probe = if let Some(ts) = trigger.occur_time.as_ref() {
            let probe = ts.utc - chrono::Duration::microseconds(1);
            snaps.state_at(conn, &trigger.field, probe, trigger.hid.as_str())?
        } else {
            None
        };

        // OrphanEnd: a state=false transition arriving at a field that has
        // never been opened.
        if is_close_state(&new_norm) && prior_at_probe.is_none() {
            filed.push(file_warning(
                conn,
                trigger,
                None,
                detected_by,
                IntegrityKind::OrphanEnd,
            )?);
        } else if let Some(prior) = prior_at_probe.as_ref() {
            // da-acft: only DoubleOpen (both states normalized to
            // `true`) is stored at insert. Other state-share patterns
            // — same string, partial parallel overlap — are surfaced
            // live by [`list_repeated_states`] so the warning clears
            // automatically once the user fixes the underlying data.
            if is_open_state(&new_norm)
                && is_open_state(prior)
                && states_share_element(&new_norm, prior)
            {
                if let Some(hit) = find_recent_state_setter(conn, trigger)? {
                    filed.push(file_warning(
                        conn,
                        trigger,
                        Some(&hit),
                        detected_by,
                        IntegrityKind::DoubleOpen,
                    )?);
                }
            }
        }
    }

    // Local on purpose: overlap is a dur-interval storage concern, not a
    // render decision — coupling it to shape.rs predicates would invert
    // the dependency direction.
    if matches!(trigger.verb, Verb::Block | Verb::BlockSwitch) {
        let overlap = find_overlapping_block(snaps.blocks(conn, &trigger.field)?, trigger).cloned();
        if let Some(other) = overlap {
            filed.push(file_warning(
                conn,
                trigger,
                Some(&other),
                detected_by,
                IntegrityKind::Overlap,
            )?);
        }
    }

    if let Some(mismatch) = detect_last_state_mismatch(conn, trigger, snaps)? {
        filed.push(file_warning(
            conn,
            trigger,
            mismatch.as_ref(),
            detected_by,
            IntegrityKind::LastStateMismatch,
        )?);
    }

    Ok(())
}

/// da-acft: walk live state-bearing entries in `occur_time` order per
/// field and return each one whose normalized state shares an element
/// with its predecessor on the same field. Excludes the open/open
/// case — that's [`IntegrityKind::DoubleOpen`], still filed at insert
/// because the auto-resolver acts on it.
///
/// This is a live computation: nothing is stored, and a user who
/// fixes the underlying data (backfill missing transition, delete
/// duplicate) stops seeing the row on the next call. The previous
/// model — a stored row written by [`detect_for_entry`] — left stale
/// warnings sitting in `integrity_warnings` long after the data was
/// repaired, because there was no auto-resolution rule for the kind.
///
/// Walks `entries_live_field_occur` (partial index on `(field,
/// occur_time) WHERE deleted_at IS NULL`) via SQLite's window
/// function. Per-row state JSON is parsed back into Rust so we can
/// reuse the same [`states_share_element`] predicate the original
/// detector used; this preserves the partial-overlap semantics for
/// parallel/structured states.
pub fn list_repeated_states(conn: &Connection) -> Result<Vec<Entry>> {
    let sql = format!(
        "WITH state_runs AS (
             SELECT entries.*,
                    LAG(state) OVER (
                        PARTITION BY field
                        ORDER BY {order}
                    ) AS prior_state
             FROM entries
             WHERE deleted_at IS NULL
               AND state IS NOT NULL
               AND occur_time IS NOT NULL
         )
         SELECT * FROM state_runs
         WHERE prior_state IS NOT NULL
         ORDER BY {order}",
        order = crate::db::STATE_ORDER_OLDEST_FIRST,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let entry = crate::db::row_to_entry_public(row)?;
        let prior_state: String = row.get("prior_state")?;
        Ok((entry, prior_state))
    })?;
    let mut hits = Vec::new();
    for row in rows {
        let (entry, prior_state_json) = row.map_err(DatumError::Db)?;
        let Some(new_state) = entry.state.as_ref() else {
            continue;
        };
        let Ok(prior_state) = serde_json::from_str::<serde_json::Value>(&prior_state_json) else {
            continue;
        };
        let new_norm = crate::state::normalize_state(new_state).to_json();
        let prior_norm = crate::state::normalize_state(&prior_state).to_json();
        if !states_share_element(&new_norm, &prior_norm) {
            continue;
        }
        if is_open_state(&new_norm) && is_open_state(&prior_norm) {
            continue;
        }
        hits.push(entry);
    }
    Ok(hits)
}

/// State value indicating "field is open" — the canonical shorthand
/// produced by `datum start` after [`crate::verb::derive_verb`] runs.
fn is_open_state(v: &serde_json::Value) -> bool {
    matches!(v, serde_json::Value::Bool(true))
}

/// State value indicating "field is closed" — the canonical shorthand
/// produced by `datum stop`.
fn is_close_state(v: &serde_json::Value) -> bool {
    matches!(v, serde_json::Value::Bool(false))
}

/// Look up the most recent state-setting entry strictly before the
/// trigger's `occur_time`. Used as the `remote_entry` snapshot on
/// DoubleOpen / SameStateRepeat conflicts so the resolver UI can show
/// "this conflicts with that one over there".
fn find_recent_state_setter(conn: &Connection, trigger: &Entry) -> Result<Option<Entry>> {
    let Some(ts) = &trigger.occur_time else {
        return Ok(None);
    };
    let at_str = ts.utc.to_rfc3339();
    // Verb list is a SHAPE filter (state-setting entry kinds), not the
    // state-participation rule; the ordering is the canonical one (da-ylet).
    let sql = format!(
        "SELECT * FROM entries
         WHERE field = ?1
           AND deleted_at IS NULL
           AND verb IN ('switch', 'block-switch', 'point-switch')
           AND occur_time IS NOT NULL
           AND occur_time < ?2
           AND hid != ?3
         ORDER BY {order}
         LIMIT 1",
        order = crate::db::STATE_ORDER_NEWEST_FIRST,
    );
    conn.query_row(
        &sql,
        params![trigger.field, at_str, trigger.hid.as_str()],
        crate::db::row_to_entry_public,
    )
    .optional()
    .map_err(DatumError::Db)
}

/// Every live Block / BlockSwitch on `field` that carries an interval, in
/// canonical oldest-first order. Read once per field per batch and scanned
/// in memory by [`find_overlapping_block`]; the trigger's own row is
/// filtered there (by hid) rather than here, so one snapshot serves every
/// trigger on the field.
fn load_blocks_for_field(conn: &Connection, field: &str) -> Result<Vec<Entry>> {
    let sql = format!(
        "SELECT * FROM entries
         WHERE field = ?1
           AND deleted_at IS NULL
           AND verb IN ('block', 'block-switch')
           AND occur_time IS NOT NULL
           AND dur IS NOT NULL
         ORDER BY {order}",
        order = crate::db::STATE_ORDER_OLDEST_FIRST,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![field], crate::db::row_to_entry_public)?;
    rows.map(|r| r.map_err(DatumError::Db)).collect()
}

/// Find a live Block / BlockSwitch on the same field whose
/// `[occur, occur+dur]` interval overlaps the trigger's. Touching
/// boundaries (one ends exactly when the other starts) do NOT overlap —
/// a back-to-back schedule is legitimate. Returns the first hit; the
/// caller files exactly one Overlap row per trigger.
fn find_overlapping_block<'a>(blocks: &'a [Entry], trigger: &Entry) -> Option<&'a Entry> {
    let occur = trigger.occur_time.as_ref()?;
    let secs = trigger.dur.as_ref()?.as_seconds()?;
    let trig_start = occur.utc;
    let trig_end = occur.utc + chrono::Duration::seconds(secs);

    blocks.iter().find(|other| {
        if other.hid == trigger.hid {
            return false;
        }
        let (Some(o_occur), Some(o_dur)) = (other.occur_time.as_ref(), other.dur.as_ref()) else {
            return false;
        };
        let Some(o_secs) = o_dur.as_seconds() else {
            return false;
        };
        let o_start = o_occur.utc;
        let o_end = o_occur.utc + chrono::Duration::seconds(o_secs);
        trig_start < o_end && o_start < trig_end
    })
}

/// S12: true when two (normalized) states share at least one element. Scalar
/// values are treated as one-element sets; `Parallel` values are treated as
/// sets of their elements. Elements compare via [`StateValue`] structural
/// equality, so a `Structured({id: "car", role: "driver"})` matches another
/// `Structured` only when every field agrees — `.id` alone is not enough.
///
/// Exported so the renderer can flag a repeated-state switch (`da-qfv0`)
/// using the same predicate that drives `SameStateRepeat` warning records.
pub fn states_share_element(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let sa = state_as_set(a);
    let sb = state_as_set(b);
    sa.iter().any(|x| sb.iter().any(|y| x == y))
}

/// S12: set-aware equality on top of [`StateValue`] PartialEq — `Parallel`
/// values compare order-insensitive (same multiset of elements), everything
/// else compares structurally.
fn states_equal_set_aware(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    let Ok(sva) = StateValue::from_json(a) else {
        return false;
    };
    let Ok(svb) = StateValue::from_json(b) else {
        return false;
    };
    match (&sva, &svb) {
        (StateValue::Parallel(va), StateValue::Parallel(vb)) => state_multiset_eq(va, vb),
        _ => sva == svb,
    }
}

/// Decompose a state into its element multiset for set-aware comparisons.
/// `Parallel` flattens to its members; everything else is a one-element set.
/// On a malformed value (none should reach here in practice — the upstream
/// shape is already a valid JSON state) the fallback is an empty set so the
/// element-share / equality predicates short-circuit to "no match".
fn state_as_set(v: &serde_json::Value) -> Vec<StateValue> {
    match StateValue::from_json(v) {
        Ok(StateValue::Parallel(items)) => items,
        Ok(other) => vec![other],
        Err(_) => Vec::new(),
    }
}

/// Multiset equality for two `Parallel` element vectors. Ours are tiny
/// (a handful of states), so the O(n²) match-and-mark walk is fine and
/// avoids needing `Hash` on `StateValue`.
fn state_multiset_eq(va: &[StateValue], vb: &[StateValue]) -> bool {
    if va.len() != vb.len() {
        return false;
    }
    let mut used = vec![false; vb.len()];
    'outer: for x in va {
        for (i, y) in vb.iter().enumerate() {
            if !used[i] && x == y {
                used[i] = true;
                continue 'outer;
            }
        }
        return false;
    }
    true
}

/// S6/S12: compare the user's explicit `-L` assertion against the computed
/// last-state. `Ok(None)` when there's no assertion or the assertion
/// matches; `Ok(Some(_))` on mismatch — the inner Option is a counterpart
/// entry for display (may be None if no prior entry exists).
fn detect_last_state_mismatch(
    conn: &Connection,
    trigger: &Entry,
    snaps: &mut FieldSnapshots,
) -> Result<Option<Option<Entry>>> {
    let Some(asserted_raw) = trigger.user_asserted_last_state.as_ref() else {
        return Ok(None);
    };
    let Some(ts) = &trigger.occur_time else {
        return Ok(None);
    };
    let probe = ts.utc - chrono::Duration::microseconds(1);
    let asserted = crate::state::normalize_state(asserted_raw).to_json();
    let actual = snaps
        .state_at(conn, &trigger.field, probe, trigger.hid.as_str())?
        .unwrap_or(serde_json::Value::Null);
    if states_equal_set_aware(&asserted, &actual) {
        return Ok(None);
    }
    let at_str = probe.to_rfc3339();
    let sql = format!(
        "SELECT * FROM entries
         WHERE field = ?1
           AND deleted_at IS NULL
           AND verb IN ('switch', 'block', 'block-switch', 'point-switch')
           AND occur_time IS NOT NULL
           AND occur_time <= ?2
           AND hid != ?3
         ORDER BY {order}
         LIMIT 1",
        order = crate::db::STATE_ORDER_NEWEST_FIRST,
    );
    let counterpart = conn
        .query_row(
            &sql,
            params![trigger.field, at_str, trigger.hid.as_str()],
            crate::db::row_to_entry_public,
        )
        .optional()
        .map_err(DatumError::Db)?;
    Ok(Some(counterpart))
}

fn file_warning(
    conn: &Connection,
    trigger: &Entry,
    counterpart: Option<&Entry>,
    detected_by: DetectedBy,
    kind: IntegrityKind,
) -> Result<IntegrityWarning> {
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
        r#"INSERT INTO integrity_warnings (
            id, hid, entry_slug, local_entry, remote_entry,
            detected_at, resolution, resolved_at, detected_by, kind
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
    Ok(IntegrityWarning {
        id,
        hid,
        entry_slug: trigger.slug.clone(),
        local_entry: trigger.clone(),
        remote_entry: counterpart.cloned(),
        detected_at: now,
        resolution: None,
        resolved_at: None,
        detected_by,
        kind,
    })
}

/// File an [`IntegrityKind::LwwOverwrite`] warning: an incoming sync row
/// (`winner`, newer `modify_time`) replaced an existing local edit
/// (`loser`) under last-write-wins. `local_entry` becomes the overwritten
/// local edit (so a later `take_local` resolution can restore it) and
/// `remote_entry` the winning remote edit. Always tagged `sync_pull` — LWW
/// overwrites only occur on apply. The loser is also snapshotted to
/// `entry_history` as `sync_merge` by the apply path; this row is the
/// user-facing counterpart so the overwrite is surfaced, not silent
/// (da-t96a). Call from inside the apply transaction so the warning lands
/// atomically with the overwrite.
pub fn file_lww_overwrite(
    conn: &Connection,
    loser: &Entry,
    winner: &Entry,
) -> Result<IntegrityWarning> {
    file_warning(
        conn,
        loser,
        Some(winner),
        DetectedBy::SyncPull,
        IntegrityKind::LwwOverwrite,
    )
}

/// File an [`IntegrityKind::IdentityCollision`] warning: two distinct
/// entries claimed one `slug` (or one live `(field, occur_time)`) during a
/// sync apply. `parked` is the deterministic loser as it now stands —
/// suffixed slug, tombstoned if the pair could not both stay live — and
/// `kept` the row that holds the contested identity. Both rows survive;
/// this row is what tells the user they need to decide between them
/// (da-zlug; the merge policy itself is da-yiq9). Call from inside the
/// apply transaction so the warning lands atomically with the parking.
pub fn file_identity_collision(
    conn: &Connection,
    parked: &Entry,
    kept: &Entry,
) -> Result<IntegrityWarning> {
    file_warning(
        conn,
        parked,
        Some(kept),
        DetectedBy::SyncPull,
        IntegrityKind::IdentityCollision,
    )
}

/// Filter for [`list`]. `None` matches any row of that axis.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// Only rows of this conflict kind.
    pub kind: Option<IntegrityKind>,
    /// `Some(true)` → pending only (resolution IS NULL).
    /// `Some(false)` → resolved only.
    /// `None` → both.
    pub pending: Option<bool>,
}

/// Read rows from `integrity_warnings`, newest first.
pub fn list(conn: &Connection, filter: &ListFilter) -> Result<Vec<IntegrityWarning>> {
    let mut sql = String::from(
        "SELECT id, hid, entry_slug, local_entry, remote_entry, detected_at,
                resolution, resolved_at, detected_by, kind
         FROM integrity_warnings",
    );
    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    // da-sidz: a warning's trigger entry (`entry_slug`) may since have been
    // soft-deleted. Once the row is tombstoned the integrity problem no
    // longer exists in the live dataset, so suppress its stored warning —
    // otherwise `datum check` keeps reporting (e.g.) a double-open after the
    // offending entry was deleted. Resolution by id still works: it goes
    // through `get_by_hid_or_id`, which is unfiltered.
    //
    // `identity_collision` is exempt (da-zlug): its trigger is the row the
    // sync apply *parked*, and parking tombstones the loser when the pair
    // cannot both be live. There the tombstone is the finding, not evidence
    // it went away — suppressing it would hide every collision it reports.
    clauses.push(format!(
        "(kind = '{}' OR \
          entry_slug NOT IN (SELECT slug FROM entries WHERE deleted_at IS NOT NULL))",
        IntegrityKind::IdentityCollision.as_str(),
    ));
    if let Some(kind) = filter.kind {
        clauses.push(format!("kind = ?{}", binds.len() + 1));
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
    let rows = stmt.query_map(params_slice.as_slice(), row_to_warning)?;
    rows.map(|r| r.map_err(DatumError::Db)).collect()
}

/// Fetch a single conflict by its hid or full id. Prefix match on `hid`
/// so the user can type the 5-char form.
pub fn get_by_hid_or_id(conn: &Connection, token: &str) -> Result<Option<IntegrityWarning>> {
    conn.query_row(
        "SELECT id, hid, entry_slug, local_entry, remote_entry, detected_at,
                resolution, resolved_at, detected_by, kind
         FROM integrity_warnings
         WHERE id = ?1 OR hid = ?1
         LIMIT 1",
        params![token],
        row_to_warning,
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
        r#"UPDATE integrity_warnings
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

fn row_to_warning(row: &rusqlite::Row<'_>) -> rusqlite::Result<IntegrityWarning> {
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
    let kind_str: String = row.get("kind")?;
    let kind: IntegrityKind = kind_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;
    Ok(IntegrityWarning {
        id: row.get("id")?,
        hid: row.get("hid")?,
        entry_slug: row.get("entry_slug")?,
        local_entry,
        remote_entry,
        detected_at,
        resolution: row.get("resolution")?,
        resolved_at,
        detected_by,
        kind,
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
///   `occur_time`. Records a `user_edit` on the older entry and marks
///   the conflict `auto_close_older`.
/// - `OrphanEnd` / `Overlap` / `SameStateRepeat` / `LastStateMismatch`:
///   no auto-rule. The user must resolve manually — either of those
///   patterns can be deliberate (e.g. logging a stop on a field tracked
///   only via stops, or two adjacent gym sessions).
pub fn auto_resolve_pending(
    conn: &Connection,
    now: DateTime<Utc>,
    min_age_secs: i64,
) -> Result<Vec<IntegrityWarning>> {
    let cutoff = now - chrono::Duration::seconds(min_age_secs.max(0));
    let pending = list(
        conn,
        &ListFilter {
            kind: Some(IntegrityKind::DoubleOpen),
            pending: Some(true),
        },
    )?;

    let mut applied = Vec::new();
    for c in pending {
        if c.detected_at > cutoff {
            continue;
        }
        let Some(older) = c.remote_entry.as_ref() else {
            continue;
        };
        close_older_at(conn, older, c.local_entry.occur_time.as_ref())?;
        mark_resolved(conn, &c.id, Resolution::AutoCloseOlder, now)?;
        applied.push(get_by_hid_or_id(conn, &c.id)?.expect("just-resolved row"));
    }
    Ok(applied)
}

/// Close `older` at `at` by setting its `dur` to `at − older.occur_time`
/// (clamped non-negative) and re-deriving its verb to `Block`. Records
/// the pre-update state in `entry_history` so the resolution is
/// reversible. Silently no-ops when `at` or `older.occur_time` is
/// missing — the caller's resolver still marks the conflict resolved.
pub(crate) fn close_older_at(
    conn: &Connection,
    older: &Entry,
    at: Option<&crate::time::DatumTime>,
) -> Result<()> {
    let Some(end) = at else {
        return Ok(());
    };
    let Some(start) = older.occur_time.as_ref() else {
        return Ok(());
    };
    let secs = (end.utc - start.utc).num_seconds().max(0);
    let new_dur = crate::dur::Dur::Of(chrono::Duration::seconds(secs));

    let mut updated = older.clone();
    updated.dur = Some(new_dur);
    updated.verb = if secs == 0 { Verb::Point } else { Verb::Block };
    updated.modify_time = Utc::now();
    updated.content_hash = updated.expected_content_hash();

    crate::history::record(conn, older, crate::history::ChangeKind::UserEdit)?;

    conn.execute(
        r#"UPDATE entries SET
            verb = ?2,
            dur = ?3,
            content_hash = ?4,
            modify_time = ?5
           WHERE slug = ?1 AND deleted_at IS NULL"#,
        params![
            older.slug,
            updated.verb.to_string(),
            updated.dur.as_ref().map(crate::dur::Dur::to_storage_string),
            updated.content_hash,
            updated.modify_time.to_rfc3339(),
        ],
    )?;
    Ok(())
}

/// Count of live "open" entries on `field` — Switch state=Bool(true)
/// transitions that haven't been balanced by a Switch state=Bool(false).
/// The result is `max(0, opens − closes)`. The `datum now` view uses
/// this to decide whether to print the overlap banner.
pub fn open_entries_for_field(conn: &Connection, field: &str) -> Result<u32> {
    let mut stmt = conn.prepare(
        r#"SELECT state FROM entries
           WHERE field = ?1
             AND deleted_at IS NULL
             AND verb = 'switch'
             AND state IS NOT NULL"#,
    )?;
    let rows = stmt.query_map(params![field], |row| row.get::<_, String>(0))?;

    let mut opens: i64 = 0;
    let mut closes: i64 = 0;
    for row in rows {
        let raw = row.map_err(DatumError::Db)?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let normalized = crate::state::normalize_state(&value).to_json();
        if is_open_state(&normalized) {
            opens += 1;
        } else if is_close_state(&normalized) {
            closes += 1;
        }
    }
    Ok((opens - closes).max(0) as u32)
}

#[cfg(test)]
mod tests {
    //! D5 invariants under v3 verbs (da-nm5b). Ported from the
    //! `#[cfg(any())]` module deleted in fdb2851 — the legacy
    //! `Verb::Start` / `Verb::Stop` / `Verb::Block` are gone, so the
    //! "start" pattern is now `Switch` + `state=Bool(true)`, "stop" is
    //! `Switch` + `state=Bool(false)`, and a closed block carries a
    //! `dur` so `derive_verb` storage-canonicalizes it as `block`.
    use super::*;
    use crate::db::Db;
    use crate::test_util::entry_builder;
    use crate::time::DatumTime;
    use chrono::{Duration, Utc};
    use serde_json::json;

    fn ts(seconds_ago: i64) -> DatumTime {
        DatumTime {
            utc: Utc::now() - Duration::seconds(seconds_ago),
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        }
    }

    /// The invariant D5 protects: every insert lands, even patterns
    /// that violate "normal" state consistency (double-open, orphan
    /// stop, overlapping blocks). The post-write hook may file
    /// conflict rows, but it MUST NOT refuse the insert.
    #[test]
    fn insert_always_succeeds_regardless_of_state_consistency() {
        let db = Db::open_in_memory().unwrap();

        // Double-open: two state=true switches on the same field.
        entry_builder("workout")
            .state(json!(true))
            .time_start(ts(300))
            .insert(&db);
        entry_builder("workout")
            .state(json!(true))
            .time_start(ts(60))
            .insert(&db);

        // Orphan stop: a state=false switch on a never-opened field.
        entry_builder("cooking")
            .state(json!(false))
            .time_start(ts(120))
            .insert(&db);

        // Overlapping blocks: two intervals on the same field that
        // share time. Both must land regardless.
        entry_builder("meeting")
            .state(json!(true))
            .time_start(ts(600))
            .time_end(ts(300))
            .insert(&db);
        entry_builder("meeting")
            .state(json!(true))
            .time_start(ts(500))
            .time_end(ts(200))
            .insert(&db);

        // All five entries should be present in the table.
        let entries = db.list_including_deleted().unwrap();
        let workout = entries.iter().filter(|e| e.field == "workout").count();
        let cooking = entries.iter().filter(|e| e.field == "cooking").count();
        let meeting = entries.iter().filter(|e| e.field == "meeting").count();
        assert_eq!(workout, 2, "both double-opens land");
        assert_eq!(cooking, 1, "orphan stop lands");
        assert_eq!(meeting, 2, "both overlapping blocks land");
    }

    /// Writing an overlapping block files exactly one `overlap`
    /// conflict via the post-write hook. The original insert still
    /// succeeds — only the conflict queue is touched.
    #[test]
    fn post_write_hook_detects_overlap() {
        let db = Db::open_in_memory().unwrap();
        entry_builder("meeting")
            .state(json!(true))
            .time_start(ts(600))
            .time_end(ts(300))
            .insert(&db);
        entry_builder("meeting")
            .state(json!(true))
            .time_start(ts(500))
            .time_end(ts(200))
            .insert(&db);

        let rows = db
            .list_integrity_warnings(&ListFilter {
                kind: Some(IntegrityKind::Overlap),
                pending: Some(true),
            })
            .unwrap();
        assert_eq!(rows.len(), 1, "exactly one overlap conflict filed");
        let c = &rows[0];
        assert_eq!(c.kind, IntegrityKind::Overlap);
        assert_eq!(c.detected_by, DetectedBy::LocalValidator);
        assert!(
            c.remote_entry.is_some(),
            "overlap carries the older block as counterpart"
        );
    }

    // ── da-ju71.5: full-StateValue equality for repeated-state checks ─────

    /// Two `Structured` states that share `.id` but differ on extras must
    /// NOT count as the same state — `drive car` and `ride car` are
    /// different things even though both name `car`.
    #[test]
    fn structured_states_with_same_id_different_extras_dont_share() {
        let drive = json!({"id": "car", "role": "driver"});
        let ride = json!({"id": "car", "role": "passenger"});
        assert!(
            !states_share_element(&drive, &ride),
            "structured states with same .id but different extras must not share",
        );
        assert!(
            !states_equal_set_aware(&drive, &ride),
            "structured states with same .id but different extras must not be equal",
        );
    }

    /// The mirror case: identical Structured states ARE the same state.
    #[test]
    fn identical_structured_states_share_and_are_equal() {
        let a = json!({"id": "car", "role": "driver"});
        let b = json!({"id": "car", "role": "driver"});
        assert!(states_share_element(&a, &b));
        assert!(states_equal_set_aware(&a, &b));
    }

    /// Scalar `Simple` vs `Structured` whose only field is the same id
    /// should *not* match — they're different shapes carrying different
    /// information (the structured form may carry extras down the line).
    #[test]
    fn simple_vs_structured_with_only_id_dont_share() {
        let simple = json!("car");
        let structured = json!({"id": "car"});
        assert!(!states_share_element(&simple, &structured));
        assert!(!states_equal_set_aware(&simple, &structured));
    }

    /// `Parallel` set semantics survive the StateValue rewrite: two arrays
    /// with overlapping elements share, disjoint arrays don't.
    #[test]
    fn parallel_sets_share_on_overlap_not_disjoint() {
        let ab = json!(["a", "b"]);
        let bc = json!(["b", "c"]);
        let de = json!(["d", "e"]);
        assert!(states_share_element(&ab, &bc), "overlap on 'b'");
        assert!(!states_share_element(&ab, &de), "disjoint sets");
    }

    /// Multiset equality on `Parallel` is order-insensitive but element-
    /// strict — `[a, b]` equals `[b, a]` but not `[a, c]`.
    #[test]
    fn parallel_equal_set_aware_is_order_insensitive() {
        assert!(states_equal_set_aware(
            &json!(["a", "b"]),
            &json!(["b", "a"])
        ));
        assert!(!states_equal_set_aware(
            &json!(["a", "b"]),
            &json!(["a", "c"])
        ));
    }

    /// A Structured element inside a Parallel still requires full structural
    /// equality. `[drive_car]` and `[ride_car]` don't share even though both
    /// elements have `id=car`.
    #[test]
    fn parallel_of_structured_uses_full_equality() {
        let drive_set = json!([{"id": "car", "role": "driver"}]);
        let ride_set = json!([{"id": "car", "role": "passenger"}]);
        assert!(!states_share_element(&drive_set, &ride_set));
        assert!(!states_equal_set_aware(&drive_set, &ride_set));
    }

    // ─── da-j8cu: batched detection ──────────────────────────────────────

    /// A batch shaped like a sync pull: two fields, a double-open pair, an
    /// orphan end, and a pair of overlapping blocks. Returned already
    /// applied (as `apply_changeset` would leave them) so detection can run
    /// over the finished table.
    fn applied_pull_batch(db: &Db) -> Vec<Entry> {
        // Fixed, not `now`: the two DBs this builds must produce slug-for-slug
        // identical batches for the comparison below to mean anything.
        let base: DateTime<Utc> = "2026-04-01T09:00:00Z".parse().unwrap();
        let mut batch = vec![
            entry_builder("workout")
                .verb(Verb::Switch)
                .state(json!(true))
                .time_start_utc(base)
                .build(),
            // Double-open: opens a field that the row above already opened.
            entry_builder("workout")
                .verb(Verb::Switch)
                .state(json!(true))
                .time_start_utc(base + Duration::minutes(10))
                .build(),
            // Orphan end: a close on a field with no history at all.
            entry_builder("cooking")
                .verb(Verb::Switch)
                .state(json!(false))
                .time_start_utc(base + Duration::minutes(20))
                .build(),
            entry_builder("meeting")
                .verb(Verb::Block)
                .time_start_utc(base)
                .time_end_utc(base + Duration::minutes(60))
                .build(),
            // Overlap: shares the middle half-hour with the block above.
            entry_builder("meeting")
                .verb(Verb::Block)
                .time_start_utc(base + Duration::minutes(30))
                .time_end_utc(base + Duration::minutes(90))
                .build(),
        ];
        // The bulk door skips detection, which is exactly the sync shape:
        // rows land first, warnings are filed afterwards.
        db.insert_entries_bulk(&batch, crate::db::ConflictStrategy::Ignore)
            .unwrap();
        for entry in &mut batch {
            *entry = db.get_entry(&entry.slug).unwrap();
        }
        batch
    }

    fn summarize(warnings: &[IntegrityWarning]) -> Vec<(IntegrityKind, String, Option<Slug>)> {
        warnings
            .iter()
            .map(|w| {
                (
                    w.kind,
                    w.entry_slug.to_string(),
                    w.remote_entry.as_ref().map(|e| e.slug.clone()),
                )
            })
            .collect()
    }

    /// The batched door files exactly what the per-entry door files, kind
    /// for kind and counterpart for counterpart. This is the contract that
    /// lets sync pull use it — the speedup must cost no detection.
    #[test]
    fn batched_detection_matches_per_entry_detection() {
        let per_entry_db = Db::open_in_memory().unwrap();
        let batch = applied_pull_batch(&per_entry_db);
        let mut one_at_a_time = Vec::new();
        for entry in &batch {
            one_at_a_time.extend(
                detect_for_entry(per_entry_db.conn(), entry, DetectedBy::SyncPull).unwrap(),
            );
        }

        let batched_db = Db::open_in_memory().unwrap();
        let same_batch = applied_pull_batch(&batched_db);
        let batched =
            detect_for_entries(batched_db.conn(), &same_batch, DetectedBy::SyncPull).unwrap();

        assert!(
            !one_at_a_time.is_empty(),
            "fixture must actually trigger detection, else this proves nothing"
        );
        assert_eq!(summarize(&batched), summarize(&one_at_a_time));
    }

    /// The point of the batch: the field's history is read once for the
    /// whole batch, not once per entry. Counted through SQLite's authorizer,
    /// which fires per column read at prepare time — so a per-entry scan
    /// shows up as a count that grows with the batch, and the batched one
    /// does not. Without this, a first sync of a real history is quadratic
    /// and looks hung (da-j8cu).
    #[test]
    fn detection_reads_each_field_once_per_batch() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        fn entry_reads_for_batch(size: usize) -> usize {
            let db = Db::open_in_memory().unwrap();
            let base = Utc::now() - Duration::hours(6);
            let batch: Vec<Entry> = (0..size)
                .map(|i| {
                    entry_builder("mood")
                        .verb(Verb::Switch)
                        .state(json!(format!("s{i}")))
                        .time_start_utc(base + Duration::minutes(i as i64))
                        .build()
                })
                .collect();
            db.insert_entries_bulk(&batch, crate::db::ConflictStrategy::Ignore)
                .unwrap();

            let reads = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&reads);
            db.conn().authorizer(Some(
                move |ctx: rusqlite::hooks::AuthContext<'_>| -> rusqlite::hooks::Authorization {
                    if let rusqlite::hooks::AuthAction::Read { table_name, .. } = ctx.action {
                        if table_name == "entries" {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    rusqlite::hooks::Authorization::Allow
                },
            ));
            detect_for_entries(db.conn(), &batch, DetectedBy::SyncPull).unwrap();
            db.conn()
                .authorizer(None::<fn(rusqlite::hooks::AuthContext<'_>) -> _>);
            reads.load(Ordering::Relaxed)
        }

        let small = entry_reads_for_batch(8);
        let large = entry_reads_for_batch(64);
        assert_eq!(
            small, large,
            "reads of `entries` must not grow with the batch — one snapshot per field"
        );
    }
}
