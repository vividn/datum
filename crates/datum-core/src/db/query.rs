//! Aggregations and consistency checks: counts, group-bys, distinct-fields,
//! id projections, time-order checks. Pure read paths.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use rusqlite::params;

use crate::error::{DatumError, Result};
use crate::ids::Hid;

use super::spec::NOT_META_FIELD_SQL;
use super::types::{filtered_where, time_expr_endbound, GroupColumn, TimeOrderIssue, LIVE_SQL};
use super::Db;

/// How far back [`Db::field_frecency`] reads, in weeks.
pub const FRECENCY_HORIZON_WEEKS: i64 = 26;

impl Db {
    /// Count of live (non-tombstoned) entries in the store.
    pub fn count_entries(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM entries WHERE {LIVE_SQL}"),
            [],
            |row| row.get(0),
        )?;
        Ok(n as usize)
    }

    /// Count live entries for `field` strictly before `t` (ordered by
    /// `start_time`, falling back to `create_time`). Used by the renderer to
    /// decide whether a given entry is the first-ever write for its field
    /// (triggers the `∅` / sparkle first-tracked markers).
    pub fn count_entries_before(&self, field: &str, t: DateTime<Utc>) -> Result<usize> {
        let sql = format!(
            "SELECT COUNT(*) FROM entries
             WHERE field = ?1
               AND {LIVE_SQL}
               AND {} < ?2",
            time_expr_endbound(),
        );
        let n: i64 = self
            .conn
            .query_row(&sql, params![field, t.to_rfc3339()], |row| row.get(0))?;
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
        let n: i64 =
            self.conn
                .query_row(&sql, rusqlite::params_from_iter(bindings.iter()), |row| {
                    row.get(0)
                })?;
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
        let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), |row| {
            let key: Option<String> = row.get(0)?;
            let n: i64 = row.get(1)?;
            Ok((key.unwrap_or_else(|| "null".to_string()), n as usize))
        })?;
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
        let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), |row| {
            row.get::<_, String>(0)
        })?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Rows where both time bounds exist and `end_time` precedes `start_time`.
    ///
    /// da-5ns sub-bead 1 stub: post-pivot there is no separate
    /// `end_time` column — duration is captured by the `dur` ISO
    /// string. A negative duration is rejected at parse time
    /// (sub-bead 3), so the legacy "end before start" anomaly cannot
    /// be expressed in the v=20 schema. Returns an empty vec; sub-bead
    /// 7 (`dm check verbs`) covers the analogous drift category.
    pub fn find_time_end_before_start(&self) -> Result<Vec<TimeOrderIssue>> {
        Ok(Vec::new())
    }

    /// The hids of rows that have at least one supersedee — rows another
    /// row lost an identity contest to (da-uxgv).
    ///
    /// Served entirely by the `entries_superseded_by` partial index, which
    /// contains *only* conflicted rows. In steady state that index is
    /// empty, so a listing surface can ask this on every render and pay
    /// nothing when there is nothing to mark.
    pub fn hids_with_supersedees(&self) -> Result<HashSet<Hid>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT superseded_by FROM entries WHERE superseded_by IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, Hid>(0))?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Distinct field names across all live (non-deleted) entries,
    /// alphabetical. The `now` view feeds this into [`Db::state_at`] to
    /// build its row set. The meta fields (`spec`, `layout`) are not life
    /// fields and are left out (da-lc26c).
    ///
    /// da-vmjh: a skip-scan, not a scan. `SELECT DISTINCT field` walked
    /// every row of `entries_live_field_occur` to return the couple of
    /// dozen names it holds — 18 ms at 22k rows, and growing. Each step
    /// of the recursion instead seeks the index straight to the next
    /// name (`SEARCH … (field>?)`, served by SQLite's `min()`
    /// optimization), so the cost is O(fields x log N) and does not grow
    /// with the length of the record. Same rows, same order: the walk is
    /// ascending and `LIVE_SQL` is the same predicate.
    pub fn distinct_fields(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(&format!(
            "WITH RECURSIVE f(field) AS (
                 SELECT min(field) FROM entries WHERE {LIVE_SQL}
                 UNION ALL
                 SELECT (SELECT min(field) FROM entries
                          WHERE field > f.field AND {LIVE_SQL})
                   FROM f WHERE f.field IS NOT NULL
             )
             SELECT field FROM f WHERE field IS NOT NULL AND {NOT_META_FIELD_SQL}"
        ))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Every distinct `(field, state)` pair among live entries that carry a
    /// state, ordered by field. The state is the stored JSON, parsed.
    pub fn distinct_field_states(&self) -> Result<Vec<(String, serde_json::Value)>> {
        // da-c8mh: a TRACK document's state is a STATUS — the `false` or
        // TRACKED a declaration sits in — not a state the field has been
        // switched to, so it is no more evidence here than IGNORED is
        // (da-5ifp). `stated` is `entries_field_states`' WHERE, which the
        // index is used under only while the two match.
        //
        // da-asgt4: a skip scan — from each (field, state) seek the next,
        // the same field's next state or else the next field's first — so
        // the read costs the pairs returned, where a DISTINCT walked every
        // row of the history.
        let stated = format!(
            "{LIVE_SQL} AND state IS NOT NULL AND {}",
            crate::track::NOT_A_DECLARATION_SQL
        );
        let mut stmt = self.conn.prepare(&format!(
            "WITH RECURSIVE d(field, state) AS (
               SELECT * FROM (SELECT field, state FROM entries WHERE {stated}
                               ORDER BY field, state LIMIT 1)
               UNION ALL
               SELECT n.field, n.state FROM d JOIN entries n ON n.rowid = COALESCE(
                 (SELECT rowid FROM entries WHERE {stated}
                     AND field = d.field AND state > d.state
                   ORDER BY field, state LIMIT 1),
                 (SELECT rowid FROM entries WHERE {stated} AND field > d.field
                   ORDER BY field, state LIMIT 1))
             )
             SELECT field, state FROM d",
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|r| {
            let (field, state) = r.map_err(DatumError::Db)?;
            Ok((field, serde_json::from_str(&state)?))
        })
        .collect()
    }

    /// How much each field is used, recent use weighing more (da-xtkcp):
    /// every live entry counts `0.5^(age / 7 days)`, aged by its
    /// `create_time` in whole days as of `now`, summed per field. A
    /// declaration is not a use.
    ///
    /// Only rows touched in the last [`FRECENCY_HORIZON_WEEKS`] are read —
    /// an older one weighs under `0.5^26`, nothing — and that is a seek on
    /// `entries_modify_time_ms` (a row is modified no earlier than it is
    /// created), not a walk of the whole history. A field unused for that
    /// long is absent, which is a score of 0.
    pub fn field_frecency(&self, now: DateTime<Utc>) -> Result<HashMap<String, f64>> {
        let horizon = now - chrono::Duration::weeks(FRECENCY_HORIZON_WEEKS);
        let mut stmt = self.conn.prepare(&format!(
            "SELECT field, max(0, CAST(julianday(?1) - julianday(create_time) AS INTEGER)) AS age, \
                    COUNT(*) \
             FROM entries WHERE modify_time_ms >= ?2 AND {LIVE_SQL} AND {} \
             GROUP BY field, age",
            crate::track::NOT_A_DECLARATION_SQL
        ))?;
        let rows = stmt.query_map(
            params![now.to_rfc3339(), horizon.timestamp_millis()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?;
        let mut out = HashMap::new();
        for row in rows {
            let (field, age, n) = row.map_err(DatumError::Db)?;
            *out.entry(field).or_insert(0.0) += n as f64 * 0.5f64.powf(age as f64 / 7.0);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{Entry, Verb};
    use crate::time::DatumTime;
    use chrono::{TimeZone, Utc};

    fn push(db: &Db, field: &str, secs: i64) {
        let t =
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + chrono::Duration::seconds(secs);
        db.insert_entry(&Entry::new_legacy(
            field,
            Verb::Switch,
            Some(serde_json::json!("on")),
            None,
            Some(DatumTime {
                utc: t,
                offset_minutes: 0,
                timezone: Some("UTC".into()),
                uncertainty: None,
            }),
            None,
            "d1",
        ))
        .unwrap();
    }

    /// The skip-scan returns exactly what `SELECT DISTINCT … ORDER BY
    /// field` returned: every live field once, alphabetical, and nothing
    /// from a row the liveness predicate excludes.
    #[test]
    fn distinct_fields_skips_to_each_name_once_in_order() {
        let db = Db::open_in_memory().unwrap();
        for (i, f) in ["mood", "coffee", "mood", "apple", "coffee", "mood"]
            .iter()
            .enumerate()
        {
            push(&db, f, i as i64 * 60);
        }
        assert_eq!(
            db.distinct_fields().unwrap(),
            vec!["apple".to_string(), "coffee".into(), "mood".into()]
        );
    }

    #[test]
    fn distinct_fields_is_empty_on_an_empty_store() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.distinct_fields().unwrap().is_empty());
    }

    /// da-vmjh's gate for this read: every step must SEEK the next name,
    /// never walk the rows between. A `SCAN entries` line is the
    /// predecessor — `SELECT DISTINCT field` read all 22k rows to
    /// return 26 names.
    #[test]
    fn distinct_fields_seeks_and_never_scans_entries() {
        let db = Db::open_in_memory().unwrap();
        push(&db, "mood", 0);
        let sql = format!(
            "WITH RECURSIVE f(field) AS (
                 SELECT min(field) FROM entries WHERE {LIVE_SQL}
                 UNION ALL
                 SELECT (SELECT min(field) FROM entries
                          WHERE field > f.field AND {LIVE_SQL})
                   FROM f WHERE f.field IS NOT NULL
             )
             SELECT field FROM f WHERE field IS NOT NULL AND {NOT_META_FIELD_SQL}"
        );
        let mut stmt = db
            .conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap();
        let plan = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<String>>>()
            .unwrap()
            .join("\n");
        assert!(
            plan.contains("SEARCH entries USING INDEX entries_live_field_occur"),
            "expected an index seek per name, got:\n{plan}"
        );
        assert!(
            !plan.contains("SCAN entries"),
            "a SCAN of entries is the whole-history read this replaced, got:\n{plan}"
        );
    }
}
