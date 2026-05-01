//! Aggregations and consistency checks: counts, group-bys, distinct-fields,
//! id projections, time-order checks. Pure read paths.

use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::error::{DatumError, Result};

use super::types::{filtered_where, time_expr_endbound, GroupColumn, TimeOrderIssue};
use super::Db;

impl Db {
    /// Count of live (non-tombstoned) entries in the store.
    pub fn count_entries(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(n as usize)
    }

    /// Count live entries for `field` strictly before `t` (ordered by
    /// `start_time`, falling back to `create_time`). Used by the renderer to
    /// decide whether a given entry is the first-ever write for its field
    /// (triggers the `∅` / sparkle first-tracked markers).
    pub fn count_entries_before(
        &self,
        field: &str,
        t: DateTime<Utc>,
    ) -> Result<usize> {
        let sql = format!(
            "SELECT COUNT(*) FROM entries
             WHERE field = ?1
               AND deleted_at IS NULL
               AND {} < ?2",
            time_expr_endbound(),
        );
        let n: i64 = self.conn.query_row(
            &sql,
            params![field, t.to_rfc3339()],
            |row| row.get(0),
        )?;
        Ok(n as usize)
    }

    /// Count live entries, optionally filtered by `field` and/or a lower
    /// bound on occur-time (falling back to `create_time` when unset).
    pub fn count_entries_filtered(
        &self,
        field: Option<&str>,
        since: Option<DateTime<Utc>>,
    ) -> Result<usize> {
        let (where_clause, bindings) = filtered_where(field, since);
        let sql = format!("SELECT COUNT(*) FROM entries WHERE {where_clause}");
        let n: i64 = self.conn.query_row(
            &sql,
            rusqlite::params_from_iter(bindings.iter()),
            |row| row.get(0),
        )?;
        Ok(n as usize)
    }

    /// Group live entries by one of {`value`, `verb`, `field`} and return
    /// `(group_key, count)` pairs ordered by count DESC, key ASC. NULL
    /// group keys are emitted as the literal string `"null"`.
    pub fn count_entries_grouped(
        &self,
        field: Option<&str>,
        group_col: GroupColumn,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<(String, usize)>> {
        let col = group_col.as_sql();
        let (where_clause, bindings) = filtered_where(field, since);
        let sql = format!(
            "SELECT {col} AS g, COUNT(*) AS n FROM entries \
             WHERE {where_clause} GROUP BY {col} ORDER BY n DESC, g ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(bindings.iter()),
            |row| {
                let key: Option<String> = row.get(0)?;
                let n: i64 = row.get(1)?;
                Ok((key.unwrap_or_else(|| "null".to_string()), n as usize))
            },
        )?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// List live-entry identifiers (hid or slug, per `as_slug`) in
    /// occur-time order. Accepts optional field / `--since` filters and a
    /// cap. Ties broken by slug in the same direction as the primary sort.
    pub fn list_entry_ids(
        &self,
        field: Option<&str>,
        since: Option<DateTime<Utc>>,
        limit: Option<usize>,
        oldest_first: bool,
        as_slug: bool,
    ) -> Result<Vec<String>> {
        let select_col = if as_slug { "slug" } else { "hid" };
        let dir = if oldest_first { "ASC" } else { "DESC" };
        let (where_clause, bindings) = filtered_where(field, since);
        let mut sql = format!(
            "SELECT {select_col} FROM entries WHERE {where_clause} \
             ORDER BY {expr} {dir}, slug {dir}",
            expr = time_expr_endbound(),
        );
        if let Some(n) = limit {
            sql.push_str(&format!(" LIMIT {n}"));
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(bindings.iter()),
            |row| row.get::<_, String>(0),
        )?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Whether any live (non-tombstoned) row has this content_hash. Read-only
    /// dedup check used by `restore --dry-run` to project the outcome of an
    /// insert without opening a write transaction.
    pub fn has_entry_with_content_hash(&self, content_hash: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM entries
             WHERE content_hash = ?1 AND deleted_at IS NULL",
            params![content_hash],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// Rows where both time bounds exist and `end_time` precedes `start_time`.
    ///
    /// da-5ns sub-bead 1 stub: post-pivot there is no separate
    /// `end_time` column — duration is captured by the `dur` ISO
    /// string. A negative duration is rejected at parse time
    /// (sub-bead 3), so the legacy "end before start" anomaly cannot
    /// be expressed in the v=20 schema. Returns an empty vec; sub-bead
    /// 7 (`dm check --verbs`) covers the analogous drift category.
    pub fn find_time_end_before_start(&self) -> Result<Vec<TimeOrderIssue>> {
        Ok(Vec::new())
    }

    /// Distinct field names across all non-deleted entries.
    pub fn distinct_fields(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT field FROM entries WHERE deleted_at IS NULL ORDER BY field",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// All distinct fields that appear in the entry log (live rows only).
    /// Alphabetical. The `now` view feeds this into [`Db::state_at`] to
    /// build its row set.
    pub fn list_distinct_fields(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT field FROM entries
             WHERE deleted_at IS NULL
             ORDER BY field",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(DatumError::Db)?);
        }
        Ok(out)
    }
}
