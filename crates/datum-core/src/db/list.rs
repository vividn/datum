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
        // da-ghlx: default-filter review markers (`extra_metadata.flag =
        // true`, da-dnnn) and protocol notes (`extra_metadata.meta =
        // true`, da-xn8j). `json_extract` returns 1 for JSON true,
        // 0 for JSON false, NULL when the path or column is absent —
        // `IS NOT 1` covers all three "not-set" cases without
        // dereferencing a NULL column.
        if !params.include_meta {
            sql.push_str(
                " AND json_extract(extra_metadata, '$.flag') IS NOT 1 \
                  AND json_extract(extra_metadata, '$.meta') IS NOT 1",
            );
        }
        if !params.fields.is_empty() {
            // One name → `= ?` (equivalent, slightly tighter plan); two or
            // more → `IN (?, ?, …)` so a multi-field tail merges via the
            // `entries_field_occur` index instead of scanning.
            if params.fields.len() == 1 {
                sql.push_str(" AND field = ?");
            } else {
                let qs = vec!["?"; params.fields.len()].join(", ");
                sql.push_str(&format!(" AND field IN ({qs})"));
            }
            bindings.extend(params.fields.iter().cloned());
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
        fields: &[&str],
        since: DateTime<Utc>,
    ) -> Result<Vec<Entry>> {
        let mut sql =
            String::from("SELECT * FROM entries WHERE deleted_at IS NULL AND create_time > ?");
        let mut bindings: Vec<String> = vec![since.to_rfc3339()];
        if !fields.is_empty() {
            if fields.len() == 1 {
                sql.push_str(" AND field = ?");
            } else {
                let qs = vec!["?"; fields.len()].join(", ");
                sql.push_str(&format!(" AND field IN ({qs})"));
            }
            bindings.extend(fields.iter().map(|f| f.to_string()));
        }
        sql.push_str(" ORDER BY create_time ASC, slug ASC");

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), row_to_entry)?;
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

    /// Every entry in the store, **including tombstoned rows**, in slug order.
    /// Undo uses this to locate deleted entries by slug/hid before restoring
    /// them; `dm check` uses it for bulk integrity scans.
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

    /// Slugs of rows with `deleted_at` set (tombstones).
    pub fn tombstone_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT slug FROM entries WHERE deleted_at IS NOT NULL ORDER BY slug ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }
}
