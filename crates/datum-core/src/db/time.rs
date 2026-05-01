//! Bulk retime paths.
//!
//! da-5ns sub-bead 1 stub: post-pivot the entry's time-shape is just
//! `occur_time` + `dur` — no end_time to slide. These methods now retime
//! `occur_time` and leave `dur` untouched. Sub-bead 3/4 wires
//! re-derivation of `verb` if the retime mutates state-relevant
//! columns; for now retime never touches state/dur/following_state so
//! verb stays put.

use chrono::{Duration, Utc};
use rusqlite::params;

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::history::{self, ChangeKind};

use super::row_mapper::row_to_entry;
use super::types::RetimeFilter;
use super::Db;

impl Db {
    /// Retime matching rows to a fresh absolute `occur_time` + precision.
    ///
    /// The `precision_unit` is rendered into a single ISO-8601 duration
    /// string (e.g. `PT1S`, `PT1M`, `P1D`) and stored in
    /// `occur_time_precision` — the v=20 schema collapsed the
    /// (precision_seconds, precision_unit) pair into one column.
    pub fn set_start_time_with_precision(
        &self,
        filter: &RetimeFilter,
        new_start: &crate::time::DatumTime,
        _precision_seconds: u32,
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
            sql.push_str(" AND occur_time >= ?");
            bindings.push(after.to_rfc3339());
        }
        if let Some(before) = filter.before_utc {
            sql.push_str(" AND occur_time <= ?");
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

        let precision_iso = precision_unit_to_iso(precision_unit);
        let now = Utc::now().to_rfc3339();
        let mut count = 0usize;
        for mut e in entries {
            e.occur_time = Some(new_start.clone());
            e.occur_time_precision = precision_iso.clone();
            let new_hash = e.expected_content_hash();
            tx.execute(
                r#"UPDATE entries SET
                    occur_time = ?2,
                    occur_time_offset = ?3,
                    occur_time_tz = ?4,
                    occur_time_precision = ?5,
                    content_hash = ?6,
                    modify_time = ?7
                   WHERE slug = ?1"#,
                params![
                    e.slug,
                    e.occur_time.as_ref().map(|t| t.utc.to_rfc3339()),
                    e.occur_time.as_ref().map(|t| t.offset_minutes),
                    e.occur_time.as_ref().map(|t| t.timezone.as_str()),
                    e.occur_time_precision,
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
            sql.push_str(" AND occur_time >= ?");
            bindings.push(after.to_rfc3339());
        }
        if let Some(before) = filter.before_utc {
            sql.push_str(" AND occur_time <= ?");
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
            history::record(&tx, &e, ChangeKind::Retime)?;
            let mut shifted = e;
            if let Some(t) = shifted.occur_time.as_mut() {
                t.utc += offset;
            }
            let new_hash = shifted.expected_content_hash();
            tx.execute(
                r#"UPDATE entries SET
                    occur_time  = ?2,
                    content_hash = ?3,
                    modify_time = ?4
                   WHERE slug = ?1"#,
                params![
                    shifted.slug,
                    shifted.occur_time.as_ref().map(|t| t.utc.to_rfc3339()),
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

/// Map a precision-unit name ("second", "minute", "hour", "day",
/// "month", "year", "exact") to a single ISO-8601 duration string.
/// Sub-bead 3 will replace this with a proper PrecisionUnit→ISO helper.
fn precision_unit_to_iso(unit: &str) -> Option<String> {
    match unit {
        "exact" | "" => None,
        "second" => Some("PT1S".to_string()),
        "minute" => Some("PT1M".to_string()),
        "hour" => Some("PT1H".to_string()),
        "date" | "day" => Some("P1D".to_string()),
        "month" => Some("P1M".to_string()),
        "year" => Some("P1Y".to_string()),
        _ => None,
    }
}
