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
    pub fn state_at(
        &self,
        field: &str,
        t: DateTime<Utc>,
    ) -> Result<Option<JsonValue>> {
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
            .query_row(
                &sql,
                params![field, cutoff],
                |row| row.get::<_, Option<String>>(0),
            )
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
        let sql = format!(
            "SELECT field, state FROM (
                SELECT field, state,
                  ROW_NUMBER() OVER (
                    PARTITION BY field
                    ORDER BY {endbound} DESC, slug DESC
                  ) AS rn
                FROM entries
                WHERE field IN ({placeholders})
                  AND verb != 'add'
                  AND deleted_at IS NULL
                  AND {endbound} <= ?
            ) WHERE rn = 1"
        );
        let mut bindings: Vec<String> =
            fields.iter().map(|s| s.to_string()).collect();
        bindings.push(cutoff);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(bindings.iter()),
            |row| {
                let field: String = row.get(0)?;
                let value: Option<String> = row.get(1)?;
                Ok((field, value))
            },
        )?;
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

    /// Project the entry log into a [`TrackedField`] list for the `now`
    /// view. A field appears here iff it has at least one live entry; its
    /// `ignored` flag derives from [`Db::state_at`] at `now`. Per-field
    /// color is read from the matching [`FieldSpec`] when one exists.
    /// Ordered alphabetically by field name.
    pub fn get_tracked_fields(&self) -> Result<Vec<TrackedField>> {
        let now = Utc::now();
        let fields = self.list_distinct_fields()?;
        let field_refs: Vec<&str> = fields.iter().map(String::as_str).collect();
        let states = self.batch_state_at(&field_refs, now)?;
        let specs = self.batch_load_specs(&field_refs)?;
        let mut out = Vec::with_capacity(fields.len());
        for field in fields {
            let ignored = match states.get(&field) {
                None => true,
                Some(JsonValue::String(s)) if s == IGNORED_STATE_VALUE => true,
                _ => false,
            };
            let color = specs.get(&field).and_then(|s| s.color.clone());
            out.push(TrackedField { field, ignored, color });
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
