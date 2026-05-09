//! `sync_conflicts` reads and resolution paths. The detection side lives in
//! `crate::conflicts`; this module is the `Db`-facing API that wraps those
//! free functions for callers that already hold a [`Db`] handle.

use chrono::Utc;

use crate::entry::Entry;
use crate::error::{DatumError, Result};

use super::insert::rewrite_entry_to_snapshot;
use super::Db;

impl Db {
    /// List rows from `sync_conflicts`.
    pub fn list_conflicts(
        &self,
        filter: &crate::conflicts::ListFilter,
    ) -> Result<Vec<crate::conflicts::Conflict>> {
        crate::conflicts::list(&self.conn, filter)
    }

    /// Fetch one conflict row by its short hid or full id.
    pub fn get_conflict(&self, token: &str) -> Result<Option<crate::conflicts::Conflict>> {
        crate::conflicts::get_by_hid_or_id(&self.conn, token)
    }

    /// Resolve a conflict: mark the row and apply the side effect implied
    /// by `resolution`.
    ///
    /// Side-effect matrix:
    /// - `CloseOlder` / `AutoCloseOlder`: end the older ongoing entry at
    ///   `local_entry.start_time` (no-op if no `remote_entry`).
    /// - `TakeLocal`: rewrite the row in `entries` matching
    ///   `local_entry.slug` to the local snapshot. For sync_pull conflicts
    ///   where the local entry was overwritten by a pulled remote, this
    ///   restores the user's local version. Skipped silently if no row
    ///   matches the slug.
    /// - `TakeRemote`: same, but using `remote_entry`. No-op when
    ///   `remote_entry IS NULL`.
    /// - `Edit`: caller is expected to have already committed the merged
    ///   version via [`Db::resolve_conflict_with_edited`]. This direct
    ///   form just records the resolution — useful when the caller has
    ///   already mutated entries through another channel.
    /// - `KeepBoth`: no entry mutation.
    pub fn resolve_conflict(
        &self,
        token: &str,
        resolution: crate::conflicts::Resolution,
    ) -> Result<crate::conflicts::Conflict> {
        let conflict = self
            .get_conflict(token)?
            .ok_or_else(|| DatumError::NotFound(format!("conflict {token}")))?;
        if conflict.resolution.is_some() {
            return Err(DatumError::Other(format!(
                "conflict {} already resolved ({})",
                conflict.hid,
                conflict.resolution.clone().unwrap_or_default()
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        match resolution {
            crate::conflicts::Resolution::CloseOlder
            | crate::conflicts::Resolution::AutoCloseOlder => {
                if let Some(older) = conflict.remote_entry.as_ref() {
                    // da-5ns sub-bead 1 stub: pass the local entry's
                    // occur_time (formerly `start_time` in v19). Sub-bead
                    // 5 reimplements close_older_at against the new
                    // (occur_time, dur, following_state) shape.
                    crate::conflicts::close_older_at(
                        &tx,
                        older,
                        conflict.local_entry.occur_time.as_ref(),
                    )?;
                }
            }
            crate::conflicts::Resolution::TakeLocal => {
                rewrite_entry_to_snapshot(&tx, &conflict.local_entry)?;
            }
            crate::conflicts::Resolution::TakeRemote => {
                if let Some(remote) = conflict.remote_entry.as_ref() {
                    rewrite_entry_to_snapshot(&tx, remote)?;
                }
            }
            crate::conflicts::Resolution::Edit | crate::conflicts::Resolution::KeepBoth => {
                // Edit: caller wrote the merged entry separately (see
                // resolve_conflict_with_edited); KeepBoth: no DB mutation.
            }
        }
        crate::conflicts::mark_resolved(&tx, &conflict.id, resolution, Utc::now())?;
        tx.commit()?;
        // Re-fetch for the updated resolution columns.
        Ok(self
            .get_conflict(&conflict.id)?
            .expect("just-resolved conflict vanished"))
    }

    /// Resolve via `--edit`: replace the row in `entries` matching
    /// `edited.slug` with the supplied snapshot, and mark the conflict
    /// `Edit`. Both mutations land in one transaction.
    pub fn resolve_conflict_with_edited(
        &self,
        token: &str,
        edited: &Entry,
    ) -> Result<crate::conflicts::Conflict> {
        let conflict = self
            .get_conflict(token)?
            .ok_or_else(|| DatumError::NotFound(format!("conflict {token}")))?;
        if conflict.resolution.is_some() {
            return Err(DatumError::Other(format!(
                "conflict {} already resolved ({})",
                conflict.hid,
                conflict.resolution.clone().unwrap_or_default()
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        rewrite_entry_to_snapshot(&tx, edited)?;
        crate::conflicts::mark_resolved(
            &tx,
            &conflict.id,
            crate::conflicts::Resolution::Edit,
            Utc::now(),
        )?;
        tx.commit()?;
        Ok(self
            .get_conflict(&conflict.id)?
            .expect("just-resolved conflict vanished"))
    }

    /// Count of pending conflicts (`resolution IS NULL`). The `now` view
    /// uses this to print a top-line warning.
    pub fn pending_conflict_count(&self) -> Result<u32> {
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sync_conflicts WHERE resolution IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(DatumError::Db)?;
        Ok(n as u32)
    }

    /// Run the auto-resolver. `min_age_secs` is the delay before a pending
    /// conflict becomes eligible.
    pub fn auto_resolve_conflicts(
        &self,
        now: chrono::DateTime<Utc>,
        min_age_secs: i64,
    ) -> Result<Vec<crate::conflicts::Conflict>> {
        let tx = self.conn.unchecked_transaction()?;
        let applied = crate::conflicts::auto_resolve_pending(&tx, now, min_age_secs)?;
        tx.commit()?;
        Ok(applied)
    }

    /// Count of live ongoing entries on `field`. The `now` view uses this
    /// to prepend an overlap warning when the count is >1.
    pub fn open_count_for_field(&self, field: &str) -> Result<u32> {
        crate::conflicts::open_entries_for_field(&self.conn, field)
    }
}
