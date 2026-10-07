//! Multi-row read paths: per-field lists, time-window lists, and the unified
//! `list_entries` query that backs `tail`/`head`.

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde_json::Value as JsonValue;

use crate::entry::Entry;
use crate::error::{DatumError, Result};

use super::row_mapper::row_to_entry;
use super::spec::NOT_META_FIELD_SQL;
use super::types::{time_expr, time_expr_endbound, ListParams, TimeMetric, LIVE_SQL};
use super::Db;

impl Db {
    /// Query live entries for a field, ordered by start_time descending.
    pub fn entries_for_field(&self, field: &str, limit: Option<usize>) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1 AND {LIVE_SQL}
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
             WHERE field = ?1 AND {LIVE_SQL}
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
        let (sql, bindings) = list_entries_sql(params);
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

    /// The distinct `(state, client_id)`, among `states`, of the rows
    /// [`Self::list_entries`] filters in but its time bounds leave out,
    /// declarations aside ([`crate::track::NOT_A_DECLARATION_SQL`]): what a
    /// reader of the window needs of the rest of the history without
    /// reading its rows (da-vq29c). A seek per state on
    /// `entries_field_states`, so it costs those states' rows and not the
    /// field's history (da-cwpr1). Empty when `params` is unbounded.
    pub fn distinct_states_outside(
        &self,
        params: &ListParams,
        states: &[JsonValue],
    ) -> Result<Vec<(Option<JsonValue>, String)>> {
        let Some((window, window_bindings)) = list_window_sql(params) else {
            return Ok(Vec::new());
        };
        let (filter, mut bindings) = list_filter_sql(params);
        let among = vec!["?"; states.len()].join(", ");
        bindings.extend(states.iter().map(JsonValue::to_string));
        bindings.extend(window_bindings);
        let sql = format!(
            "SELECT DISTINCT state, client_id FROM entries
             WHERE {filter} AND state IN ({among}) AND {} AND NOT ({window})",
            crate::track::NOT_A_DECLARATION_SQL
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), |row| {
            let state =
                (row.get::<_, Option<String>>(0)?).and_then(|s| serde_json::from_str(&s).ok());
            Ok((state, row.get(1)?))
        })?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Live entries whose `create_time` is strictly greater than `since`,
    /// optionally filtered by `field`. Ordered oldest-first so `tail --watch`
    /// can append rows as they arrive.
    pub fn list_entries_created_after(
        &self,
        fields: &[&str],
        since: DateTime<Utc>,
    ) -> Result<Vec<Entry>> {
        let mut sql = format!("SELECT * FROM entries WHERE {LIVE_SQL} AND create_time > ?");
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
            .prepare(&format!(
                "SELECT * FROM entries WHERE {LIVE_SQL} ORDER BY slug ASC"
            ))
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

    /// Every entry in the store, **including tombstoned and superseded
    /// rows**, in slug order.
    /// Undo uses this to locate deleted entries by slug/hid before restoring
    /// them; `dm check` uses it for bulk integrity scans.
    pub fn list_including_deleted(&self) -> Result<Vec<Entry>> {
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

/// The SQL + bindings for [`Db::list_entries`], split out so the plan can
/// be asserted (da-vmjh). The window read every render surface makes —
/// `TimeMetric::Hybrid`, which resolves to `occur_time`, bounded at both
/// ends — must be served by `entries_occur_time`, the partial
/// `(occur_time, slug) WHERE deleted_at IS NULL` index: it covers the
/// range AND the whole ORDER BY, so the read touches only the rows in
/// the window and never sorts.
pub(super) fn list_entries_sql(params: &ListParams) -> (String, Vec<String>) {
    let metric_expr: &str = time_expr(params.metric);
    let (filter, mut bindings) = list_filter_sql(params);
    let mut sql = format!("SELECT * FROM entries WHERE {filter}");
    if let Some((window, b)) = list_window_sql(params) {
        sql.push_str(&format!(" AND {window}"));
        bindings.extend(b);
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

    (sql, bindings)
}

/// [`list_entries_sql`]'s WHERE clause but for its time bounds.
fn list_filter_sql(params: &ListParams) -> (String, Vec<String>) {
    let mut sql = LIVE_SQL.to_string();
    let mut bindings: Vec<String> = Vec::new();

    // da-5ns / da-r17h: a timeless row (bare `add`/`note`, or any
    // `-T`/--no-timestamp insert) stores `occur_time IS NULL` (its
    // slug is `field:c<create_time>`, not a stub timestamp). The Occur
    // metric (chronological "when did this happen") must skip those
    // rows; Modify / Hybrid still see them.
    if params.metric == TimeMetric::Occur {
        sql.push_str(" AND occur_time IS NOT NULL");
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
    // da-lc26c: entries that describe the database — specs, layouts —
    // are not life data either. Naming the field (`tail spec`) asks for it.
    if !params.include_meta && params.fields.is_empty() {
        sql.push_str(&format!(" AND {NOT_META_FIELD_SQL}"));
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
    (sql, bindings)
}

/// [`list_entries_sql`]'s time bounds, `None` when it has none.
fn list_window_sql(params: &ListParams) -> Option<(String, Vec<String>)> {
    let metric_expr: &str = time_expr(params.metric);
    let bounds = [(">=", params.start_utc), ("<=", params.end_utc)];
    let (sql, bindings): (Vec<String>, Vec<String>) = (bounds.into_iter())
        .filter_map(|(op, t)| Some((format!("{metric_expr} {op} ?"), t?.to_rfc3339())))
        .unzip();
    (!sql.is_empty()).then(|| (sql.join(" AND "), bindings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::types::TimeMetric;

    /// da-vmjh item 4: the window read every render surface makes is
    /// index-bounded, and was already. `dayview` / `nowview` fetch
    /// `TimeMetric::Hybrid` between two instants with no field filter;
    /// `entries_occur_time` — partial `(occur_time, slug) WHERE
    /// deleted_at IS NULL` — covers the range and the whole ORDER BY, so
    /// the plan is one SEARCH with no sort and no scan. Recorded here so
    /// a later edit to the predicate (a metric change, a new default
    /// filter that is not sargable) cannot quietly turn the last-few-days
    /// read back into a whole-history one.
    #[test]
    fn the_render_window_read_is_index_bounded() {
        let db = Db::open_in_memory().unwrap();
        let params = ListParams {
            metric: TimeMetric::Hybrid,
            start_utc: Some("2026-09-12T00:00:00Z".parse().unwrap()),
            end_utc: Some("2026-09-14T12:00:00Z".parse().unwrap()),
            ..Default::default()
        };
        let (sql, bindings) = list_entries_sql(&params);
        let mut stmt = db
            .conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap();
        let plan = stmt
            .query_map(rusqlite::params_from_iter(bindings.iter()), |r| {
                r.get::<_, String>(3)
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<String>>>()
            .unwrap()
            .join("\n");
        assert!(
            plan.contains("SEARCH entries USING INDEX entries_occur_time"),
            "expected the window to be walked on entries_occur_time, got:\n{plan}"
        );
        assert!(
            !plan.contains("SCAN entries"),
            "a SCAN reads the whole history for a two-day window, got:\n{plan}"
        );
        assert!(
            !plan.contains("TEMP B-TREE"),
            "the index covers the ORDER BY; a sort means it stopped doing so, got:\n{plan}"
        );
    }
}
