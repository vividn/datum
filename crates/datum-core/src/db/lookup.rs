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
    like_prefix_pattern, time_expr_endbound, AMBIGUOUS_PREFIX_CANDIDATE_CAP, LIVE_SQL,
    STATE_ORDER_NEWEST_FIRST, STATE_ORDER_OLDEST_FIRST, STATE_PARTICIPATION_SQL,
};

/// Address one row by slug or hid. A superseded row (da-uxgv) has stepped
/// out of the live stream but still exists and is still *addressable* — by
/// its hid, the identity that outlives every edit. Its slug is shared with
/// the row that won, and there the winner is what the user means.
const BY_SLUG_OR_HID_SQL: &str =
    "(slug = ?1 OR hid = ?1) AND deleted_at IS NULL AND (superseded_by IS NULL OR hid = ?1)";
use super::Db;

impl Db {
    /// Read the `extra_metadata` JSON for a row, addressed by slug or hid.
    /// Returns `None` for live rows that carry no provenance and for rows
    /// that don't exist (callers wanting NotFound should compose with
    /// [`Db::get_entry`]). System provenance — never user-visible payload.
    pub fn get_metadata(&self, slug_or_hid: &str) -> Result<Option<JsonValue>> {
        let raw: Option<Option<String>> = self
            .conn
            .query_row(
                &format!("SELECT extra_metadata FROM entries WHERE {BY_SLUG_OR_HID_SQL} LIMIT 1"),
                params![slug_or_hid],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        Ok(raw.flatten().and_then(|s| serde_json::from_str(&s).ok()))
    }

    /// Fetch a live entry by slug or hid. Tombstoned rows are hidden — use
    /// [`Db::list_including_deleted`] or [`Db::get_entry_including_deleted`]
    /// if you need to see them.
    pub fn get_entry(&self, slug_or_hid: &str) -> Result<Entry> {
        self.conn
            .query_row(
                &format!("SELECT * FROM entries WHERE {BY_SLUG_OR_HID_SQL} LIMIT 1"),
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
        let mut entries = self.resolve_prefix_all(prefix)?;
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

    /// Like [`Db::resolve_prefix`], but returns *every* live entry whose
    /// slug or hid starts with `prefix` rather than erroring on ambiguity.
    /// An exact slug/hid match still short-circuits to that single entry
    /// (shielding full-form lookups from suffixed siblings); zero matches
    /// returns an empty vec. The return-all `get` path uses this to render
    /// all matches instead of asking the user to disambiguate.
    pub fn resolve_prefix_all(&self, prefix: &str) -> Result<Vec<Entry>> {
        // Exact match (slug or hid) takes precedence — shields full-form
        // lookups from being drowned by suffixed siblings.
        let exact = self
            .conn
            .query_row(
                &format!("SELECT * FROM entries WHERE {BY_SLUG_OR_HID_SQL} LIMIT 1"),
                params![prefix],
                row_to_entry,
            )
            .optional()?;
        if let Some(e) = exact {
            return Ok(vec![e]);
        }

        let pattern = like_prefix_pattern(prefix);
        let mut stmt = self.conn.prepare(
            "SELECT * FROM entries
             WHERE (slug LIKE ?1 ESCAPE '\\' OR hid LIKE ?1 ESCAPE '\\')
               AND deleted_at IS NULL
               AND (superseded_by IS NULL OR hid LIKE ?1 ESCAPE '\\')
             ORDER BY slug",
        )?;
        let entries: Vec<Entry> = stmt
            .query_map(params![pattern], row_to_entry)?
            .collect::<std::result::Result<_, _>>()?;
        Ok(entries)
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
    /// the field holds none there. [`Db::state_at`] with the absence
    /// folded into `Null` — the one oracle, not a second one (da-u5l1o).
    pub fn get_active_state(&self, field: &str, at: DateTime<Utc>) -> Result<JsonValue> {
        Ok(self.state_at(field, at)?.unwrap_or(JsonValue::Null))
    }

    /// Find the most recent "open block" for `field` at time `at`: the latest
    /// start or switch entry whose state-effect is still active (not closed by
    /// a subsequent stop/occur or overridden by a later switch).
    ///
    /// Returns `None` if the field's active state at `at` is `null` or off
    /// (`false` / TRACKED — nothing to close).
    pub fn find_open_block(&self, field: &str, at: DateTime<Utc>) -> Result<Option<Entry>> {
        let active = self.get_active_state(field, at)?;
        if active.is_null() || crate::shape::is_off_state(&active) {
            return Ok(None);
        }
        let at_str = at.to_rfc3339();
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1
               AND {LIVE_SQL}
               AND verb IN ('start', 'switch', 'block', 'block-switch')
               AND occur_time <= ?2
             ORDER BY {STATE_ORDER_NEWEST_FIRST}
             LIMIT 1"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let row = stmt
            .query_row(params![field, at_str], row_to_entry)
            .optional()?;
        Ok(row)
    }

    /// Find the most recent live entry for `field` whose stored `state`
    /// matches `state_json`. Powers the `-u`/`--undo` log flag: the user
    /// re-runs a command with `-u` appended and expects the previous
    /// matching insert to be reversed.
    ///
    /// Match semantics: `None` matches NULL state; `Some(v)` matches by
    /// exact JSON serialization (so `2` and `2.0` are distinct, just as
    /// they are at insert). Ordering: `occur_time` DESC, `create_time`
    /// DESC — newest match wins.
    ///
    /// Deliberate OPT-OUT from the canonical state ordering
    /// ([`super::types::STATE_ORDER_NEWEST_FIRST`], da-ylet): undo targets
    /// the literal previous *insert*, not the state-timeline winner, so
    /// same-instant ties break on `create_time` (latest write) rather
    /// than slug. Timeless rows (occur_time NULL) are also fair game — `-u` must
    /// be able to reverse a `-T` note.
    pub fn find_latest_for_undo(
        &self,
        field: &str,
        state_json: Option<&JsonValue>,
    ) -> Result<Option<Entry>> {
        let state_text = state_json.map(|s| s.to_string());
        let mut stmt = self.conn.prepare(&format!(
            r#"SELECT * FROM entries
               WHERE field = ?1
                 AND {LIVE_SQL}
                 AND ((?2 IS NULL AND state IS NULL) OR state = ?2)
               ORDER BY occur_time DESC, create_time DESC
               LIMIT 1"#
        ))?;
        let row = stmt
            .query_row(params![field, state_text], row_to_entry)
            .optional()?;
        Ok(row)
    }

    /// Latest state-participating entry for a field, for use in the now
    /// view. Participation and ordering follow the canonical rule
    /// ([`STATE_PARTICIPATION_SQL`]): timeless rows (occur_time NULL, free-form
    /// `-T` notes with no state effect) are skipped, and the unbounded
    /// arm is exactly where that predicate matters — timeless rows store
    /// NULL `occur_time`, so the bounded arm excludes them implicitly.
    ///
    /// `snapshot_time = Some(t)` (da-tx8j) bounds the query to entries
    /// whose effective time is `<= t` — used by `datum now -t` to render
    /// the state-at-a-past-point view. `None` keeps the legacy "latest
    /// of all time" behaviour.
    pub fn latest_entry_for_now(
        &self,
        field: &str,
        snapshot_time: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Option<Entry>> {
        Ok(self
            .entries_for_now(field, snapshot_time, 1, false)?
            .into_iter()
            .next())
    }

    /// The oldest occur dot ([`crate::state::StateRow::is_occur_dot`]) of
    /// `field` stored after `after` and at or before `at` — the one that
    /// turns an IGNORED field TRACKED. The state reads leave dots out and
    /// ask for this one on its own.
    pub(crate) fn first_dot_after(
        &self,
        field: &str,
        after: Option<chrono::DateTime<chrono::Utc>>,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<Entry>> {
        let after = after.map(|t| t.to_rfc3339()).unwrap_or_default();
        self.conn
            .query_row(
                &first_dot_sql("*"),
                params![field, after, at.to_rfc3339()],
                row_to_entry,
            )
            .optional()
            .map_err(DatumError::Db)
    }

    /// The newest `limit` state-participating entries for a field, newest
    /// first — [`Db::latest_entry_for_now`] with room to look further
    /// back. `now` reads one row in the common case and asks for more
    /// only when that row is a block that has already handed the field
    /// back and the resumed state lies behind it (da-phoe). `skip_dots`
    /// leaves occur dots out, as the depth-limited state reads do.
    pub fn entries_for_now(
        &self,
        field: &str,
        snapshot_time: Option<chrono::DateTime<chrono::Utc>>,
        limit: usize,
        skip_dots: bool,
    ) -> Result<Vec<Entry>> {
        let expr = time_expr_endbound();
        let (bound, cutoff) = match snapshot_time {
            None => (String::new(), None),
            Some(t) => (format!("AND {expr} <= ?2"), Some(t.to_rfc3339())),
        };
        let dots = if skip_dots {
            format!("AND NOT {}", crate::state::OCCUR_DOT_SQL)
        } else {
            String::new()
        };
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1 AND {STATE_PARTICIPATION_SQL}
               {bound} {dots}
             ORDER BY {STATE_ORDER_NEWEST_FIRST}
             LIMIT {limit}",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = match &cutoff {
            None => stmt.query_map(params![field], row_to_entry)?,
            Some(c) => stmt.query_map(params![field, c], row_to_entry)?,
        };
        let mut entries: Vec<Entry> = rows
            .map(|r| r.map_err(DatumError::Db))
            .collect::<Result<Vec<_>>>()?;
        if let Some(t) = snapshot_time {
            entries.extend(self.blocks_spanning(field, t)?);
        }
        Ok(entries)
    }

    /// Dur-bearing rows stored AFTER `t` whose span still covers it.
    /// da-zwct: a block's `occur_time` is its END, so the row that holds
    /// the field at `t` can sit in `t`'s future — the bounded readers
    /// would miss it entirely. Only the nearest
    /// [`crate::state::SPAN_LOOKAHEAD`] forward blocks are considered; the
    /// leading edge is re-derived in Rust because `dur` is an ISO-8601
    /// string SQL cannot subtract from a time.
    fn blocks_spanning(&self, field: &str, t: chrono::DateTime<chrono::Utc>) -> Result<Vec<Entry>> {
        let expr = time_expr_endbound();
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1 AND {STATE_PARTICIPATION_SQL}
               AND dur IS NOT NULL AND {expr} > ?2
             ORDER BY {expr} ASC
             LIMIT {}",
            crate::state::SPAN_LOOKAHEAD,
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![field, t.to_rfc3339()], row_to_entry)?;
        let mut out = Vec::new();
        for r in rows {
            let e = r.map_err(DatumError::Db)?;
            if e.leading_edge_utc().is_some_and(|edge| edge <= t) {
                out.push(e);
            }
        }
        Ok(out)
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

/// The oldest occur dot of `?1` stored after `?2` and at or before `?3`,
/// selecting `columns` ([`Db::first_dot_after`]).
pub(super) fn first_dot_sql(columns: &str) -> String {
    format!(
        "SELECT {columns} FROM entries
         WHERE field = ?1 AND {STATE_PARTICIPATION_SQL}
           AND {dot}
           AND occur_time > ?2 AND occur_time <= ?3
         ORDER BY {STATE_ORDER_OLDEST_FIRST}
         LIMIT 1",
        dot = crate::state::OCCUR_DOT_SQL,
    )
}
