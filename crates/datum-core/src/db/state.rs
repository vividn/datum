//! State-bearing queries: `state_at`, `batch_state_at`, the `now`-view
//! projection, and per-state color reads/writes.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

use crate::error::{DatumError, Result};
use crate::field_spec::FieldSpec;

use super::types::{
    time_expr_endbound, TrackedField, STATE_ORDER_NEWEST_FIRST, STATE_ORDER_OLDEST_FIRST,
    STATE_PARTICIPATION_SQL,
};
use super::Db;

impl Db {
    /// Load the field's spec, or mint one carrying the verb the field's
    /// log implies. Used by the color setters, which may need a spec on
    /// first write: a colour says nothing about what a press means, but
    /// every spec has to answer that anyway (da-c8mh), so the log does.
    pub(super) fn load_or_new_spec(&self, field: &str) -> Result<FieldSpec> {
        match self.load_spec(field)? {
            Some(s) => Ok(s),
            None => Ok(FieldSpec::new(field, self.field_verb(field)?)),
        }
    }

    /// Return the field's most recent state-bearing value at or before time
    /// `t`. Row participation and ordering follow the canonical rule
    /// ([`super::types::STATE_PARTICIPATION_SQL`]): live, occur_time-bearing
    /// rows, newest `occur_time` first with a slug tiebreak. Delegates to
    /// [`Db::batch_state_at`], and through it to the one oracle
    /// ([`crate::state::resolve_effective`], da-u5l1o).
    ///
    /// da-2i8g: when the winning row is a block-switch / point-switch
    /// whose trailing edge (`occur_time + dur`) is at or before `t`, the
    /// row's `following_state` is the effective value — same rule as
    /// `now`.
    ///
    /// da-phoe: when the winning row is instead a plain BLOCK past its
    /// trailing edge, it has HANDED THE FIELD BACK — `following_state IS
    /// NULL` means "resume whatever preceded me" — so the read walks
    /// behind it to the row that actually decides the state. Both rules
    /// live in [`crate::state::resolve_effective`], which `now` and the
    /// per-field cache [`crate::state::FieldStateRows`] read through too.
    ///
    /// `Ok(None)` means the field has no qualifying entries at or before
    /// `t` — nothing has said to ignore it, so callers read it as TRACKED
    /// (da-m1l8). `Ok(Some(JsonValue::String("IGNORED")))` is the explicit
    /// ignore sentinel; any other value (including `Null` from a "track,
    /// no current state" entry) means the field is tracked.
    pub fn state_at(&self, field: &str, t: DateTime<Utc>) -> Result<Option<JsonValue>> {
        Ok(self.batch_state_at(&[field], t)?.remove(field))
    }

    /// Batch version of [`Db::state_at`]: latest state-bearing `value` for
    /// every field in `fields`, evaluated at time `t`. Fields with no
    /// qualifying entry are absent from the returned map. A present entry
    /// whose `value` column was NULL maps to [`JsonValue::Null`].
    ///
    /// Avoids the N+1 query pattern in callers that scan every distinct
    /// field in the store (see [`Db::get_tracked_fields`]).
    pub fn batch_state_at(
        &self,
        fields: &[&str],
        t: DateTime<Utc>,
    ) -> Result<std::collections::HashMap<String, JsonValue>> {
        Ok(self
            .batch_governing_at(fields, t)?
            .into_iter()
            .map(|(field, (state, _))| (field, state))
            .collect())
    }

    /// [`Db::batch_state_at`] with the GOVERNING ROW named: for each
    /// field, the state it holds at `t` and the whole hid of the entry
    /// that established it (da-6hg6v) — the row a picture's mark points
    /// at when it is the open block of that state. The id is `None` when
    /// the state came from no single row: every row handed the field
    /// back, and nothing preceded them.
    ///
    /// This is the walk; [`Db::batch_state_at`] is this with the id
    /// dropped, so the two cannot disagree about which row won.
    pub fn batch_governing_at(
        &self,
        fields: &[&str],
        t: DateTime<Utc>,
    ) -> Result<std::collections::HashMap<String, (JsonValue, Option<String>)>> {
        // One row per field answers the common case — and is the whole
        // of the pre-da-phoe cost. A field whose newest row is a block
        // that has already handed the field back needs the row behind
        // it, so those fields (only) are re-read to depth.
        let mut rows = self.state_rows_at(fields, t, 1)?;
        let deep: Vec<&str> = fields
            .iter()
            .copied()
            .filter(|f| {
                rows.get(*f)
                    .and_then(|r| r.first())
                    .is_some_and(|row| row.hands_field_back(t))
            })
            .collect();
        if !deep.is_empty() {
            rows.extend(self.state_rows_at(&deep, t, crate::state::RESUME_LOOKBACK)?);
        }
        // The reads leave occur dots out; a field an occur moves (IGNORED,
        // or no state at all — a field of nothing but dots included) needs
        // the first one after that took effect, which turns it TRACKED.
        let mut out = std::collections::HashMap::with_capacity(rows.len());
        for field in dedup_fields(fields) {
            let mut rows = rows.remove(field).unwrap_or_default();
            let mut eff = crate::state::resolve_effective(&rows, t);
            if eff.an_occur_tracks() {
                if let Some(dot) = self.first_dot_row_after(field, eff.since, t)? {
                    rows.push(dot);
                    eff = crate::state::resolve_effective(&rows, t);
                }
            }
            if rows.is_empty() {
                continue;
            }
            let id = eff.governing.map(|i| rows[i].id.clone());
            out.insert(field.to_string(), (eff.state, id));
        }
        Ok(out)
    }

    /// [`Db::first_dot_after`] in the state columns alone, as the other
    /// state reads select them.
    fn first_dot_row_after(
        &self,
        field: &str,
        after: Option<DateTime<Utc>>,
        at: DateTime<Utc>,
    ) -> Result<Option<crate::state::StateRow>> {
        use rusqlite::OptionalExtension;
        let after = after.map(|t| t.to_rfc3339()).unwrap_or_default();
        let columns = self
            .conn
            .query_row(
                &super::lookup::first_dot_sql(
                    "field, state, verb, occur_time, dur, following_state, hid",
                ),
                rusqlite::params![field, after, at.to_rfc3339()],
                read_state_columns,
            )
            .optional()?;
        Ok(match columns {
            Some(cols) => Some(decode_state_row(cols)?.1),
            None => None,
        })
    }

