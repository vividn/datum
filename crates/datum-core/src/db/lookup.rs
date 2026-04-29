//! Single-entry lookups: `get_entry`, `resolve_prefix`, plus the
//! state-bearing reads that pivot off a single field at a point in time
//! (`get_active_state`, `find_open_block`, `latest_entry_for_now`).

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::entry::Entry;
use crate::error::{DatumError, PrefixCandidate, Result};

use super::row_mapper::row_to_entry;
use super::types::{
    like_prefix_pattern, time_expr_endbound, AMBIGUOUS_PREFIX_CANDIDATE_CAP,
};
use super::Db;

impl Db {
    /// Fetch a live entry by slug or hid. Tombstoned rows are hidden — use
    /// [`Db::list_including_deleted`] or [`Db::get_entry_including_deleted`]
    /// if you need to see them.
    pub fn get_entry(&self, slug_or_hid: &str) -> Result<Entry> {
        self.conn
            .query_row(
                "SELECT * FROM entries
                 WHERE (slug = ?1 OR hid = ?1) AND deleted_at IS NULL
                 LIMIT 1",
                params![slug_or_hid],
                row_to_entry,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DatumError::NotFound(slug_or_hid.to_string())
                }
                other => DatumError::Db(other),
            })
    }

    /// Resolve a prefix string against live entries' `slug` or `hid` columns.
    ///
    /// An exact match always wins over a prefix match, so typing a full
    /// slug still works unambiguously even when a suffix-disambiguated
    /// sibling row exists (e.g. `coffee:<time>` and `coffee:<time>_xxxx`).
    /// SQL's `LIKE '<prefix>%'` is used with an explicit ESCAPE clause;
    /// `%`, `_`, and `\` in the prefix are escaped so they match literally.
    /// Tombstoned rows are excluded.
    ///
    /// # Errors
    /// - [`DatumError::NotFound`] if no live entry starts with `prefix`.
    /// - [`DatumError::AmbiguousPrefix`] with a candidate list (up to 16
    ///   `(slug, hid)` pairs) when more than one live entry matches.
    pub fn resolve_prefix(&self, prefix: &str) -> Result<Entry> {
        // Exact match (slug or hid) takes precedence — shields full-form
        // lookups from being drowned by suffixed siblings.
        let exact = self
            .conn
            .query_row(
                "SELECT * FROM entries
                 WHERE (slug = ?1 OR hid = ?1) AND deleted_at IS NULL
                 LIMIT 1",
                params![prefix],
                row_to_entry,
            )
            .optional()?;
        if let Some(e) = exact {
            return Ok(e);
        }

        let pattern = like_prefix_pattern(prefix);
        let mut stmt = self.conn.prepare(
            "SELECT * FROM entries
             WHERE (slug LIKE ?1 ESCAPE '\\' OR hid LIKE ?1 ESCAPE '\\')
               AND deleted_at IS NULL
             ORDER BY slug",
        )?;
        let mut entries: Vec<Entry> = stmt
            .query_map(params![pattern], row_to_entry)?
            .collect::<std::result::Result<_, _>>()?;

        match entries.len() {
            0 => Err(DatumError::NotFound(prefix.to_string())),
            1 => Ok(entries.pop().unwrap()),
            _ => {
                let candidates = entries
                    .iter()
                    .take(AMBIGUOUS_PREFIX_CANDIDATE_CAP)
                    .map(|e| PrefixCandidate {
                        slug: e.slug.clone(),
                        hid: e.hid.clone(),
                    })
                    .collect();
                Err(DatumError::AmbiguousPrefix {
                    prefix: prefix.to_string(),
                    candidates,
                })
            }
        }
    }

    /// Fetch an entry by slug or hid regardless of tombstone state.
    /// Supports undo workflows that need to reference deleted rows.
    pub fn get_entry_including_deleted(&self, slug_or_hid: &str) -> Result<Entry> {
        self.conn
            .query_row(
                "SELECT * FROM entries WHERE slug = ?1 OR hid = ?1 LIMIT 1",
                params![slug_or_hid],
                row_to_entry,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DatumError::NotFound(slug_or_hid.to_string())
                }
                other => DatumError::Db(other),
            })
    }

    /// Return the active state of `field` at time `at`, or `Value::Null` if
    /// the field has no state-affecting entries at or before `at`.
    ///
    /// Reconstructs the state-change timeline from non-tombstoned entries of
    /// verb switch/start/stop/occur for `field` (see `state::compile_state` for
    /// the event rules) and returns the most recent event's value.
    ///
    /// SQL fetches just the candidate rows (field match, verb ∈ {switch, start,
    /// stop, occur}, and time_start or time_end ≤ at); compilation runs in Rust.
    pub fn get_active_state(
        &self,
        field: &str,
        at: DateTime<Utc>,
    ) -> Result<JsonValue> {
        let at_str = at.to_rfc3339();
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1
               AND deleted_at IS NULL
               AND verb IN ('switch', 'start', 'stop', 'occur', 'block')
               AND (
                    (start_time IS NOT NULL AND start_time <= ?2)
                 OR (end_time   IS NOT NULL AND end_time   <= ?2)
               )
             ORDER BY {} ASC",
            time_expr_endbound(),
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![field, at_str], row_to_entry)?;
        let entries: Vec<Entry> = rows
            .map(|r| r.map_err(DatumError::Db))
            .collect::<Result<Vec<_>>>()?;
        Ok(crate::state::compile_state(&entries, at))
    }

    /// Find the most recent "open block" for `field` at time `at`: the latest
    /// start or switch entry whose state-effect is still active (not closed by
    /// a subsequent stop/occur or overridden by a later switch).
    ///
    /// Returns `None` if the field's active state at `at` is `null` or `false`
    /// (i.e. there is nothing to close).
    pub fn find_open_block(
        &self,
        field: &str,
        at: DateTime<Utc>,
    ) -> Result<Option<Entry>> {
        let active = self.get_active_state(field, at)?;
        if matches!(active, JsonValue::Null | JsonValue::Bool(false)) {
            return Ok(None);
        }
        let at_str = at.to_rfc3339();
        let mut stmt = self.conn.prepare(
            r#"SELECT * FROM entries
               WHERE field = ?1
                 AND deleted_at IS NULL
                 AND verb IN ('start', 'switch')
                 AND start_time IS NOT NULL
                 AND start_time <= ?2
               ORDER BY start_time DESC, create_time DESC
               LIMIT 1"#,
        )?;
        let row = stmt
            .query_row(params![field, at_str], row_to_entry)
            .optional()?;
        Ok(row)
    }

    /// Latest non-Add, non-tombstoned entry for a field, for use in the now
    /// view. `add` is skipped because it represents free-form data with no
    /// state effect; the now view shows the most recent state-bearing entry.
    pub fn latest_entry_for_now(&self, field: &str) -> Result<Option<Entry>> {
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1 AND verb != 'add' AND deleted_at IS NULL
             ORDER BY {} DESC
             LIMIT 1",
            time_expr_endbound(),
        );
        self.conn
            .query_row(&sql, params![field], row_to_entry)
            .optional()
            .map_err(DatumError::Db)
    }
}

/// Fetch an entry by slug or hid regardless of tombstone state. Kept internal
/// — public callers go through [`Db::get_entry`] or
/// [`Db::get_entry_including_deleted`].
pub(crate) fn fetch_entry_any(conn: &Connection, slug_or_hid: &str) -> Result<Entry> {
    conn.query_row(
        "SELECT * FROM entries WHERE slug = ?1 OR hid = ?1 LIMIT 1",
        params![slug_or_hid],
        row_to_entry,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DatumError::NotFound(slug_or_hid.to_string()),
        other => DatumError::Db(other),
    })
}
