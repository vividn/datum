//! Soft- and hard-delete paths plus the undo/revert flow that pairs with
//! them. All mutators record a history row inside the same transaction so
//! the audit trail is atomic with the entries change.

use chrono::Utc;
use rusqlite::{params, OptionalExtension};

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
        tx.execute(
            "UPDATE entries SET deleted_at = NULL, modify_time = ?2
             WHERE slug = ?1",
            params![pre.slug, Utc::now().to_rfc3339()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Hard-delete an entry by slug or hid. Returns Err(NotFound) if no rows
    /// affected. Bypasses the soft-delete tombstone — used by tests and
    /// admin paths that need the row physically gone.
    pub fn delete_entry(&self, slug_or_hid: &str) -> Result<()> {
        let affected = self.conn.execute(
            "DELETE FROM entries WHERE slug = ?1 OR hid = ?1",
            params![slug_or_hid],
        )?;
        if affected == 0 {
            Err(DatumError::NotFound(slug_or_hid.to_string()))
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
        let tx = self.conn.unchecked_transaction()?;
        let pre: Option<Entry> = tx
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
        history::record(&tx, &pre, ChangeKind::UserEdit)?;
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE entries SET deleted_at = ?1, modify_time = ?1 WHERE slug = ?2",
            params![now, pre.slug],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Revert an entry to the state it held at a prior version, resolving
    /// `hid_or_slug` through `entries.hid` / `entries.slug` (including
    /// tombstoned rows). Records the undo as a new `Undo` history row whose
    /// snapshot is the pre-revert state.
    ///
    /// Returns the restored [`Entry`]. Errors [`DatumError::NotFound`] if
    /// the entry or the target version doesn't exist.
    pub fn revert_entry_to(
        &self,
        hid_or_slug: &str,
        target_version: u32,
    ) -> Result<Entry> {
        let tx = self.conn.unchecked_transaction()?;

        // Resolve the current row (live or tombstoned) to pin down the hid.
        let current: Entry = tx
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
        let target_state = history::state_at_version(
            &tx,
            &current.hid,
            target_version,
            Some(&current),
        )?
        .ok_or_else(|| {
            DatumError::NotFound(format!(
                "version {target_version} for entry {}",
                current.hid
            ))
        })?;

        // Record the pre-undo state as the new history row, tagged as Undo.
        history::record(&tx, &current, ChangeKind::Undo)?;

        // Apply the target state to the live entries row. This covers
        // unchanged-slug updates as well as flipping `deleted_at` back and
        // forth — the snapshot drives every mutable column, including the
        // tombstone.
        apply_entry_state(&tx, &target_state)?;

        let restored = fetch_entry_any(&tx, &target_state.slug)?;
        tx.commit()?;
        Ok(restored)
    }
}