    /// The newest `depth` state-participating rows for every field in
    /// `fields`, evaluated at `t`, newest first — the column-level input
    /// [`crate::state::resolve_effective`] reads. Fields with no
    /// qualifying entry are absent from the map.
    ///
    /// da-vmjh: one index-bounded `ORDER BY … LIMIT depth` PER FIELD,
    /// looped over a single prepared statement. The predecessor asked
    /// for all fields at once and ranked them with `ROW_NUMBER() OVER
    /// (PARTITION BY field …)`, filtering `rn <= depth` OUTSIDE the
    /// window — so SQLite numbered every row of every field back to the
    /// beginning of history just to keep one per field, and the cost of
    /// reading "what is the state right now" grew with the length of the
    /// record (77 ms at 22k rows, 0.9 ms this way). Per field the
    /// planner walks `entries_live_field_occur` — the partial
    /// `(field, occur_time) WHERE deleted_at IS NULL` index, whose
    /// predicate [`STATE_PARTICIPATION_SQL`] implies — backwards from
    /// the cutoff and stops after `depth` rows.
    fn state_rows_at(
        &self,
        fields: &[&str],
        t: DateTime<Utc>,
        depth: usize,
    ) -> Result<std::collections::HashMap<String, Vec<crate::state::StateRow>>> {
        let mut out: std::collections::HashMap<String, Vec<crate::state::StateRow>> =
            std::collections::HashMap::with_capacity(fields.len());
        if fields.is_empty() {
            return Ok(out);
        }
        let cutoff = t.to_rfc3339();
        let mut stmt = self.conn.prepare(&state_rows_sql(depth))?;
        for field in dedup_fields(fields) {
            let rows = stmt.query_map(rusqlite::params![field, cutoff], read_state_columns)?;
            for r in rows {
                let (field, row) = decode_state_row(r.map_err(DatumError::Db)?)?;
                out.entry(field).or_default().push(row);
            }
        }
        self.append_spanning_blocks(fields, t, &mut out)?;
        Ok(out)
    }

    /// Append, per field, the dur-bearing rows stored AFTER `t` whose
    /// span still covers it. da-zwct: a block's `occur_time` is its END,
    /// so the row holding a field at `t` can sit in `t`'s future and the
    /// `occur_time <= t` window above would never see it. Mirrors
    /// [`Db::blocks_spanning`] for the column-level reader; the leading
    /// edge is re-derived in Rust because `dur` is an ISO-8601 string
    /// SQL cannot subtract from a time. [`crate::state::resolve_effective`]
    /// re-orders the merged rows on that edge.
    ///
    /// da-vmjh: same per-field `ORDER BY … LIMIT` shape as
    /// [`Db::state_rows_at`], and for the same reason — this one walked
    /// forward from `t` over every future row of every field.
    fn append_spanning_blocks(
        &self,
        fields: &[&str],
        t: DateTime<Utc>,
        out: &mut std::collections::HashMap<String, Vec<crate::state::StateRow>>,
    ) -> Result<()> {
        let cutoff = t.to_rfc3339();
        let mut stmt = self
            .conn
            .prepare(&spanning_blocks_sql(crate::state::SPAN_LOOKAHEAD))?;
        for field in dedup_fields(fields) {
            let rows = stmt.query_map(rusqlite::params![field, cutoff], read_state_columns)?;
            for r in rows {
                let (field, row) = decode_state_row(r.map_err(DatumError::Db)?)?;
                if row.leading_edge().is_some_and(|edge| edge <= t) {
                    out.entry(field).or_default().push(row);
                }
            }
        }
        Ok(())
    }

    /// Resolve the `n`th most-recent DISTINCT previous state of `field`,
    /// walking back through its state-transition history. Backs the
    /// `_` / `__` / `_n` switch-state ordinals (`_` = 1 back, `__` = 2,
    /// `_n` = n) — one-keystroke toggle/cycle for stateful fields:
    ///
    /// (history written oldest→newest, so the current state is the last
    /// `A`; the walk returns the most recent distinct alternative first):
    /// - `[A, B, A, C, A]` → `resolve_state_back(1)` → `C`
    /// - `[A, B, A, C, A]` → `resolve_state_back(2)` → `B`
    /// - `[A, B, A, C, A]` → `resolve_state_back(3)` → `None`
    ///
    /// The walk reads the field's state TIMELINE — the one oracle,
    /// [`crate::state::resolve_effective`], read at every row's edges
    /// ([`crate::state::FieldStateRows::changes`], da-u5l1o). Consequences:
    /// - the "current" anchor is the state at `at` — a compound switch
    ///   past its trailing edge anchors on its `following_state`, and a
    ///   block that has ended anchors on the state it handed back to,
    ///   exactly like `now`;
    /// - a `following_state` is an ordinal candidate in its own right;
    /// - changes after `at` (e.g. the trailing edge of a still-running
    ///   block-switch) are invisible;
    /// - what an occur-shaped row sets ([`crate::state::StateRow::is_annotation`]:
    ///   a dot, a stateless block) is not somewhere to go back to, and is
    ///   skipped; an explicit JSON `null` state is a state like any other;
    /// - a zero-span compound switch holds its primary state for no time
    ///   at all, so only its `following_state` is ever held.
    ///
    /// States compare in normalized form ([`crate::state::normalize_state`]).
    ///
    /// Array states are ONE OPAQUE entity by design — `switch body a,b`
    /// participates as the single value `["a","b"]`, never element-wise.
    /// This is a key feature: `switch body _` returns to a parallel
    /// multi-state without retyping it.
    pub fn resolve_state_back(
        &self,
        field: &str,
        n: usize,
        at: DateTime<Utc>,
    ) -> Result<Option<JsonValue>> {
        if n == 0 {
            return Ok(None);
        }
        // 256 entries gives plenty of headroom for a heavily-toggled field;
        // the SQL is field-scoped and indexed, so the cost is negligible.
        let entries = self.entries_for_field(field, Some(256))?;
        let rows = crate::state::FieldStateRows::from_entries(&entries);
        let held: Vec<JsonValue> = rows
            .changes()
            .into_iter()
            .filter(|c| c.at <= at && !c.row.is_annotation())
            .map(|c| crate::state::normalize_state(&c.state).to_json())
            .collect();
        let mut walk = held.iter().rev();
        let Some(current) = walk.next() else {
            return Ok(None);
        };
        let mut alternatives: Vec<JsonValue> = Vec::new();
        for s in walk {
            if s == current || alternatives.iter().any(|x| x == s) {
                continue;
            }
            alternatives.push(s.clone());
            if alternatives.len() == n {
                return Ok(alternatives.pop());
            }
        }
        Ok(None)
    }

