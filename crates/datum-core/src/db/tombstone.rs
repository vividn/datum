//! Soft- and hard-delete paths plus the undo/revert flow that pairs with
//! them. All mutators record a history row inside the same transaction so
//! the audit trail is atomic with the entries change.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::history::{self, ChangeKind};

use super::insert::apply_entry_state;
use super::lookup::fetch_entry_any;
use super::row_mapper::row_to_entry;
use super::Db;

impl Db {
    /// Clear a tombstone by slug or hid — the inverse of [`Db::soft_delete_entry`].
    /// Errors with `NotFound` if no matching tombstone exists. Records a
    /// `user_edit` history row snapshotting the pre-undelete (tombstoned) state.
    pub fn undelete(&self, slug_or_hid: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let pre: Option<Entry> = tx
            .query_row(
                "SELECT * FROM entries
                 WHERE (slug = ?1 OR hid = ?1) AND deleted_at IS NOT NULL
                 LIMIT 1",
                params![slug_or_hid],
                row_to_entry,
            )
            .optional()?;
        let pre = match pre {
            Some(e) => e,
            None => return Err(DatumError::NotFound(slug_or_hid.to_string())),
        };
        history::record(&tx, &pre, ChangeKind::UserEdit)?;
        // da-4vyw: deleted-ness is hashed, so clearing the tombstone moves
        // the row's content_hash back to its live value. Without this the
        // stored hash disagrees with `expected_content_hash` forever after.
        let mut live = pre.clone();
        live.deleted_at = None;
        tx.execute(
            "UPDATE entries SET deleted_at = NULL, modify_time = ?2, content_hash = ?3
             WHERE hid = ?1",
            params![
                pre.hid,
                Utc::now().to_rfc3339(),
                live.expected_content_hash()
            ],
        )?;
        super::sync::emit(&tx, Some(&pre), &pre.hid, ChangeKind::UserEdit)?;
        tx.commit()?;
        Ok(())
    }

    /// Hard-delete an entry by hid — not slug, which a keep-both pair
    /// shares. Returns Err(NotFound) if no rows affected. Bypasses the
    /// soft-delete tombstone — used by tests and admin paths that need the
    /// row physically gone.
    pub fn delete_entry(&self, hid: &str) -> Result<()> {
        let affected = self
            .conn
            .execute("DELETE FROM entries WHERE hid = ?1", params![hid])?;
        if affected == 0 {
            Err(DatumError::NotFound(hid.to_string()))
        } else {
            Ok(())
        }
    }

    /// Soft-delete an entry by slug or hid (tombstone).
    ///
    /// Sets `deleted_at` to now; read paths that care about soft-delete (such
    /// as `get_active_state`) will ignore tombstoned rows. Errors if no row
    /// matches. If the row is already tombstoned, the tombstone is refreshed.
    /// Records a `user_edit` history row capturing the pre-delete state so
    /// `datum undo` can restore it by reverting to the prior version.
    pub fn soft_delete_entry(&self, slug_or_hid: &str) -> Result<()> {
        self.soft_delete_entry_as(slug_or_hid, ChangeKind::UserEdit)
    }

    /// [`Db::soft_delete_entry`] that also appends `comment` to the row's
    /// `data.comment` array in the same transaction (da-ult68, `delete -c`).
    /// Returns the tombstoned row.
    pub fn soft_delete_entry_with_comment(
        &self,
        slug_or_hid: &str,
        comment: Option<&str>,
    ) -> Result<Entry> {
        self.soft_delete(slug_or_hid, ChangeKind::UserEdit, comment)
    }

    /// [`Db::soft_delete_entry`] with the history row's `change_kind` chosen by
    /// the caller. `undo` of a just-created entry is a delete, and it tags the
    /// row [`ChangeKind::Undo`] so `redo` can see that the newest step was an
    /// undo and step forward again (da-s6no).
    pub fn soft_delete_entry_as(&self, slug_or_hid: &str, kind: ChangeKind) -> Result<()> {
        self.soft_delete(slug_or_hid, kind, None).map(|_| ())
    }

    fn soft_delete(
        &self,
        slug_or_hid: &str,
        kind: ChangeKind,
        comment: Option<&str>,
    ) -> Result<Entry> {
        let tx = self.conn.unchecked_transaction()?;
        let dead = soft_delete_in_tx(&tx, slug_or_hid, kind, comment)?;
        tx.commit()?;
        Ok(dead)
    }

    /// Revert an entry to the state it held at a prior version, resolving
    /// `hid_or_slug` through `entries.hid` / `entries.slug` (including
    /// tombstoned rows). Records the undo as a new `Undo` history row whose
    /// snapshot is the pre-revert state.
    ///
    /// Returns the restored [`Entry`]. Errors [`DatumError::NotFound`] if
    /// the entry or the target version doesn't exist.
    pub fn revert_entry_to(&self, hid_or_slug: &str, target_version: u32) -> Result<Entry> {
        self.revert_entry_to_as(hid_or_slug, target_version, ChangeKind::Undo)
    }

