//! Multi-row read paths: per-field lists, time-window lists, and the unified
//! `list_entries` query that backs `tail`/`head`.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension};

use crate::entry::Entry;
use crate::error::{DatumError, Result};

use super::row_mapper::row_to_entry;
use super::types::{time_expr, time_expr_endbound, ListParams, TimeMetric};
use super::Db;

impl Db {
    /// Query live entries for a field, ordered by start_time descending.
    pub fn entries_for_field(&self, field: &str, limit: Option<usize>) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1 AND deleted_at IS NULL
             ORDER BY {} DESC{}",
            time_expr_endbound(),
            limit.map(|l| format!(" LIMIT {l}")).unwrap_or_default()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![field], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Query live entries for a field, ordered by start_time ascending (oldest first).
    pub fn entries_for_field_asc(&self, field: &str, limit: Option<usize>) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1 AND deleted_at IS NULL
             ORDER BY {} ASC{}",
            time_expr_endbound(),
            limit.map(|l| format!(" LIMIT {l}")).unwrap_or_default()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![field], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Unified entry query for `tail` / `head`.
    ///
    /// Applies optional field filter, a time window bounded by the selected
    /// [`TimeMetric`], a limit (last-N for tail, first-N for head), and an
    /// optional final reverse. The returned vec is oldest-first by default
    /// (so tail's output reads top-to-bottom in chronological order).
    pub fn list_entries(&self, params: &ListParams) -> Result<Vec<Entry>> {
        let metric_expr: &str = time_expr(params.metric);

        let mut sql = String::from("SELECT * FROM entries WHERE deleted_at IS NULL");
        let mut bindings: Vec<String> = Vec::new();

        // da-5ns: occur_time is NOT NULL post-pivot — but the CLI's
        // `-T` / no-timestamp path stubs occur_time with create_time and
        // marks the row by giving it a `:unset` slug suffix. The Occur
        // metric (chronological "when did this happen") must skip those
        // stub rows; Modify / Hybrid still see them.
        if params.metric == TimeMetric::Occur {
            sql.push_str(" AND slug NOT LIKE '%:unset'");
        }
        if let Some(field) = &params.field {
            sql.push_str(" AND field = ?");
            bindings.push(field.clone());
        }
        if let Some(start) = &params.start_utc {
            sql.push_str(&format!(" AND {metric_expr} >= ?"));
            bindings.push(start.to_rfc3339());
        }
        if let Some(end) = &params.end_utc {
            sql.push_str(&format!(" AND {metric_expr} <= ?"));
            bindings.push(end.to_rfc3339());
        }

        // For tail we want the LAST N by metric, so sort DESC + LIMIT then
        // reverse; for head we want FIRST N, so sort ASC + LIMIT. Secondary
        // sort by `slug` keeps tied metric timestamps in a deterministic
        // order — slugs embed the timestamp so same-metric rows sort stably.
        let order_dir = if params.head { "ASC" } else { "DESC" };
        sql.push_str(&format!(
            " ORDER BY {metric_expr} {order_dir}, slug {order_dir}"
        ));
        if let Some(n) = params.limit {
            sql.push_str(&format!(" LIMIT {n}"));
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), row_to_entry)?;
        let mut results: Vec<Entry> = rows
            .map(|r| r.map_err(DatumError::Db))
            .collect::<Result<_>>()?;

        // Normalize to oldest-first; then honour the caller's reverse.
        if !params.head {
            results.reverse();
        }
        if params.reverse {
            results.reverse();
        }
        Ok(results)
    }

    /// Live entries whose `create_time` is strictly greater than `since`,
    /// optionally filtered by `field`. Ordered oldest-first so `tail --watch`
    /// can append rows as they arrive.
    pub fn list_entries_created_after(
        &self,
        field: Option<&str>,
        since: DateTime<Utc>,
    ) -> Result<Vec<Entry>> {
        let mut sql =
            String::from("SELECT * FROM entries WHERE deleted_at IS NULL AND create_time > ?");
        let mut bindings: Vec<String> = vec![since.to_rfc3339()];
        if let Some(f) = field {
            sql.push_str(" AND field = ?");
            bindings.push(f.to_string());
        }
        sql.push_str(" ORDER BY create_time ASC, slug ASC");

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Get all live entries within a UTC time range (inclusive).
    pub fn entries_in_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Entry>> {
        let expr = time_expr_endbound();
        let sql = format!(
            "SELECT * FROM entries
             WHERE {expr} >= ?1
               AND {expr} <= ?2
               AND deleted_at IS NULL
             ORDER BY {expr} ASC",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![start.to_rfc3339(), end.to_rfc3339()], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Stream every live entry through `f`, without buffering the whole set
    /// in memory. Rows are yielded in slug order (stable across backups).
    ///
    /// The callback's error type is generic so callers can propagate I/O and
    /// serialization failures alongside database errors without wrapping.
    pub fn visit_all_entries<E, F>(&self, mut f: F) -> std::result::Result<(), E>
    where
        F: FnMut(Entry) -> std::result::Result<(), E>,
        E: From<DatumError>,
    {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM entries WHERE deleted_at IS NULL ORDER BY slug ASC")
            .map_err(|e| E::from(DatumError::Db(e)))?;
        let rows = stmt
            .query_map([], row_to_entry)
            .map_err(|e| E::from(DatumError::Db(e)))?;
        for row in rows {
            let entry = row.map_err(|e| E::from(DatumError::Db(e)))?;
            f(entry)?;
        }
        Ok(())
    }

    /// Every entry in the store, **including tombstoned rows**. Undo uses this
    /// to locate deleted entries by slug/hid before restoring them.
    pub fn list_including_deleted(&self) -> Result<Vec<Entry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM entries ORDER BY slug ASC")?;
        let rows = stmt.query_map([], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Tombstoned rows whose `deleted_at` is within `within` of now, most
    /// recently deleted first. Undo uses this to locate recent deletes
    /// inside the safety window.
    pub fn list_recent_tombstones(&self, within: Duration) -> Result<Vec<Entry>> {
        let cutoff = (Utc::now() - within).to_rfc3339();
        let mut stmt = self.conn.prepare(
            "SELECT * FROM entries
             WHERE deleted_at IS NOT NULL AND deleted_at >= ?1
             ORDER BY deleted_at DESC, slug DESC",
        )?;
        let rows = stmt.query_map(params![cutoff], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Most recently tombstoned row, regardless of age. `undo --force`
    /// uses this to reach back past the default safety window.
    pub fn most_recent_tombstone(&self) -> Result<Option<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM entries
             WHERE deleted_at IS NOT NULL
             ORDER BY deleted_at DESC, slug DESC
             LIMIT 1",
        )?;
        stmt.query_row([], row_to_entry)
            .optional()
            .map_err(DatumError::Db)
    }

    /// All entries (including tombstoned), for bulk checks.
    pub fn all_entries(&self) -> Result<Vec<Entry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM entries ORDER BY slug ASC")?;
        let rows = stmt.query_map([], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Slugs of rows with `deleted_at` set (tombstones).
    pub fn tombstone_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT slug FROM entries WHERE deleted_at IS NOT NULL ORDER BY slug ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }
}