    /// Project the entry log into a [`TrackedField`] list for the `now`
    /// view. A field appears here iff it has at least one live entry or a
    /// declared spec; its `ignored` flag derives from [`Db::state_at`] at
    /// `at` — pass `Utc::now()` for the live view, or a time-travel anchor
    /// (da-n0dr) so scope membership matches the per-field states at that
    /// moment. Per-field color is read from the matching [`FieldSpec`]
    /// when one exists. Ordered alphabetically by field name.
    ///
    /// da-m1l8: this is the SAME list, under the same rule, that `datum
    /// fields --json` emits — one definition of "ignored", not two. A
    /// field is IGNORED only when its latest state-bearing row says so;
    /// a field with no such row at all (every entry timeless, or nothing
    /// but a spec) is TRACKED, so it keeps its place in `info`'s count
    /// and in the front ends that offer it as a button.
    ///
    /// The one thing absence still decides is MEMBERSHIP (da-n0dr): a
    /// field none of whose rows had happened by `at` did not exist in the
    /// world as of `at`, and is left out of the list rather than
    /// classified either way. At `at = Utc::now()` — every caller but
    /// `now`'s time travel — that excludes nothing.
    pub fn get_tracked_fields(&self, at: DateTime<Utc>) -> Result<Vec<TrackedField>> {
        let logged: std::collections::BTreeSet<String> =
            self.distinct_fields()?.into_iter().collect();
        let mut names = logged.clone();
        names.extend(self.spec_fields()?);
        let fields: Vec<String> = names.into_iter().collect();
        let field_refs: Vec<&str> = fields.iter().map(String::as_str).collect();
        let states = self.batch_state_at(&field_refs, at)?;
        let specs = self.batch_load_specs(&field_refs)?;
        // Only the fields with no state at `at` need their existence
        // established — every other field has just proved it with a row.
        let stateless: Vec<&str> = field_refs
            .iter()
            .copied()
            .filter(|f| !states.contains_key(*f))
            .collect();
        let existed = self.fields_seen_by(&stateless, at)?;
        let mut out = Vec::with_capacity(fields.len());
        for field in fields {
            let ignored = match states.get(&field) {
                Some(state) => crate::shape::is_ignored_state(state),
                // A spec with nothing logged behind it is a standing
                // declaration, not an event, so it vouches for the field at
                // any anchor (da-m1l8). Once the field HAS rows, they decide
                // when it entered the world — every logged field carries a
                // spec now (da-c8mh), so the spec alone cannot vouch.
                None if existed.contains(&field)
                    || (specs.contains_key(&field) && !logged.contains(&field)) =>
                {
                    false
                }
                None => continue,
            };
            let color = specs.get(&field).and_then(|s| s.color.clone());
            out.push(TrackedField {
                field,
                ignored,
                color,
            });
        }
        Ok(out)
    }

    /// Which of `fields` had any live row of their own at or before `at`.
    /// A timeless row (no `occur_time`) enters the world when it is
    /// written, so `create_time` stands in for it.
    fn fields_seen_by(
        &self,
        fields: &[&str],
        at: DateTime<Utc>,
    ) -> Result<std::collections::HashSet<String>> {
        let mut out = std::collections::HashSet::new();
        if fields.is_empty() {
            return Ok(out);
        }
        let placeholders = std::iter::repeat_n("?", fields.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT DISTINCT field FROM entries \
             WHERE field IN ({placeholders}) AND {live} \
               AND COALESCE(occur_time, create_time) <= ?{n}",
            live = super::types::LIVE_SQL,
            n = fields.len() + 1
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<String> = fields
            .iter()
            .map(|f| (*f).to_string())
            .chain(std::iter::once(at.to_rfc3339()))
            .collect();
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
            row.get::<_, String>(0)
        })?;
        for r in rows {
            out.insert(r.map_err(DatumError::Db)?);
        }
        Ok(out)
    }

    /// Set a per-state color on a field. Loads/creates the spec first.
    pub fn set_state_color(&self, field: &str, state: &str, color: &str) -> Result<()> {
        let mut spec = self.load_or_new_spec(field)?;
        spec.state_colors
            .insert(state.to_string(), color.to_string());
        self.save_field_spec(&spec)
    }

    /// Return all state color overrides as a (field, state) → color map.
    pub fn get_all_state_colors(
        &self,
    ) -> Result<std::collections::HashMap<(String, String), String>> {
        let mut map = std::collections::HashMap::new();
        for spec in self.stored_specs("")? {
            for (state, color) in spec.state_colors {
                map.insert((spec.field.clone(), state), color);
            }
        }
        Ok(map)
    }
}

/// The `fields` list with repeats removed, order preserved. The
/// predecessor's `field IN (…)` deduped for free; the per-field loop that
/// replaced it would otherwise read a repeated name twice and hand
/// [`crate::state::resolve_effective`] a doubled row list.
fn dedup_fields<'a>(fields: &[&'a str]) -> Vec<&'a str> {
    let mut seen = std::collections::HashSet::with_capacity(fields.len());
    fields.iter().copied().filter(|f| seen.insert(*f)).collect()
}

/// The single-field state read (da-vmjh): the newest `depth`
/// state-participating rows at or before `?2`, newest first. Bound
/// `?1` = field, `?2` = the RFC-3339 cutoff.
///
/// The shape is the point: an equality on `field` plus a range on
/// `occur_time`, ordered the way `entries_live_field_occur` already
/// stores them, so the planner SEARCHes that index and stops at
/// `LIMIT` — `USE TEMP B-TREE FOR LAST TERM OF ORDER BY` is only the
/// slug tiebreak sorting within one `occur_time`, which still lets the
/// scan terminate early. A plan that SCANs a subquery instead has read
/// the whole history.
pub(super) fn state_rows_sql(depth: usize) -> String {
    use crate::state::OCCUR_DOT_SQL;
    // da-2i8g: each row's verb / occur_time / dur / following_state come
    // along so the trailing-edge rules can be applied in Rust — `dur` is
    // an ISO-8601 string, which SQL cannot add to a time.
    let endbound = time_expr_endbound();
    format!(
        "SELECT field, state, verb, occur_time, dur, following_state, hid
           FROM entries
          WHERE field = ?1
            AND {STATE_PARTICIPATION_SQL}
            AND NOT {OCCUR_DOT_SQL}
            AND {endbound} <= ?2
          ORDER BY {STATE_ORDER_NEWEST_FIRST}
          LIMIT {depth}"
    )
}

/// The single-field spanning-block read (da-vmjh): the oldest `limit`
/// dur-bearing rows stored after `?2`, oldest first. Same index and same
/// early exit as [`state_rows_sql`], walked the other way.
pub(super) fn spanning_blocks_sql(limit: usize) -> String {
    use crate::state::OCCUR_DOT_SQL;
    let endbound = time_expr_endbound();
    format!(
        "SELECT field, state, verb, occur_time, dur, following_state, hid
           FROM entries
          WHERE field = ?1
            AND {STATE_PARTICIPATION_SQL}
            AND NOT {OCCUR_DOT_SQL}
            AND dur IS NOT NULL
            AND {endbound} > ?2
          ORDER BY {STATE_ORDER_OLDEST_FIRST}
          LIMIT {limit}"
    )
}

/// The seven columns both state-row readers select, decoded as far as
/// rusqlite alone can take them. The JSON columns stay as strings here so
/// a parse failure surfaces as a [`DatumError`] rather than being flattened
/// into a rusqlite error — [`decode_state_row`] finishes the job.
pub(crate) type StateColumns = (
    String,
    Option<String>,
    crate::entry::Verb,
    Option<DateTime<Utc>>,
    Option<crate::dur::Dur>,
    Option<String>,
    String,
);

pub(crate) fn read_state_columns(row: &rusqlite::Row) -> rusqlite::Result<StateColumns> {
    let field: String = row.get(0)?;
    let value: Option<String> = row.get(1)?;
    let verb: crate::entry::Verb = row.get::<_, String>(2)?.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, e.into())
    })?;
    let occur_utc: Option<DateTime<Utc>> = row
        .get::<_, Option<String>>(3)?
        .and_then(|s| s.parse().ok());
    let dur: Option<crate::dur::Dur> = row
        .get::<_, Option<String>>(4)?
        .as_deref()
        .and_then(crate::dur::Dur::parse);
    let following: Option<String> = row.get(5)?;
    let id: String = row.get(6)?;
    Ok((field, value, verb, occur_utc, dur, following, id))
}

