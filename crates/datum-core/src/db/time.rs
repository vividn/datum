//! Bulk retime paths: shift `start_time` / `end_time` on filtered rows
//! (`update_times`) or set them outright (`set_start_time_with_precision`).
//! Both reach inside the row to recompute `content_hash` and bump
//! `modify_time`, then record the pre-edit snapshot via the history layer.

use chrono::{Duration, Utc};
use rusqlite::params;

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::history::{self, ChangeKind};

use super::row_mapper::row_to_entry;
use super::types::RetimeFilter;
use super::Db;

impl Db {
    /// Shift `start_time` / `end_time` on every live entry matching `filter`
    /// by `offset`. Recomputes `content_hash` and bumps `modify_time` on each
    /// affected row. Slug is left unchanged.
    ///
    /// Returns the number of rows updated.
    /// Retime matching rows to a fresh absolute `start_time` + precision.
    ///
    /// Unlike [`Self::update_times`], this overwrites start_time (and
    /// start_time_precision/_unit) with the caller's values rather than
    /// applying a delta. Any existing `end_time` shifts by the same delta
    /// `(new_start - old_start)` so the block's duration is preserved, but
    /// the end's precision is left untouched (only the input's precision
    /// informs the start's).
    pub fn set_start_time_with_precision(
        &self,
        filter: &RetimeFilter,
        new_start: &crate::time::DatumTime,
        precision_seconds: u32,
        precision_unit: &str,
    ) -> Result<usize> {
        let mut sql = String::from("SELECT * FROM entries WHERE deleted_at IS NULL");
        let mut bindings: Vec<String> = Vec::new();

        if let Some(needle) = filter.slug_or_hid {
            sql.push_str(" AND (slug = ? OR hid = ?)");
            bindings.push(needle.to_string());
            bindings.push(needle.to_string());
        }
        if let Some(field) = filter.field {
            sql.push_str(" AND field = ?");
            bindings.push(field.to_string());
        }
        if let Some(after) = filter.after_utc {
            sql.push_str(" AND start_time IS NOT NULL AND start_time >= ?");
            bindings.push(after.to_rfc3339());
        }
        if let Some(before) = filter.before_utc {
            sql.push_str(" AND start_time IS NOT NULL AND start_time <= ?");
            bindings.push(before.to_rfc3339());
        }

        let tx = self.conn.unchecked_transaction()?;
        let entries: Vec<Entry> = {
            let mut stmt = tx.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(bindings.iter()),
                row_to_entry,
            )?;
            rows.map(|r| r.map_err(DatumError::Db))
                .collect::<Result<_>>()?
        };

        let now = Utc::now().to_rfc3339();
        let mut count = 0usize;
        for mut e in entries {
            let delta = match e.start_time.as_ref() {
                Some(t) => Some(new_start.utc - t.utc),
                None => None,
            };
            e.start_time = Some(new_start.clone());
            e.start_time_precision = precision_seconds;
            e.start_time_precision_unit = precision_unit
                .parse()
                .map_err(|msg: String| DatumError::InvalidTime(msg))?;
            if let (Some(t), Some(d)) = (e.end_time.as_mut(), delta) {
                t.utc += d;
            }
            let new_hash = e.expected_content_hash();
            tx.execute(
                r#"UPDATE entries SET
                    start_time = ?2,
                    start_time_offset = ?3,
                    start_time_tz = ?4,
                    start_time_precision = ?5,
                    start_time_precision_unit = ?6,
                    end_time = ?7,
                    end_time_offset = ?8,
                    end_time_tz = ?9,
                    content_hash = ?10,
                    modify_time = ?11
                   WHERE slug = ?1"#,
                params![
                    e.slug,
                    e.start_time.as_ref().map(|t| t.utc.to_rfc3339()),
                    e.start_time.as_ref().map(|t| t.offset_minutes),
                    e.start_time.as_ref().map(|t| t.timezone.as_str()),
                    e.start_time_precision,
                    e.start_time_precision_unit.to_string(),
                    e.end_time.as_ref().map(|t| t.utc.to_rfc3339()),
                    e.end_time.as_ref().map(|t| t.offset_minutes),
                    e.end_time.as_ref().map(|t| t.timezone.as_str()),
                    new_hash,
                    now,
                ],
            )?;
            count += 1;
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn update_times(&self, filter: &RetimeFilter, offset: Duration) -> Result<usize> {
        let mut sql = String::from(
            "SELECT * FROM entries WHERE deleted_at IS NULL",
        );
        let mut bindings: Vec<String> = Vec::new();

        if let Some(needle) = filter.slug_or_hid {
            sql.push_str(" AND (slug = ? OR hid = ?)");
            bindings.push(needle.to_string());
            bindings.push(needle.to_string());
        }
        if let Some(field) = filter.field {
            sql.push_str(" AND field = ?");
            bindings.push(field.to_string());
        }
        if let Some(after) = filter.after_utc {
            sql.push_str(" AND start_time IS NOT NULL AND start_time >= ?");
            bindings.push(after.to_rfc3339());
        }
        if let Some(before) = filter.before_utc {
            sql.push_str(" AND start_time IS NOT NULL AND start_time <= ?");
            bindings.push(before.to_rfc3339());
        }

        let tx = self.conn.unchecked_transaction()?;
        let entries: Vec<Entry> = {
            let mut stmt = tx.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(bindings.iter()),
                row_to_entry,
            )?;
            rows.map(|r| r.map_err(DatumError::Db))
                .collect::<Result<_>>()?
        };

        let now = Utc::now().to_rfc3339();
        let mut count = 0usize;
        for e in entries {
            // Record the pre-retime snapshot before shifting — the history
            // row must reflect the state that existed before this retime.
            history::record(&tx, &e, ChangeKind::Retime)?;
            let mut shifted = e;
            if let Some(t) = shifted.start_time.as_mut() {
                t.utc += offset;
            }
            if let Some(t) = shifted.end_time.as_mut() {
                t.utc += offset;
            }
            let new_hash = shifted.expected_content_hash();
            tx.execute(
                r#"UPDATE entries SET
                    start_time  = ?2,
                    end_time    = ?3,
                    content_hash = ?4,
                    modify_time = ?5
                   WHERE slug = ?1"#,
                params![
                    shifted.slug,
                    shifted.start_time.as_ref().map(|t| t.utc.to_rfc3339()),
                    shifted.end_time.as_ref().map(|t| t.utc.to_rfc3339()),
                    new_hash,
                    now,
                ],
            )?;
            count += 1;
        }
        tx.commit()?;
        Ok(count)
    }
}
