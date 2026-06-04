//! State-bearing queries: `state_at`, `batch_state_at`, the `now`-view
//! projection, and per-state color reads/writes.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

use crate::action::Action;
use crate::error::{DatumError, Result};
use crate::field_spec::FieldSpec;

use super::types::{
    time_expr_endbound, TrackedField, IGNORED_STATE_VALUE, STATE_ORDER_NEWEST_FIRST,
    STATE_PARTICIPATION_SQL,
};
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
    /// `t`. Row participation and ordering follow the canonical rule
    /// ([`super::types::STATE_PARTICIPATION_SQL`]): live, non-`:unset`
    /// rows, newest `occur_time` first with a slug tiebreak. Delegates to
    /// [`Db::batch_state_at`] so the two oracles cannot diverge (da-ylet).
    ///
    /// da-2i8g: when the winning row is a block-switch / point-switch
    /// whose trailing edge (`occur_time + dur`) is at or before `t`, the
    /// row's `following_state` is the effective value — same rule as
    /// `now` ([`super::now::effective_state`]).
    ///
    /// `Ok(None)` means the field has no qualifying entries at or before
    /// `t`, which callers should treat as IGNORED (the default for unseen
    /// fields). `Ok(Some(JsonValue::String("IGNORED")))` is the explicit
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
        // da-2i8g: the winning row's verb / occur_time / dur /
        // following_state come along so the trailing-edge rule
        // ([`super::now::effective_state`]) can swap in the
        // `following_state` of a block-switch / point-switch whose
        // trailing edge is at or before `t`.
        let sql = format!(
            "SELECT field, state, verb, occur_time, dur, following_state FROM (
                SELECT field, state, verb, occur_time, dur, following_state,
                  ROW_NUMBER() OVER (
                    PARTITION BY field
                    ORDER BY {STATE_ORDER_NEWEST_FIRST}
                  ) AS rn
                FROM entries
                WHERE field IN ({placeholders})
                  AND {STATE_PARTICIPATION_SQL}
                  AND {endbound} <= ?
            ) WHERE rn = 1"
        );
        let mut bindings: Vec<String> = fields.iter().map(|s| s.to_string()).collect();
        bindings.push(cutoff);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), |row| {
            let field: String = row.get(0)?;
            let value: Option<String> = row.get(1)?;
            let verb: crate::entry::Verb =
                row.get::<_, String>(2)?.parse().map_err(|e: String| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        e.into(),
                    )
                })?;
            let occur_utc: Option<DateTime<Utc>> = row
                .get::<_, Option<String>>(3)?
                .and_then(|s| s.parse().ok());
            let dur: Option<crate::dur::Dur> = row
                .get::<_, Option<String>>(4)?
                .as_deref()
                .and_then(crate::dur::Dur::parse);
            let following: Option<String> = row.get(5)?;
            Ok((field, value, verb, occur_utc, dur, following))
        })?;
        for r in rows {
            let (field, value, verb, occur_utc, dur, following) = r.map_err(DatumError::Db)?;
            let state: Option<JsonValue> = match value {
                None => None,
                Some(s) => Some(serde_json::from_str(&s)?),
            };
            let fs: Option<JsonValue> = match following {
                None => None,
                Some(s) => Some(serde_json::from_str(&s)?),
            };
            let json = super::now::effective_state(
                verb,
                state.as_ref(),
                occur_utc,
                dur.as_ref(),
                fs.as_ref(),
                t,
            );
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
    /// `following_state` is NOT consulted: the walk reads only primary
    /// states. ([`Db::state_at`] / `now` DO honour it for the *current*
    /// state since da-2i8g, but how compound switches should participate
    /// in the distinct-alternatives walk is a separate design question.)
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
            // The verb half is exactly the complement of shape.rs's badge
            // set (`Point` is the lone badge-less verb).
            crate::shape::has_state_badge(e.verb) || state != &JsonValue::Bool(true)
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
    use crate::entry::{Entry, Verb};
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

    /// Timeless row (`-T`): occur_time NULL, slug `field:unset`.
    fn push_timeless_note(db: &Db, field: &str) {
        let entry = Entry::new(field, Verb::Point, None, None, None, None, "d1");
        assert!(entry.slug.as_str().ends_with(":unset"));
        db.insert_entry(&entry).unwrap();
    }

    /// da-ylet oracle-equality property: for any field and probe time,
    /// `state_at(f, t)` must equal `batch_state_at([f], t).get(f)` —
    /// including fields whose history contains timeless `:unset` rows,
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

    /// da-ylet axis 1: a timeless `:unset` row must not participate in
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

        // get_active_state (compile-based door)
        assert_eq!(db.get_active_state("mood", probe).unwrap(), json!("happy"));
        assert_eq!(db.get_active_state("solo", probe).unwrap(), JsonValue::Null);

        // last_state_at (integrity/render door)
        assert_eq!(
            crate::state::last_state_at(&db.conn, "mood", probe, None).unwrap(),
            Some(json!("happy"))
        );
        assert_eq!(
            crate::state::last_state_at(&db.conn, "solo", probe, None).unwrap(),
            None
        );
    }

    /// da-ylet axis 3: same-instant ties resolve deterministically to the
    /// slug-DESC winner in BOTH oracles. Live same-instant rows are
    /// schema-impossible on one device (da-ga8b's partial UNIQUE on
    /// `(field, occur_time)`), so drop the index to simulate rows that
    /// bypassed it (raw inserts, sync merges).
    #[test]
    fn same_instant_tie_resolves_to_slug_desc_winner_in_both_oracles() {
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
        let latest = db.latest_entry_for_now("mood", None).unwrap().unwrap();
        assert_eq!(
            latest.slug.as_str(),
            "mood:tie-z",
            "latest_entry_for_now must pick the same winner"
        );
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

    // ─── da-2i8g: following_state-aware state_at / batch_state_at ───

    /// Compound switch (`switch f primary <dur> , following`): the verb is
    /// re-derived on insert from the (state, dur, following_state) triple —
    /// positive dur → block-switch, zero dur → point-switch.
    fn push_compound_switch(
        db: &Db,
        field: &str,
        primary: serde_json::Value,
        dur_secs: i64,
        following: serde_json::Value,
        secs_offset: i64,
    ) {
        let mut entry = Entry::new(
            field,
            Verb::Switch,
            Some(primary),
            None,
            Some(at(secs_offset)),
            Some(crate::dur::Dur::Of(chrono::Duration::seconds(dur_secs))),
            "d1",
        );
        entry.following_state = Some(following);
        db.insert_entry(&entry).unwrap();
    }

    #[test]
    fn state_at_block_switch_honours_following_state_past_trailing_edge() {
        // The da-2i8g repro: `datum switch body sit 30 , vertical`.
        let db = Db::open_in_memory().unwrap();
        push_compound_switch(&db, "body", json!("sit"), 1800, json!("vertical"), 100);

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
        push_compound_switch(&db, "body", json!("sit"), 1800, json!("vertical"), 100);
        push_switch(&db, "body", json!("lying"), 100 + 3600);
        assert_eq!(
            db.state_at("body", at(100 + 7200).utc).unwrap(),
            Some(json!("lying"))
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
            100,
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
}