pub(crate) fn decode_state_row(cols: StateColumns) -> Result<(String, crate::state::StateRow)> {
    let (field, value, verb, occur, dur, following, id) = cols;
    let state: Option<JsonValue> = match value {
        None => None,
        Some(s) => Some(serde_json::from_str(&s)?),
    };
    let following_state: Option<JsonValue> = match following {
        None => None,
        Some(s) => Some(serde_json::from_str(&s)?),
    };
    Ok((
        field,
        crate::state::StateRow {
            id,
            verb,
            state,
            occur,
            dur,
            following_state,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{Entry, Verb};
    use crate::shape::IGNORED_STATE_VALUE;
    use crate::time::DatumTime;
    use chrono::TimeZone;
    use serde_json::json;

    fn at(secs_offset: i64) -> DatumTime {
        let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        DatumTime {
            utc: base + chrono::Duration::seconds(secs_offset),
            offset_minutes: 0,
            timezone: Some("UTC".into()),
            uncertainty: None,
        }
    }

    /// Ordinal-walk anchor comfortably past every event the tests push.
    fn probe() -> DateTime<Utc> {
        at(1_000_000).utc
    }

    fn push_switch(db: &Db, field: &str, state: serde_json::Value, secs_offset: i64) {
        let entry = Entry::new_legacy(
            field,
            Verb::Switch,
            Some(state),
            None,
            Some(at(secs_offset)),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();
    }

    fn push_point(db: &Db, field: &str, secs_offset: i64) {
        // Mirror a real `occur` row: state=true + Moment dur derives
        // Verb::Point (da-ju71.2). Plain state=true with NO dur would
        // derive Verb::Switch — a genuine "switch to active" transition,
        // which SHOULD participate in the walk.
        let entry = Entry::new(
            field,
            Verb::Point,
            Some(json!(true)),
            None,
            Some(at(secs_offset)),
            Some(crate::dur::Dur::moment()),
            "d1",
        );
        db.insert_entry(&entry).unwrap();
    }

    fn push_moment_switch(db: &Db, field: &str, state: serde_json::Value, secs_offset: i64) {
        // Mirror `switch x --moment`: a real state + Moment dur derives
        // Verb::Point too — but it IS a genuine zero-length transition.
        let entry = Entry::new(
            field,
            Verb::Point,
            Some(state),
            None,
            Some(at(secs_offset)),
            Some(crate::dur::Dur::moment()),
            "d1",
        );
        db.insert_entry(&entry).unwrap();
    }

    /// Every line of `EXPLAIN QUERY PLAN` for `sql`, joined.
    fn plan_of(db: &Db, sql: &str) -> String {
        let mut stmt = db
            .conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap();
        let rows = stmt
            .query_map(rusqlite::params!["mood", "2026-01-01T00:00:00Z"], |r| {
                r.get::<_, String>(3)
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<String>>>()
            .unwrap();
        rows.join("\n")
    }

    /// The gate for da-vmjh. Both state reads must be able to STOP at
    /// their `LIMIT`, and the plan says whether they can.
    ///
    /// `SEARCH … USING INDEX entries_field_state` is the seek: the partial
    /// index of the rows that move a state, whose predicate
    /// [`STATE_PARTICIPATION_SQL`] and `NOT` [`crate::state::OCCUR_DOT_SQL`]
    /// imply. A `SCAN` line is the discriminator — the predecessor ranked
    /// every row of every field with `ROW_NUMBER() OVER (PARTITION BY field
    /// …)` and filtered `rn <= depth` outside the window, so its plan read
    /// `CO-ROUTINE (subquery-N)` / `SCAN (subquery-N)` / `USE TEMP B-TREE
    /// FOR ORDER BY`: the whole history materialized and sorted before one
    /// row per field was kept. Neither may reappear.
    ///
    /// No temp b-tree at all (da-asgt4): the index carries `slug`, the
    /// tiebreak. `entries_live_field_occur`, which these reads used before
    /// it, sorted each equal-`occur_time` block, and on a field of occur
    /// dots alone walked to the field's first row looking for a state.
    #[test]
    fn state_reads_stop_at_their_limit() {
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("happy"), 100);

        for (label, sql) in [
            ("state_rows_sql", state_rows_sql(1)),
            (
                "state_rows_sql deep",
                state_rows_sql(crate::state::RESUME_LOOKBACK),
            ),
            (
                "spanning_blocks_sql",
                spanning_blocks_sql(crate::state::SPAN_LOOKAHEAD),
            ),
        ] {
            let plan = plan_of(&db, &sql);
            assert!(
                plan.contains("SEARCH entries USING INDEX entries_field_state "),
                "{label}: expected a SEARCH on entries_field_state, got:\n{plan}"
            );
            assert!(
                !plan.contains("SCAN"),
                "{label}: a SCAN means the read cannot stop at its LIMIT, got:\n{plan}"
            );
            assert!(
                !plan.contains("USE TEMP B-TREE"),
                "{label}: a sort means more of the history is read than the LIMIT, \
                 got:\n{plan}"
            );
        }
    }

    /// The per-field loop must not double a repeated name — `field IN
    /// (…)` used to dedupe for free.
    #[test]
    fn repeated_field_reads_once() {
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("happy"), 100);
        let rows = db.state_rows_at(&["mood", "mood"], probe(), 4).unwrap();
        assert_eq!(rows["mood"].len(), 1);
    }

    #[test]
    fn state_back_returns_none_when_history_empty() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.resolve_state_back("mood", 1, probe()).unwrap(), None);
    }

    #[test]
    fn state_back_returns_none_for_zero_ordinal() {
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("happy"), 100);
        assert_eq!(db.resolve_state_back("mood", 0, probe()).unwrap(), None);
    }

    #[test]
    fn state_back_walks_distinct_alternatives() {
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("a"), 100);
        push_switch(&db, "mood", json!("b"), 200);
        push_switch(&db, "mood", json!("c"), 300);
        push_switch(&db, "mood", json!("b"), 400); // current = b
        assert_eq!(
            db.resolve_state_back("mood", 1, probe()).unwrap(),
            Some(json!("c")),
            "_ returns the most recent distinct non-current state"
        );
        assert_eq!(
            db.resolve_state_back("mood", 2, probe()).unwrap(),
            Some(json!("a")),
            "__ returns the second-most recent distinct non-current state"
        );
        assert_eq!(
            db.resolve_state_back("mood", 3, probe()).unwrap(),
            None,
            "___ runs out when only two distinct alternatives exist"
        );
    }

    #[test]
    fn state_back_toggle_loop_yields_single_alternative() {
        let db = Db::open_in_memory().unwrap();
        for (i, s) in ["a", "b", "a", "b", "a", "b", "a"].iter().enumerate() {
            push_switch(&db, "mood", json!(s), 100 + i as i64 * 10);
        }
        // Current = a; only one distinct non-current state exists (b).
        assert_eq!(
            db.resolve_state_back("mood", 1, probe()).unwrap(),
            Some(json!("b"))
        );
        assert_eq!(db.resolve_state_back("mood", 2, probe()).unwrap(), None);
    }

    #[test]
    fn state_back_treats_json_null_as_distinct_state() {
        // JSON `null` is a legitimate (if unusual) state value. It's not
        // the same as SQL NULL (no state column at all — those rows are
        // skipped); the user CAN explicitly land a row at state=null.
        // Treat it as a normal alternative — the walk doesn't skip it.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("happy"), 100);
        push_switch(&db, "mood", json!(null), 200);
        push_switch(&db, "mood", json!("sad"), 300); // current = sad
        assert_eq!(
            db.resolve_state_back("mood", 1, probe()).unwrap(),
            Some(json!(null))
        );
        assert_eq!(
            db.resolve_state_back("mood", 2, probe()).unwrap(),
            Some(json!("happy"))
        );
    }

    #[test]
    fn state_back_ignores_point_rows_on_the_same_field() {
        // Regression (Copilot review on the original PR #40): Point rows
        // interleaved on the field — e.g. `occur`-style annotations whose
        // state is just `true` — must affect neither the "current" pick
        // nor the distinct walk, even when the NEWEST row is a Point.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("a"), 100);
        push_switch(&db, "mood", json!("b"), 200);
        push_point(&db, "mood", 250); // noise mid-history
        push_switch(&db, "mood", json!("c"), 300); // current = c
        push_point(&db, "mood", 350); // noise as the newest row
        assert_eq!(
            db.resolve_state_back("mood", 1, probe()).unwrap(),
            Some(json!("b")),
            "current must be c (newest transition), not the Point's true"
        );
        assert_eq!(
            db.resolve_state_back("mood", 2, probe()).unwrap(),
            Some(json!("a"))
        );
        assert_eq!(db.resolve_state_back("mood", 3, probe()).unwrap(), None);
    }

    /// Timeless row (`note`/`-T`): occur_time NULL, slug `field:c<create_time>`.
    fn push_timeless_note(db: &Db, field: &str) {
        let entry = Entry::new(field, Verb::Point, None, None, None, None, "d1");
        assert!(
            entry.occur_time.is_none() && entry.slug.as_str().contains(":c"),
            "timeless row: occur_time NULL + create_time-keyed slug, got {}",
            entry.slug
        );
        db.insert_entry(&entry).unwrap();
    }

    /// da-ylet oracle-equality property: for any field and probe time,
    /// `state_at(f, t)` must equal `batch_state_at([f], t).get(f)` —
    /// including fields whose history contains timeless (occur_time NULL) rows,
    /// fields with ONLY a timeless row, and fields with no rows at all.
    #[test]
    fn state_at_equals_batch_state_at_for_every_field_and_probe() {
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "alpha", json!("a1"), 100);
        push_switch(&db, "alpha", json!("a2"), 300);
        push_timeless_note(&db, "alpha");
        push_timeless_note(&db, "beta"); // only a timeless row
        push_switch(&db, "gamma", json!("g"), 200);
        // "delta" has no rows at all.

        let fields = ["alpha", "beta", "gamma", "delta"];
        for field in fields {
            for offset in [0, 100, 150, 250, 300, 400] {
                let t = at(offset).utc;
                let single = db.state_at(field, t).unwrap();
                let batch = db.batch_state_at(&[field], t).unwrap();
                assert_eq!(
                    single,
                    batch.get(field).cloned(),
                    "oracles diverge for field={field} at offset={offset}"
                );
            }
        }
        // Multi-field batch agrees with per-field state_at too.
        let t = at(400).utc;
        let batch = db.batch_state_at(&fields, t).unwrap();
        for field in fields {
            assert_eq!(db.state_at(field, t).unwrap(), batch.get(field).cloned());
        }
    }

    /// da-ylet axis 1: a timeless (occur_time NULL) row must not participate in
    /// state through ANY door. Before the fix, `state_at`'s dead
    /// `verb != 'add'` filter let the timeless row through (resolving to
    /// `Some(Null)`) while `batch_state_at` excluded it.
    #[test]
    fn timeless_unset_rows_excluded_through_every_door() {
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("happy"), 100);
        push_timeless_note(&db, "mood"); // newest row, timeless
        push_timeless_note(&db, "solo"); // field's ONLY row
        let probe = at(200).utc;

        // state_at / batch_state_at
        assert_eq!(db.state_at("mood", probe).unwrap(), Some(json!("happy")));
        assert_eq!(db.state_at("solo", probe).unwrap(), None);
        let batch = db.batch_state_at(&["mood", "solo"], probe).unwrap();
        assert_eq!(batch.get("mood"), Some(&json!("happy")));
        assert!(!batch.contains_key("solo"));

        // latest_entry_for_now — the unbounded arm is exactly where the
        // predicate does real work (timeless rows have NULL occur_time,
        // so any `occur_time <= ?` bound excludes them implicitly).
        let latest = db.latest_entry_for_now("mood", None).unwrap().unwrap();
        assert_eq!(latest.state, Some(json!("happy")));
        assert!(db.latest_entry_for_now("solo", None).unwrap().is_none());
        let bounded = db
            .latest_entry_for_now("mood", Some(probe))
            .unwrap()
            .unwrap();
        assert_eq!(bounded.state, Some(json!("happy")));

        // get_active_state
        assert_eq!(db.get_active_state("mood", probe).unwrap(), json!("happy"));
        assert_eq!(db.get_active_state("solo", probe).unwrap(), JsonValue::Null);

        // the per-field cache (integrity/render door)
        let rows = |f| crate::state::FieldStateRows::load(&db.conn, f).unwrap();
        assert_eq!(rows("mood").at(probe, None), Some(json!("happy")));
        assert_eq!(rows("solo").at(probe, None), None);
    }

    /// da-u5l1o: the two ways of fetching a field's rows — the windowed
    /// SQL read behind `state_at`, and the per-field cache the render,
    /// integrity and restore probe — both hand them to the one oracle, so
    /// they agree at every edge of every history and a microsecond either
    /// side of it. A future reader of a field's state joins this table.
    #[test]
    fn the_sql_window_and_the_row_cache_agree_everywhere() {
        type Row = (Verb, Option<JsonValue>, i64, Option<i64>, Option<JsonValue>);
        let sw = |s: &str, at: i64| -> Row { (Verb::Switch, Some(json!(s)), at, None, None) };
        let block = |s: JsonValue, end: i64, secs: i64| -> Row {
            (Verb::Block, Some(s), end, Some(secs), None)
        };
        let compound = |s: &str, end: i64, secs: i64, f: &str| -> Row {
            let verb = if secs == 0 {
                Verb::PointSwitch
            } else {
                Verb::BlockSwitch
            };
            (verb, Some(json!(s)), end, Some(secs), Some(json!(f)))
        };
        let histories: Vec<(&str, Vec<Row>)> = vec![
            ("switch", vec![sw("a", 100), sw("b", 200), sw("a", 300)]),
            // vividn's: a timed switch with no following_state, then its state again.
            (
                "timed",
                vec![sw("s1", 100), block(json!("s2"), 400, 100), sw("s2", 460)],
            ),
            (
                "timed-following",
                vec![sw("s1", 100), compound("sit", 400, 100, "vertical")],
            ),
            (
                "nested",
                vec![
                    sw("s1", 100),
                    block(json!("out"), 600, 400),
                    block(json!("in"), 400, 100),
                ],
            ),
            (
                "back-to-back",
                vec![
                    sw("s1", 100),
                    block(json!("x"), 300, 100),
                    block(json!("y"), 400, 100),
                ],
            ),
            (
                "point-switch",
                vec![sw("s1", 100), compound("startled", 200, 0, "calm")],
            ),
            (
                "switch-inside-compound",
                vec![compound("sit", 400, 200, "vertical"), sw("stand", 300)],
            ),
            (
                "start-stop",
                vec![
                    (Verb::Switch, Some(json!(true)), 100, None, None),
                    (Verb::Switch, Some(json!(false)), 200, None, None),
                    block(json!(true), 400, 100),
                ],
            ),
            (
                "occur",
                vec![sw("on", 100), (Verb::Point, None, 200, None, None)],
            ),
            (
                "occur-after-ignored",
                vec![
                    sw("IGNORED", 100),
                    (Verb::Point, None, 200, None, None),
                    sw("x", 300),
                ],
            ),
            (
                "occur-only",
                vec![
                    (Verb::Point, None, 100, None, None),
                    (Verb::Point, None, 200, None, None),
                ],
            ),
            // da-795cy: a block that interrupted nothing leaves no state,
            // which the next occur makes TRACKED.
            (
                "occur-after-a-block",
                vec![
                    block(json!("x"), 200, 100),
                    (Verb::Point, None, 300, None, None),
                ],
            ),
            // ...and past more blocks than a state read looks back through.
            (
                "occur-after-many-blocks",
                (1..=70)
                    .map(|i| block(json!("x"), 100 + i * 10, 5))
                    .chain(std::iter::once((Verb::Point, None, 805, None, None)))
                    .collect(),
            ),
            // da-795cy review: occur dots stored after a window (with the
            // MOMENT every occur carries) are not blocks, so they must not
            // fill the look-ahead and push out the block spanning it.
            (
                "dots-before-a-spanning-block",
                std::iter::once(sw("a", 100))
                    .chain(std::iter::once(block(json!("x"), 600, 450)))
                    .chain((1..=12).map(|i| (Verb::Point, None, 300 + i * 10, Some(0), None)))
                    .collect(),
            ),
            // da-47v7z: as many blocks as a read looks ahead through, under
            // one more that spans them all.
            (
                "blocks-under-a-block",
                (1..=crate::state::SPAN_LOOKAHEAD as i64)
                    .map(|i| block(json!("short"), 100 + i * 10, 4))
                    .chain(std::iter::once(block(json!("long"), 400, 350)))
                    .collect(),
            ),
            // More dots than a state read looks back through: they never
            // decide a state, so they must not push the switch out of reach.
            (
                "many-occurs",
                std::iter::once(sw("drinking", 100))
                    .chain((1..=70).map(|i| (Verb::Point, None, 100 + i * 10, None, None)))
                    .collect(),
            ),
        ];
        let db = Db::open_in_memory().unwrap();
        for (field, rows) in &histories {
            let (mut edges, mut written) = (Vec::new(), Vec::new());
            for (verb, state, end, secs, following) in rows {
                let mut e = Entry::new(
                    *field,
                    *verb,
                    state.clone(),
                    None,
                    Some(at(*end)),
                    secs.map(|s| match (verb, s) {
                        // What `occur` stores: a point is a MOMENT.
                        (Verb::Point, 0) => crate::dur::Dur::moment(),
                        _ => crate::dur::Dur::of(chrono::Duration::seconds(s)),
                    }),
                    "d1",
                );
                e.following_state = following.clone();
                db.insert_entry(&e).unwrap();
                edges.extend([at(*end).utc, at(end - secs.unwrap_or(0)).utc]);
                written.push(e);
            }
            let cache = crate::state::FieldStateRows::load(&db.conn, field).unwrap();
            // da-47v7z: the render and integrity detection load a row's own
            // span and read its two ends there as though it were not written.
            for e in &written {
                let hid = Some(e.hid.as_str());
                let lo = e.leading_edge_utc().unwrap() - chrono::Duration::microseconds(1);
                let hi = e.occur_time.as_ref().unwrap().utc;
                let span =
                    crate::state::FieldStateRows::load_between(&db.conn, field, lo, hi).unwrap();
                for probe in [lo, hi] {
                    assert_eq!(span.at(probe, hid), cache.at(probe, hid), "{field} {probe}");
                }
            }
            for edge in edges {
                for probe in [-1, 0, 1].map(|us| edge + chrono::Duration::microseconds(us)) {
                    let sql = db.state_at(field, probe).unwrap().filter(|v| !v.is_null());
                    assert_eq!(cache.at(probe, None), sql, "{field} at {probe}");
                    // da-795cy: the timelines load only their window, and
                    // read the same changes from it.
                    let window = crate::state::FieldStateRows::load_between(
                        &db.conn,
                        field,
                        probe,
                        probe + chrono::Duration::seconds(150),
                    )
                    .unwrap();
                    let changes = |rows: &crate::state::FieldStateRows| {
                        rows.changes_between(probe, probe + chrono::Duration::seconds(150))
                            .iter()
                            .map(|c| (c.at, c.state.clone(), c.row.id.clone()))
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(changes(&window), changes(&cache), "{field} from {probe}");
                }
            }
        }
        // And the answers are the right ones where the oracles used to split.
        let state = |f: &str, secs: i64| db.state_at(f, at(secs).utc).unwrap();
        assert_eq!(
            state("timed", 450),
            Some(json!("s1")),
            "the block handed back"
        );
        assert_eq!(
            state("switch-inside-compound", 450),
            Some(json!("vertical"))
        );
        assert_eq!(state("nested", 450), Some(json!("out")));
        assert_eq!(
            state("occur", 250),
            Some(json!("on")),
            "an occur leaves a state alone"
        );
        assert_eq!(state("occur-after-ignored", 150), Some(json!("IGNORED")));
        assert_eq!(state("occur-after-ignored", 250), Some(json!("TRACKED")));
        assert_eq!(state("occur-only", 150), Some(json!("TRACKED")));
        assert_eq!(state("many-occurs", 900), Some(json!("drinking")));
    }

    /// da-ylet axis 3: same-instant ties resolve deterministically to the
    /// slug-DESC winner in every reader. Live same-instant rows are
    /// schema-impossible on one device (da-ga8b's partial UNIQUE on
    /// `(field, occur_time)`), so drop the index to simulate rows that
    /// bypassed it (raw inserts, sync merges).
    #[test]
    fn same_instant_tie_resolves_to_slug_desc_winner_in_every_reader() {
        let db = Db::open_in_memory().unwrap();
        db.conn
            .execute("DROP INDEX entries_field_occurtime", [])
            .unwrap();
        let t = at(100);
        // Insert the slug-DESC winner FIRST so a (wrong) create_time
        // tiebreak would pick the other row.
        let mut winner = Entry::new(
            "mood",
            Verb::Switch,
            Some(json!("zzz")),
            None,
            Some(t.clone()),
            None,
            "d1",
        );
        winner.slug = "mood:tie-z".into();
        let mut loser = Entry::new(
            "mood",
            Verb::Switch,
            Some(json!("aaa")),
            None,
            Some(t.clone()),
            None,
            "d2",
        );
        loser.slug = "mood:tie-a".into();
        db.insert_entry(&winner).unwrap();
        db.insert_entry(&loser).unwrap();

        let probe = at(200).utc;
        assert_eq!(
            db.state_at("mood", probe).unwrap(),
            Some(json!("zzz")),
            "state_at must pick the slug-DESC winner"
        );
        assert_eq!(
            db.batch_state_at(&["mood"], probe).unwrap().get("mood"),
            Some(&json!("zzz")),
            "batch_state_at must pick the same winner"
        );
        assert_eq!(
            crate::state::FieldStateRows::load(&db.conn, "mood")
                .unwrap()
                .at(probe, None),
            Some(json!("zzz")),
            "the per-field cache must pick the same winner"
        );
        let latest = db.latest_entry_for_now("mood", None).unwrap().unwrap();
        assert_eq!(
            latest.slug.as_str(),
            "mood:tie-z",
            "latest_entry_for_now must pick the same winner"
        );
    }

    #[test]
    fn state_back_passes_over_moment_states() {
        // da-22750: a moment (`switch x --moment`, Verb::Point with a
        // state) switches nothing, so the timeline never holds it — the
        // walk goes back over the states the field was actually in.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("a"), 100);
        push_moment_switch(&db, "mood", json!("b"), 200);
        push_switch(&db, "mood", json!("c"), 300);
        push_moment_switch(&db, "mood", json!("d"), 400); // current = c
        assert_eq!(
            db.resolve_state_back("mood", 1, probe()).unwrap(),
            Some(json!("a"))
        );
        assert_eq!(db.resolve_state_back("mood", 2, probe()).unwrap(), None);
    }

    // ─── da-2i8g: following_state-aware state_at / batch_state_at ───

    /// Compound switch (`switch f primary <dur> , following`): the verb is
    /// re-derived on insert from the (state, dur, following_state) triple —
    /// positive dur → block-switch, zero dur → point-switch.
    /// da-zwct: `end_offset` is the block's TRAILING edge — the row's
    /// stored `occur_time` — and `dur_secs` the span before it.
    fn push_compound_switch(
        db: &Db,
        field: &str,
        primary: serde_json::Value,
        dur_secs: i64,
        following: serde_json::Value,
        end_offset: i64,
    ) {
        let mut entry = Entry::new(
            field,
            Verb::Switch,
            Some(primary),
            None,
            Some(at(end_offset)),
            Some(crate::dur::Dur::of(chrono::Duration::seconds(dur_secs))),
            "d1",
        );
        entry.following_state = Some(following);
        db.insert_entry(&entry).unwrap();
    }

    #[test]
    fn state_at_block_switch_honours_following_state_past_trailing_edge() {
        // The da-2i8g repro: `datum switch body sit 30 , vertical`.
        let db = Db::open_in_memory().unwrap();
        push_compound_switch(&db, "body", json!("sit"), 1800, json!("vertical"), 1900);

        // Inside the block → primary state.
        assert_eq!(
            db.state_at("body", at(100 + 900).utc).unwrap(),
            Some(json!("sit"))
        );
        // At the trailing edge and beyond → following_state.
        assert_eq!(
            db.state_at("body", at(100 + 1800).utc).unwrap(),
            Some(json!("vertical"))
        );
        assert_eq!(
            db.state_at("body", at(100 + 3600).utc).unwrap(),
            Some(json!("vertical"))
        );
        // The batch oracle applies the same rule.
        let batch = db.batch_state_at(&["body"], at(100 + 3600).utc).unwrap();
        assert_eq!(batch.get("body"), Some(&json!("vertical")));
    }

    #[test]
    fn state_at_point_switch_following_state_wins_at_occur() {
        // Zero-span compound switch: the primary never prevails on a
        // `<= t` probe — the trailing edge IS the occur instant.
        let db = Db::open_in_memory().unwrap();
        push_compound_switch(&db, "mood", json!("startled"), 0, json!("calm"), 100);

        assert_eq!(db.state_at("mood", at(50).utc).unwrap(), None);
        assert_eq!(
            db.state_at("mood", at(100).utc).unwrap(),
            Some(json!("calm"))
        );
    }

    #[test]
    fn state_at_later_switch_supersedes_block_switch_following_state() {
        // A newer plain switch wins outright — following_state only
        // applies when the block-switch row itself is the winner.
        let db = Db::open_in_memory().unwrap();
        push_compound_switch(&db, "body", json!("sit"), 1800, json!("vertical"), 1900);
        push_switch(&db, "body", json!("lying"), 100 + 3600);
        assert_eq!(
            db.state_at("body", at(100 + 7200).utc).unwrap(),
            Some(json!("lying"))
        );
    }

    // ─── da-phoe: a block hands the field back to what preceded it ───

    /// Mirror `datum end work 12m`: a positive `dur` with a state and no
    /// `following_state` re-derives as [`Verb::Block`] on insert.
    /// da-zwct: `end_offset` is where the block ENDS — its stored
    /// `occur_time` — and the `dur_secs` it lasted precede that instant.
    fn push_block(db: &Db, field: &str, state: serde_json::Value, dur_secs: i64, end_offset: i64) {
        let entry = Entry::new(
            field,
            Verb::Block,
            Some(state),
            None,
            Some(at(end_offset)),
            Some(crate::dur::Dur::of(chrono::Duration::seconds(dur_secs))),
            "d1",
        );
        db.insert_entry(&entry).unwrap();
    }

    #[test]
    fn state_at_resumes_the_state_a_block_interrupted() {
        // vividn's ledger in miniature: `switch work datum`, then a 12-min
        // break. Probe the WHOLE span, because the defect was only ever
        // visible in the stretch that went missing — the block's own
        // 12 minutes always read correctly.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "work", json!("datum"), 100);
        push_block(&db, "work", json!(false), 720, 1720);

        let state = |offset: i64| db.state_at("work", at(offset).utc).unwrap();
        assert_eq!(state(50), None, "before anything was logged");
        assert_eq!(state(500), Some(json!("datum")), "before the block");
        assert_eq!(state(1000), Some(json!(false)), "the block's leading edge");
        assert_eq!(state(1700), Some(json!(false)), "inside the block");
        assert_eq!(
            state(1720),
            Some(json!("datum")),
            "AT the trailing edge the field is handed back"
        );
        assert_eq!(
            state(50_000),
            Some(json!("datum")),
            "and stays handed back — this is the stretch that used to \
             read `false` forever, so every minute of it went uncounted"
        );
    }

    #[test]
    fn state_at_walks_back_past_a_run_of_blocks() {
        // Two breaks in a row: the read has to walk past BOTH to reach
        // the switch that still governs.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "work", json!("datum"), 100);
        push_block(&db, "work", json!(false), 720, 1720);
        push_block(&db, "work", json!(false), 600, 3600);
        assert_eq!(
            db.state_at("work", at(3300).utc).unwrap(),
            Some(json!(false)),
            "inside the second block"
        );
        assert_eq!(
            db.state_at("work", at(50_000).utc).unwrap(),
            Some(json!("datum")),
            "past both"
        );
    }

    #[test]
    fn state_at_stops_walking_at_the_first_row_that_governs() {
        // A block hands the field back to the row before it — not to the
        // oldest row it can find. A newer switch after the block wins
        // outright, and the block resumes the state it interrupted, not
        // an earlier one.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "work", json!("email"), 100);
        push_switch(&db, "work", json!("datum"), 500);
        push_block(&db, "work", json!(false), 720, 1720);
        assert_eq!(
            db.state_at("work", at(2000).utc).unwrap(),
            Some(json!("datum")),
            "resumes the state the block interrupted"
        );
        push_switch(&db, "work", json!("lunch"), 3000);
        assert_eq!(
            db.state_at("work", at(50_000).utc).unwrap(),
            Some(json!("lunch")),
            "a later switch supersedes the resumed state"
        );
    }

    #[test]
    fn state_at_block_with_nothing_before_it_resumes_no_state() {
        // There is no state to hand back to, so the honest answer is
        // "tracked, no current state" — `Null`, and NOT absent. Both
        // read as TRACKED now (da-m1l8), but they still say different
        // things: `Null` is a field that HAS a timed row and holds no
        // state; absent is a field with no timed row at all.
        let db = Db::open_in_memory().unwrap();
        push_block(&db, "work", json!(false), 720, 1720);
        assert_eq!(
            db.state_at("work", at(1700).utc).unwrap(),
            Some(json!(false)),
            "inside the block"
        );
        assert_eq!(
            db.state_at("work", at(2000).utc).unwrap(),
            Some(JsonValue::Null),
            "past it: no prior state to resume"
        );
        let fields = db.get_tracked_fields(at(2000).utc).unwrap();
        let work = fields.iter().find(|f| f.field == "work").unwrap();
        assert!(!work.ignored, "a `Null` resume is tracked, not ignored");
    }

    #[test]
    fn state_at_block_switch_switches_where_a_block_hands_back() {
        // The whole point of the `following_state` column: `Some(x)`
        // means "switch to x at the trailing edge", NULL means "resume".
        // Same two rows, one column apart, two different answers.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "work", json!("datum"), 100);
        push_compound_switch(&db, "work", json!(false), 720, json!("email"), 1720);
        assert_eq!(
            db.state_at("work", at(50_000).utc).unwrap(),
            Some(json!("email")),
            "a stated following_state is not a resume"
        );
    }

    #[test]
    fn tracked_fields_ignored_flag_follows_following_state() {
        // `get_tracked_fields` classifies via batch_state_at, so a
        // block-switch into IGNORED takes effect at the trailing edge.
        let db = Db::open_in_memory().unwrap();
        push_compound_switch(
            &db,
            "body",
            json!("sit"),
            1800,
            json!(IGNORED_STATE_VALUE),
            1900,
        );
        let flag_at = |offset: i64| {
            let fields = db.get_tracked_fields(at(offset).utc).unwrap();
            fields
                .into_iter()
                .find(|f| f.field == "body")
                .unwrap()
                .ignored
        };
        assert!(!flag_at(100 + 900), "inside the block: tracked");
        assert!(flag_at(100 + 3600), "past the trailing edge: ignored");
    }

    // ─── da-a0n3: `_` ordinals on the canonical transition timeline ──

    #[test]
    fn state_back_anchors_on_following_state_past_trailing_edge() {
        // `switch body sit 30 , vertical`, probed after the block ends:
        // the timeline reads … lying@50, sit@100, vertical@1900 — the
        // current anchor is `vertical` (matching `now`), so `_` is the
        // block's primary and `__` the state before the block.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "body", json!("lying"), 50);
        push_compound_switch(&db, "body", json!("sit"), 1800, json!("vertical"), 1900);

        let after = at(100 + 3600).utc;
        assert_eq!(
            db.resolve_state_back("body", 1, after).unwrap(),
            Some(json!("sit")),
            "anchor must be the following_state, so _ is the primary"
        );
        assert_eq!(
            db.resolve_state_back("body", 2, after).unwrap(),
            Some(json!("lying"))
        );
        assert_eq!(db.resolve_state_back("body", 3, after).unwrap(), None);
    }

    #[test]
    fn state_back_mid_block_ignores_future_trailing_edge() {
        // Probed while the block is still running, the trailing
        // `following_state` event lies in the future: the anchor is the
        // block's primary and the following_state is NOT yet a candidate.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "body", json!("lying"), 50);
        push_compound_switch(&db, "body", json!("sit"), 1800, json!("vertical"), 1900);

        let mid = at(100 + 900).utc;
        assert_eq!(
            db.resolve_state_back("body", 1, mid).unwrap(),
            Some(json!("lying")),
            "mid-block: current = primary, _ = the pre-block state"
        );
        assert_eq!(
            db.resolve_state_back("body", 2, mid).unwrap(),
            None,
            "the future trailing edge must not surface as a candidate"
        );
    }

    #[test]
    fn state_back_offers_following_state_as_candidate() {
        // A compound switch contributes BOTH its primary and its
        // following_state as distinct ordinal candidates once both have
        // happened.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("a"), 100);
        push_compound_switch(&db, "mood", json!("b"), 100, json!("c"), 300); // block [200,300], trailing c@300
        push_switch(&db, "mood", json!("d"), 400); // current = d
        let t = probe();
        assert_eq!(
            db.resolve_state_back("mood", 1, t).unwrap(),
            Some(json!("c"))
        );
        assert_eq!(
            db.resolve_state_back("mood", 2, t).unwrap(),
            Some(json!("b"))
        );
        assert_eq!(
            db.resolve_state_back("mood", 3, t).unwrap(),
            Some(json!("a"))
        );
        assert_eq!(db.resolve_state_back("mood", 4, t).unwrap(), None);
    }

    #[test]
    fn state_back_point_switch_skips_its_instant_primary() {
        // Zero-span compound switch: the primary and the following state
        // share one instant, and at that instant the field holds the
        // following state (da-u5l1o: the oracle is read at the edge). The
        // primary is held for no time, so it is nowhere `_` can go back to.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("bored"), 50);
        push_compound_switch(&db, "mood", json!("startled"), 0, json!("calm"), 100);
        assert_eq!(
            db.resolve_state_back("mood", 1, probe()).unwrap(),
            Some(json!("bored")),
            "current = calm (the following_state); _ skips the instant primary"
        );
        assert_eq!(db.resolve_state_back("mood", 2, probe()).unwrap(), None);
    }

    #[test]
    fn state_back_array_state_is_one_opaque_entity() {
        // Spec, not deferral: a parallel multi-state participates as ONE
        // value — `switch body _` returns to the whole parallel state
        // without retyping it, never to a single element.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "body", json!(["vertical", "sit"]), 100);
        push_switch(&db, "body", json!("lying"), 200); // current = lying
        assert_eq!(
            db.resolve_state_back("body", 1, probe()).unwrap(),
            Some(json!(["vertical", "sit"]))
        );
        assert_eq!(db.resolve_state_back("body", 2, probe()).unwrap(), None);
    }
}
