//! `integrity_warnings` reads and resolution paths. Detection lives in
//! `crate::integrity`; this module is the `Db`-facing API that wraps those
//! free functions for callers that already hold a [`Db`] handle.

use chrono::Utc;

use crate::entry::Entry;
use crate::error::{DatumError, Result};

use super::insert::rewrite_entry_to_snapshot;
use super::Db;

impl Db {
    /// List rows from `integrity_warnings`.
    pub fn list_integrity_warnings(
        &self,
        filter: &crate::integrity::ListFilter,
    ) -> Result<Vec<crate::integrity::IntegrityWarning>> {
        crate::integrity::list(&self.conn, filter)
    }

    /// Fetch one integrity-warning row by its short hid or full id.
    pub fn get_integrity_warning(
        &self,
        token: &str,
    ) -> Result<Option<crate::integrity::IntegrityWarning>> {
        crate::integrity::get_by_hid_or_id(&self.conn, token)
    }

    /// Resolve an integrity warning: mark the row and apply the side effect
    /// implied by `resolution`.
    ///
    /// Side-effect matrix:
    /// - `CloseOlder` / `AutoCloseOlder`: end the older ongoing entry at
    ///   `local_entry.occur_time` (no-op if no `remote_entry`).
    /// - `TakeLocal`: rewrite the row in `entries` matching
    ///   `local_entry.slug` to the local snapshot. For sync-pull rows
    ///   where the local entry was overwritten by a pulled remote, this
    ///   restores the user's local version. Skipped silently if no row
    ///   matches the slug.
    /// - `TakeRemote`: same, but using `remote_entry`. No-op when
    ///   `remote_entry IS NULL`.
    /// - `Edit`: caller is expected to have already committed the merged
    ///   version via [`Db::resolve_integrity_warning_with_edited`]. This
    ///   direct form just records the resolution — useful when the caller
    ///   has already mutated entries through another channel.
    /// - `KeepBoth`: no entry mutation.
    pub fn resolve_integrity_warning(
        &self,
        token: &str,
        resolution: crate::integrity::Resolution,
    ) -> Result<crate::integrity::IntegrityWarning> {
        let warning = self
            .get_integrity_warning(token)?
            .ok_or_else(|| DatumError::NotFound(format!("integrity warning {token}")))?;
        if warning.resolution.is_some() {
            return Err(DatumError::Other(format!(
                "integrity warning {} already resolved ({})",
                warning.hid,
                warning.resolution.clone().unwrap_or_default()
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        match resolution {
            crate::integrity::Resolution::CloseOlder
            | crate::integrity::Resolution::AutoCloseOlder => {
                if let Some(older) = warning.remote_entry.as_ref() {
                    crate::integrity::close_older_at(
                        &tx,
                        older,
                        warning.local_entry.occur_time.as_ref(),
                    )?;
                }
            }
            crate::integrity::Resolution::TakeLocal => {
                rewrite_entry_to_snapshot(&tx, &warning.local_entry)?;
            }
            crate::integrity::Resolution::TakeRemote => {
                if let Some(remote) = warning.remote_entry.as_ref() {
                    rewrite_entry_to_snapshot(&tx, remote)?;
                }
            }
            crate::integrity::Resolution::Edit | crate::integrity::Resolution::KeepBoth => {
                // Edit: caller wrote the merged entry separately (see
                // resolve_integrity_warning_with_edited); KeepBoth: no DB mutation.
            }
        }
        crate::integrity::mark_resolved(&tx, &warning.id, resolution, Utc::now())?;
        tx.commit()?;
        Ok(self
            .get_integrity_warning(&warning.id)?
            .expect("just-resolved warning vanished"))
    }

    /// Resolve via `--edit`: replace the row in `entries` matching
    /// `edited.slug` with the supplied snapshot, and mark the warning
    /// `Edit`. Both mutations land in one transaction.
    pub fn resolve_integrity_warning_with_edited(
        &self,
        token: &str,
        edited: &Entry,
    ) -> Result<crate::integrity::IntegrityWarning> {
        let warning = self
            .get_integrity_warning(token)?
            .ok_or_else(|| DatumError::NotFound(format!("integrity warning {token}")))?;
        if warning.resolution.is_some() {
            return Err(DatumError::Other(format!(
                "integrity warning {} already resolved ({})",
                warning.hid,
                warning.resolution.clone().unwrap_or_default()
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        rewrite_entry_to_snapshot(&tx, edited)?;
        crate::integrity::mark_resolved(
            &tx,
            &warning.id,
            crate::integrity::Resolution::Edit,
            Utc::now(),
        )?;
        tx.commit()?;
        Ok(self
            .get_integrity_warning(&warning.id)?
            .expect("just-resolved warning vanished"))
    }

    /// Count of pending integrity warnings (`resolution IS NULL`). The
    /// `now` view uses this to print a top-line warning.
    pub fn pending_integrity_count(&self) -> Result<u32> {
        // da-sidz: keep this count consistent with `integrity::list` — a
        // warning whose trigger entry has been soft-deleted is moot, so it
        // must not inflate the `now`-view pending count either.
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM integrity_warnings
                 WHERE resolution IS NULL
                   AND entry_slug NOT IN (SELECT slug FROM entries WHERE deleted_at IS NOT NULL)",
                [],
                |row| row.get(0),
            )
            .map_err(DatumError::Db)?;
        Ok(n as u32)
    }

    /// Run the auto-resolver. `min_age_secs` is the delay before a pending
    /// warning becomes eligible.
    pub fn auto_resolve_integrity(
        &self,
        now: chrono::DateTime<Utc>,
        min_age_secs: i64,
    ) -> Result<Vec<crate::integrity::IntegrityWarning>> {
        let tx = self.conn.unchecked_transaction()?;
        let applied = crate::integrity::auto_resolve_pending(&tx, now, min_age_secs)?;
        tx.commit()?;
        Ok(applied)
    }

    /// Count of live ongoing entries on `field`. The `now` view uses this
    /// to prepend an overlap warning when the count is >1.
    pub fn open_count_for_field(&self, field: &str) -> Result<u32> {
        crate::integrity::open_entries_for_field(&self.conn, field)
    }
}
