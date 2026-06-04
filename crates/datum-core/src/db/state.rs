//! State-bearing queries: `state_at`, `batch_state_at`, the `now`-view
//! projection, and per-state color reads/writes.

use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::action::Action;
use crate::entry::Verb;
use crate::error::{DatumError, Result};
use crate::field_spec::FieldSpec;

use super::types::{time_expr_endbound, TrackedField, IGNORED_STATE_VALUE};
use super::Db;

impl Db {
    /// Load the field's spec or return a fresh default-`Occur` one. Used by
    /// the color setters that may need to mint a spec on first write.
    pub(super) fn load_or_new_spec(&self, field: &str) -> Result<FieldSpec> {
        match self.load_spec(field)? {
            Some(s) => Ok(s),
            None => Ok(FieldSpec::new(field, Action::Occur)),
        }
    }

    /// Return the field's most recent state-bearing value at or before time
    /// `t`. The state-bearing verbs are switch / occur / start / stop —
    /// `add` (notes) does not change state. Tombstoned rows are excluded.
    ///
    /// `Ok(None)` means the field has no qualifying entries at or before
    /// `t`, which callers should treat as IGNORED (the default for unseen
    /// fields). `Ok(Some(JsonValue::String("IGNORED")))` is the explicit
    /// ignore sentinel; any other value (including `Null` from a "track,
    /// no current state" entry) means the field is tracked.
    pub fn state_at(&self, field: &str, t: DateTime<Utc>) -> Result<Option<JsonValue>> {
        let cutoff = t.to_rfc3339();
        let sql = format!(
            "SELECT state FROM entries
             WHERE field = ?1
               AND verb != 'add'
               AND deleted_at IS NULL
               AND {expr} <= ?2
             ORDER BY {expr} DESC
             LIMIT 1",
            expr = time_expr_endbound(),
        );
        let row: Option<Option<String>> = self
            .conn
            .query_row(&sql, params![field, cutoff], |row| {
                row.get::<_, Option<String>>(0)
            })
            .optional()?;
        match row {
            None => Ok(None),
            Some(None) => Ok(Some(JsonValue::Null)),
            Some(Some(s)) => Ok(Some(serde_json::from_str(&s)?)),
        }
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
        let mut out: std::collections::HashMap<String, JsonValue> =
            std::collections::HashMap::with_capacity(fields.len());
        if fields.is_empty() {
            return Ok(out);
        }
        let cutoff = t.to_rfc3339();
        let endbound = time_expr_endbound();
        let placeholders = std::iter::repeat_n("?", fields.len())
            .collect::<Vec<_>>()
            .join(",");
        // v3 (da-5ns): the legacy `add` verb is gone — Action::Add now
        // routes to Verb::Point. The "skip add-only entries when computing
        // tracked-field state" rule survives via the slug `:unset` marker
        // applied to timeless rows (see log_entry's -T path and build_slug
        // in entry.rs).
        let sql = format!(
            "SELECT field, state FROM (
                SELECT field, state,
                  ROW_NUMBER() OVER (
                    PARTITION BY field
                    ORDER BY {endbound} DESC, slug DESC
                  ) AS rn
                FROM entries
                WHERE field IN ({placeholders})
                  AND slug NOT LIKE '%:unset'
                  AND deleted_at IS NULL
                  AND {endbound} <= ?
            ) WHERE rn = 1"
        );
        let mut bindings: Vec<String> = fields.iter().map(|s| s.to_string()).collect();
        bindings.push(cutoff);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), |row| {
            let field: String = row.get(0)?;
            let value: Option<String> = row.get(1)?;
            Ok((field, value))
        })?;
        for r in rows {
            let (field, value) = r.map_err(DatumError::Db)?;
            let json = match value {
                None => JsonValue::Null,
                Some(s) => serde_json::from_str(&s)?,
            };
            out.insert(field, json);
        }
        Ok(out)
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
    /// Only state-transition rows participate. A row is a transition iff
    /// it carries a state (SQL NULL / `state == None` rows are skipped —
    /// they carry no state at all) AND it isn't an occur-shaped
    /// annotation: a `Verb::Point` row whose state is literal `true` is a
    /// point event (`occur` is Moment-locked and always lands as
    /// Point+true), so it's ignored for both the CURRENT pick and the
    /// alternatives walk. A `Verb::Point` row with any OTHER state is a
    /// genuine zero-length transition — `switch x --moment` derives
    /// `Point` while keeping its real state — and participates fully.
    /// An explicit JSON `null` state (`Some(JsonValue::Null)`) is a
    /// real, storable state value and participates like any other.
    ///
    /// `following_state` is NOT consulted — consistent with [`Db::state_at`]
    /// / `now`, which don't read it yet either (state.rs sub-bead 5).
    pub fn resolve_state_back(&self, field: &str, n: usize) -> Result<Option<JsonValue>> {
        if n == 0 {
            return Ok(None);
        }
        // 256 entries gives plenty of headroom for a heavily-toggled field;
        // the SQL is field-scoped and indexed, so the cost is negligible.
        let entries = self.entries_for_field(field, Some(256))?;
        let mut transitions = entries.iter().filter(|e| {
            let Some(state) = e.state.as_ref() else {
                return false;
            };
            // Point+true ⇒ occur-shaped annotation noise. Point with any
            // other state ⇒ a zero-length (`--moment`) switch — a genuine
            // transition that must participate (review finding on #43).
            !(matches!(e.verb, Verb::Point) && state == &JsonValue::Bool(true))
        });
        let Some(current_entry) = transitions.next() else {
            return Ok(None);
        };
        let current = current_entry.state.as_ref();
        let mut alternatives: Vec<JsonValue> = Vec::new();
        for e in transitions {
            let s = e
                .state
                .as_ref()
                .expect("filter keeps only state-bearing rows");
            if Some(s) == current || alternatives.iter().any(|x| x == s) {
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
    /// view. A field appears here iff it has at least one live entry; its
    /// `ignored` flag derives from [`Db::state_at`] at `at` — pass
    /// `Utc::now()` for the live view, or a time-travel anchor (da-n0dr)
    /// so scope membership matches the per-field states at that moment.
    /// Per-field color is read from the matching [`FieldSpec`] when one
    /// exists. Ordered alphabetically by field name.
    pub fn get_tracked_fields(&self, at: DateTime<Utc>) -> Result<Vec<TrackedField>> {
        let fields = self.list_distinct_fields()?;
        let field_refs: Vec<&str> = fields.iter().map(String::as_str).collect();
        let states = self.batch_state_at(&field_refs, at)?;
        let specs = self.batch_load_specs(&field_refs)?;
        let mut out = Vec::with_capacity(fields.len());
        for field in fields {
            let ignored = match states.get(&field) {
                None => true,
                Some(JsonValue::String(s)) if s == IGNORED_STATE_VALUE => true,
                _ => false,
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
        let mut stmt = self.conn.prepare("SELECT spec_json FROM field_specs")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut map = std::collections::HashMap::new();
        for r in rows {
            let spec: FieldSpec = serde_json::from_str(&r.map_err(DatumError::Db)?)?;
            for (state, color) in spec.state_colors {
                map.insert((spec.field.clone(), state), color);
            }
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;
    use crate::time::DatumTime;
    use chrono::TimeZone;
    use serde_json::json;

    fn at(secs_offset: i64) -> DatumTime {
        let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        DatumTime {
            utc: base + chrono::Duration::seconds(secs_offset),
            offset_minutes: 0,
            timezone: "UTC".into(),
        }
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
            Some(crate::dur::Dur::Moment),
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
            Some(crate::dur::Dur::Moment),
            "d1",
        );
        db.insert_entry(&entry).unwrap();
    }

    #[test]
    fn state_back_returns_none_when_history_empty() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.resolve_state_back("mood", 1).unwrap(), None);
    }

    #[test]
    fn state_back_returns_none_for_zero_ordinal() {
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("happy"), 100);
        assert_eq!(db.resolve_state_back("mood", 0).unwrap(), None);
    }

    #[test]
    fn state_back_walks_distinct_alternatives() {
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("a"), 100);
        push_switch(&db, "mood", json!("b"), 200);
        push_switch(&db, "mood", json!("c"), 300);
        push_switch(&db, "mood", json!("b"), 400); // current = b
        assert_eq!(
            db.resolve_state_back("mood", 1).unwrap(),
            Some(json!("c")),
            "_ returns the most recent distinct non-current state"
        );
        assert_eq!(
            db.resolve_state_back("mood", 2).unwrap(),
            Some(json!("a")),
            "__ returns the second-most recent distinct non-current state"
        );
        assert_eq!(
            db.resolve_state_back("mood", 3).unwrap(),
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
        assert_eq!(db.resolve_state_back("mood", 1).unwrap(), Some(json!("b")));
        assert_eq!(db.resolve_state_back("mood", 2).unwrap(), None);
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
        assert_eq!(db.resolve_state_back("mood", 1).unwrap(), Some(json!(null)));
        assert_eq!(
            db.resolve_state_back("mood", 2).unwrap(),
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
            db.resolve_state_back("mood", 1).unwrap(),
            Some(json!("b")),
            "current must be c (newest transition), not the Point's true"
        );
        assert_eq!(db.resolve_state_back("mood", 2).unwrap(), Some(json!("a")));
        assert_eq!(db.resolve_state_back("mood", 3).unwrap(), None);
    }

    #[test]
    fn state_back_counts_moment_switches_as_transitions() {
        // Review finding on #43: `switch x --moment` derives Verb::Point
        // while keeping its real state — a genuine zero-length transition
        // that must participate in both the current pick and the walk.
        let db = Db::open_in_memory().unwrap();
        push_switch(&db, "mood", json!("a"), 100);
        push_moment_switch(&db, "mood", json!("b"), 200);
        push_switch(&db, "mood", json!("c"), 300);
        push_moment_switch(&db, "mood", json!("d"), 400); // current = d
        assert_eq!(
            db.resolve_state_back("mood", 1).unwrap(),
            Some(json!("c")),
            "a moment-switch as the newest row IS the current state"
        );
        assert_eq!(db.resolve_state_back("mood", 2).unwrap(), Some(json!("b")));
        assert_eq!(db.resolve_state_back("mood", 3).unwrap(), Some(json!("a")));
    }
}