    /// [`Db::revert_entry_to`] with the history row's `change_kind` chosen by
    /// the caller — [`ChangeKind::Undo`] for a step back, [`ChangeKind::Redo`]
    /// for the step forward that takes it back (da-s6no). The mechanics are
    /// identical; only the label on the new row differs, and that label is what
    /// tells `redo` whether anything lies ahead.
    pub fn revert_entry_to_as(
        &self,
        hid_or_slug: &str,
        target_version: u32,
        kind: ChangeKind,
    ) -> Result<Entry> {
        let tx = self.conn.unchecked_transaction()?;
        let restored = revert_in_tx(&tx, hid_or_slug, target_version, kind)?;
        tx.commit()?;
        Ok(restored)
    }
}

/// Transaction-free body of [`Db::revert_entry_to_as`]: reverts inside
/// whatever transaction `conn` already has open, so a bare `undo` / `redo`
/// moves every entry of one stack step in one commit (da-pxsa8).
pub fn revert_in_tx(
    conn: &Connection,
    hid_or_slug: &str,
    target_version: u32,
    kind: ChangeKind,
) -> Result<Entry> {
    // Resolve the current row (live or tombstoned) to pin down the hid.
    let current: Entry = conn
        .query_row(
            "SELECT * FROM entries WHERE slug = ?1 OR hid = ?1 LIMIT 1",
            params![hid_or_slug],
            row_to_entry,
        )
        .optional()?
        .ok_or_else(|| DatumError::NotFound(hid_or_slug.to_string()))?;

    // Resolve the target state. State at version 1 = v1.snapshot;
    // state at version N>1 = v(N+1).snapshot if recorded, else the
    // current live row when N is the latest version.
    let target_state =
        history::state_at_version(conn, &current.hid, target_version, Some(&current))?.ok_or_else(
            || {
                DatumError::NotFound(format!(
                    "version {target_version} for entry {}",
                    current.hid
                ))
            },
        )?;

    // Record the pre-revert state as the new history row, tagged with the
    // step that is about to happen.
    history::record(conn, &current, kind)?;

    // Apply the target state to the live entries row. This covers
    // unchanged-slug updates as well as flipping `deleted_at` back and
    // forth — the snapshot drives every mutable column, including the
    // tombstone.
    apply_entry_state(conn, &target_state)?;
    super::sync::emit(conn, Some(&current), &current.hid, kind)?;

    fetch_entry_any(conn, &target_state.hid)
}

/// Transaction-free body of [`Db::soft_delete_entry_as`]: tombstones inside
/// whatever transaction `conn` already has open, so a replace restore
/// (`Db::replace_entries`) deletes in the same commit as it writes, and a
/// stack step in the same commit as the rest of the step.
pub fn soft_delete_in_tx(
    conn: &Connection,
    slug_or_hid: &str,
    kind: ChangeKind,
    comment: Option<&str>,
) -> Result<Entry> {
    let pre: Option<Entry> = conn
        .query_row(
            "SELECT * FROM entries WHERE slug = ?1 OR hid = ?1 LIMIT 1",
            params![slug_or_hid],
            row_to_entry,
        )
        .optional()?;
    let pre = match pre {
        Some(e) => e,
        None => return Err(DatumError::NotFound(slug_or_hid.to_string())),
    };
    // da-ult68: a reason is written onto the row, so an undo must revert
    // through history rather than `undelete`, which keeps `data`. A row with
    // no trail gets its pre-delete state as v1 from `history::record`.
    history::record(conn, &pre, kind)?;
    let now = Utc::now();
    // da-4vyw: a tombstone must not hash identically to the live row it
    // replaced — that equality is what would authorise a silent dedupe
    // of a deletion against its own original. Rehash with the deleted
    // marker set (the marker, not the instant — see `HashInputs`).
    let mut dead = pre.clone();
    dead.deleted_at = Some(now);
    if let Some(c) = comment {
        let data = dead.data.get_or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = data.as_object_mut() {
            let slot = obj
                .entry("comment")
                .or_insert_with(|| serde_json::json!([]));
            if !slot.is_array() {
                *slot = serde_json::Value::Array(vec![slot.take()]);
            }
            if let Some(a) = slot.as_array_mut() {
                a.push(c.into());
            }
        }
    }
    conn.execute(
        "UPDATE entries SET deleted_at = ?1, modify_time = ?1, content_hash = ?3, data = ?4
         WHERE hid = ?2",
        params![
            now.to_rfc3339(),
            pre.hid,
            dead.expected_content_hash(),
            dead.data.as_ref().map(|v| v.to_string())
        ],
    )?;
    super::sync::emit(conn, Some(&pre), &pre.hid, kind)?;
    Ok(dead)
}
