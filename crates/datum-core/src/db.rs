//! SQLite database layer for datum.
//!
//! All entry data, field specs, and aliases live in a single SQLite file.
//! The schema is designed to be CRDT-friendly (append-mostly, ULID-ordered).
//!
//! FieldSpec is the source of truth for a field's tracking state, color,
//! allowed states, and derived aliases. The `tracked_fields` and
//! `field_state_colors` tables have been absorbed into the FieldSpec JSON;
//! helper methods here project that shape back out for callers that still
//! work in terms of "tracked fields" and "state colors".

use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::entry::{new_hid, Entry, Verb};
use crate::error::{DatumError, PrefixCandidate, Result};
use crate::field_spec::{Alias, FieldSpec, SpecRegistry};
use crate::time::DatumTime;
use chrono::{DateTime, Duration, Utc};

/// Projection of a FieldSpec for the `now` view. Built from FieldSpec on read.
pub struct TrackedField {
    pub field: String,
    pub ignored: bool,
    pub color: Option<String>,
}

/// Which timestamp to sort/filter entries by in [`Db::list_entries`].
///
/// `Hybrid` falls back to `modified_at` for entries without a `time_start`
/// (e.g. notes). `Occur` excludes rows lacking `time_start` entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeMetric {
    #[default]
    Hybrid,
    Occur,
    Create,
    Modify,
}

impl std::str::FromStr for TimeMetric {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hybrid" => Ok(TimeMetric::Hybrid),
            "occur" => Ok(TimeMetric::Occur),
            "create" => Ok(TimeMetric::Create),
            "modify" => Ok(TimeMetric::Modify),
            other => Err(format!("unknown time metric: {other}")),
        }
    }
}

/// Parameters for [`Db::list_entries`]: the unified entry query used by
/// `tail` / `head`. Results are always returned oldest-first (unless
/// `reverse` is set), so callers can treat the output as a time-ordered slice.
#[derive(Debug, Clone, Default)]
pub struct ListParams {
    /// Restrict to entries with `field == <name>` when Some.
    pub field: Option<String>,
    /// Which timestamp governs ordering and the bound checks.
    pub metric: TimeMetric,
    /// Inclusive lower bound on the metric timestamp.
    pub start_utc: Option<DateTime<Utc>>,
    /// Inclusive upper bound on the metric timestamp.
    pub end_utc: Option<DateTime<Utc>>,
    /// `None` = unlimited. For tail/head, bounds the returned count.
    pub limit: Option<usize>,
    /// If true, take the *first* `limit` entries in the window (head semantics).
    /// If false (default), take the *last* `limit` entries (tail semantics).
    pub head: bool,
    /// Invert the final output ordering (newest-first).
    pub reverse: bool,
}

/// Selection filter for [`Db::update_times`]. Every non-None field narrows
/// the match. `slug_or_hid` matches either column. `after_utc` / `before_utc`
/// bound `time_start_utc` (inclusive); rows with null `time_start_utc` are
/// excluded whenever a time bound is set.
#[derive(Debug, Clone, Default)]
pub struct RetimeFilter<'a> {
    pub slug_or_hid: Option<&'a str>,
    pub field: Option<&'a str>,
    pub after_utc: Option<DateTime<Utc>>,
    pub before_utc: Option<DateTime<Utc>>,
}

/// Row reported by [`Db::find_time_end_before_start`].
#[derive(Debug, Clone)]
pub struct TimeOrderIssue {
    pub slug: String,
    pub field: String,
    pub time_start_utc: String,
    pub time_end_utc: String,
}

/// Strategy for handling content_hash collisions on insert.
///
/// Rows are mutable: `Update` / `Merge` / `Rekey` all bump `modified_at`.
/// CRDT/sync reconciliation is deferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictStrategy {
    /// Silently skip the new row and return [`InsertOutcome::Ignored`] with
    /// the existing id. Current default.
    #[default]
    Ignore,
    /// Overwrite the existing row's mutable fields with the incoming entry's
    /// values (bumps `modified_at`).
    Update,
    /// Deep-merge incoming `extra_data` into the existing row's `extra_data`
    /// and otherwise behave like [`ConflictStrategy::Update`].
    Merge,
    /// Insert the incoming row anyway with its new ULID, leaving the existing
    /// row untouched. Both rows end up with the same `content_hash`.
    Rekey,
}

/// Outcome reported by [`Db::insert_entry_with_strategy`].
///
/// All variants carry the stored slug — the primary key. On a successful
/// [`InsertOutcome::Inserted`] the slug may differ from `entry.slug` if a
/// collision forced a suffixed retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    /// New row inserted (no collision).
    Inserted { slug: String },
    /// Collision resolved by mutating the existing row in place.
    Updated { slug: String },
    /// Collision resolved by merging `extra_data` into the existing row.
    Merged { slug: String },
    /// Collision resolved by inserting a new row under a new slug.
    Rekeyed { old_slug: String, new_slug: String },
    /// Collision left the existing row untouched.
    Ignored { slug: String },
}

/// Max number of candidates returned in [`DatumError::AmbiguousPrefix`].
/// A wide-enough error list to disambiguate in practice, but bounded so a
/// pathological prefix (e.g. empty) can't dump the whole table.
const AMBIGUOUS_PREFIX_CANDIDATE_CAP: usize = 16;

/// Build a SQL LIKE pattern that matches anything starting with `prefix`
/// literally — `%`, `_`, and `\` in the user-supplied prefix are escaped so
/// they are treated as plain characters. Pair with `ESCAPE '\\'` in the query.
fn like_prefix_pattern(prefix: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + 1);
    for ch in prefix.chars() {
        if ch == '\\' || ch == '%' || ch == '_' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (or create) the database at the given path.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        // Pre-a0 aliases table only carried (name, alias_json); add the
        // derived_from_field column before schema.sql tries to index it.
        // Missing-table errors on a fresh DB are ignored.
        let _ = self.conn.execute(
            "ALTER TABLE aliases ADD COLUMN derived_from_field TEXT",
            [],
        );

        // Reject pre-refactor entries tables (which have an `id` column):
        // this refactor drops ULID entirely and slug is now PK. Silent
        // migration would lose or re-key data unpredictably; the user must
        // delete the DB and re-create it.
        let legacy_entries_sql: String = self
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='entries'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();
        if !legacy_entries_sql.is_empty() {
            let upper = legacy_entries_sql.to_uppercase();
            // The pre-refactor schema named the ULID column `id` as the PK.
            // Our new schema has no such column.
            if upper.contains(" ID ") || upper.contains("\tID\t") || upper.contains("(ID ") {
                return Err(DatumError::IncompatibleSchema(
                    "legacy entries table with `id` column found; this version drops ULID \
                     and uses slug as PK. Delete the DB and recreate it, or migrate from a \
                     backup with `datum migrate`.".to_string(),
                ));
            }
        }

        self.conn.execute_batch(include_str!("schema.sql"))?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Entry operations
    // -------------------------------------------------------------------------

    /// Insert an entry using the default [`ConflictStrategy::Ignore`].
    /// Returns `Err(Duplicate)` if the content_hash collides with a live row —
    /// preserved for call sites that pre-date the strategy API.
    pub fn insert_entry(&self, entry: &Entry) -> Result<()> {
        match self.insert_entry_with_strategy(entry, ConflictStrategy::Ignore)? {
            InsertOutcome::Inserted { .. } => Ok(()),
            InsertOutcome::Ignored { .. } => Err(DatumError::Duplicate(format!(
                "Entry with identical content already exists (hash: {})",
                &entry.content_hash[..entry.content_hash.len().min(12)]
            ))),
            // Ignore strategy never produces the other outcomes.
            other => unreachable!("Ignore strategy produced {other:?}"),
        }
    }

    /// Insert an entry and dispatch on [`ConflictStrategy`] when the incoming
    /// `content_hash` already matches a live row. Tombstoned rows are ignored
    /// for collision detection — re-inserting a deleted-then-identical payload
    /// behaves like a fresh insert.
    pub fn insert_entry_with_strategy(
        &self,
        entry: &Entry,
        strategy: ConflictStrategy,
    ) -> Result<InsertOutcome> {
        let existing: Option<(String, Option<String>)> = self
            .conn
            .query_row(
                "SELECT slug, extra_data FROM entries
                 WHERE content_hash = ?1 AND deleted_at IS NULL
                 LIMIT 1",
                params![entry.content_hash],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()?;

        match (existing, strategy) {
            (None, _) => {
                let stored_slug = self.raw_insert(entry)?;
                Ok(InsertOutcome::Inserted { slug: stored_slug })
            }
            (Some((slug, _)), ConflictStrategy::Ignore) => {
                Ok(InsertOutcome::Ignored { slug })
            }
            (Some((slug, _)), ConflictStrategy::Update) => {
                self.raw_overwrite_mutables(&slug, entry, None)?;
                Ok(InsertOutcome::Updated { slug })
            }
            (Some((slug, existing_extra_json)), ConflictStrategy::Merge) => {
                let existing_extra: Option<JsonValue> = existing_extra_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok());
                let merged = merge_extra_data(existing_extra.as_ref(), entry.extra_data.as_ref());
                self.raw_overwrite_mutables(&slug, entry, Some(merged))?;
                Ok(InsertOutcome::Merged { slug })
            }
            (Some((old_slug, _)), ConflictStrategy::Rekey) => {
                // Rekey: insert the incoming row with its own slug. Both rows
                // share a content_hash — the content_hash column is not UNIQUE.
                let new_slug = self.raw_insert(entry)?;
                Ok(InsertOutcome::Rekeyed { old_slug, new_slug })
            }
        }
    }

    /// Insert an entry, retrying on slug/hid collisions with a suffixed slug
    /// or a freshly-generated hid. Returns the slug actually stored.
    fn raw_insert(&self, entry: &Entry) -> Result<String> {
        const MAX_ATTEMPTS: u8 = 8;
        let mut slug = entry.slug.clone();
        let mut hid = entry.hid.clone();
        let mut attempt = 0u8;

        loop {
            match self.try_insert(&slug, &hid, entry) {
                Ok(()) => return Ok(slug),
                Err(DatumError::Db(ref sql_err)) if is_pk_collision(sql_err) => {
                    if attempt >= MAX_ATTEMPTS {
                        return Err(DatumError::Other(format!(
                            "slug/hid collision persisted after {MAX_ATTEMPTS} retries",
                        )));
                    }
                    // The collision could be on slug PK or hid UNIQUE. We
                    // can't cheaply tell which without a second query, so
                    // rotate both to converge quickly.
                    slug = suffixed_slug(&entry.slug);
                    hid = new_hid();
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn try_insert(&self, slug: &str, hid: &str, entry: &Entry) -> Result<()> {
        self.conn.execute(
            r#"INSERT INTO entries (
                slug, hid, content_hash, field, verb,
                value, extra_data, unit,
                time_start_utc, time_start_offset, time_start_tz,
                time_end_utc, time_end_offset, time_end_tz,
                lat, lon, location,
                created_at, modified_at, device_id,
                deleted_at, id_structure, field_structure
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14,
                ?15, ?16, ?17,
                ?18, ?19, ?20,
                ?21, ?22, ?23
            )"#,
            params![
                slug,
                hid,
                entry.content_hash,
                entry.field,
                entry.verb.to_string(),
                entry.value.as_ref().map(|v| v.to_string()),
                entry.extra_data.as_ref().map(|v| v.to_string()),
                entry.unit,
                entry.time_start.as_ref().map(|t| t.utc.to_rfc3339()),
                entry.time_start.as_ref().map(|t| t.offset_minutes),
                entry.time_start.as_ref().map(|t| t.timezone.as_str()),
                entry.time_end.as_ref().map(|t| t.utc.to_rfc3339()),
                entry.time_end.as_ref().map(|t| t.offset_minutes),
                entry.time_end.as_ref().map(|t| t.timezone.as_str()),
                entry.lat,
                entry.lon,
                entry.location.as_deref(),
                entry.created_at.to_rfc3339(),
                entry.modified_at.to_rfc3339(),
                entry.device_id,
                entry.deleted_at.map(|t| t.to_rfc3339()),
                entry.id_structure.as_deref(),
                entry.field_structure.as_deref(),
            ],
        )?;
        Ok(())
    }

    /// Overwrite an existing row identified by `slug` with the mutable fields
    /// from `entry`. When `override_extra` is `Some`, it replaces the stored
    /// `extra_data`; otherwise the incoming `entry.extra_data` wins.
    /// Refreshes `modified_at` (and recomputes `content_hash` from the
    /// incoming payload). The slug is immutable — this never renames the row.
    fn raw_overwrite_mutables(
        &self,
        slug: &str,
        entry: &Entry,
        override_extra: Option<Option<JsonValue>>,
    ) -> Result<()> {
        let extra_value: Option<&JsonValue> = match override_extra.as_ref() {
            Some(o) => o.as_ref(),
            None => entry.extra_data.as_ref(),
        };
        let extra_json = extra_value.map(|v| v.to_string());
        let now = Utc::now().to_rfc3339();

        self.conn.execute(
            r#"UPDATE entries SET
                content_hash = ?2,
                field = ?3,
                verb = ?4,
                value = ?5,
                extra_data = ?6,
                unit = ?7,
                time_start_utc = ?8,
                time_start_offset = ?9,
                time_start_tz = ?10,
                time_end_utc = ?11,
                time_end_offset = ?12,
                time_end_tz = ?13,
                lat = ?14,
                lon = ?15,
                location = ?16,
                modified_at = ?17,
                device_id = ?18
               WHERE slug = ?1"#,
            params![
                slug,
                entry.content_hash,
                entry.field,
                entry.verb.to_string(),
                entry.value.as_ref().map(|v| v.to_string()),
                extra_json,
                entry.unit,
                entry.time_start.as_ref().map(|t| t.utc.to_rfc3339()),
                entry.time_start.as_ref().map(|t| t.offset_minutes),
                entry.time_start.as_ref().map(|t| t.timezone.as_str()),
                entry.time_end.as_ref().map(|t| t.utc.to_rfc3339()),
                entry.time_end.as_ref().map(|t| t.offset_minutes),
                entry.time_end.as_ref().map(|t| t.timezone.as_str()),
                entry.lat,
                entry.lon,
                entry.location.as_deref(),
                now,
                entry.device_id,
            ],
        )?;
        Ok(())
    }

    /// Persist a mutation to an existing entry, matched by `entry.slug`.
    /// Errors [`DatumError::NotFound`] if the slug is unknown and
    /// [`DatumError::Tombstoned`] if the row is soft-deleted. Refreshes
    /// `modified_at` and rewrites all mutable fields (including
    /// `content_hash`, `extra_data`) from the incoming payload.
    pub fn save_existing_entry(&self, entry: &Entry) -> Result<()> {
        let state: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT deleted_at FROM entries WHERE slug = ?1",
                params![entry.slug],
                |row| row.get(0),
            )
            .optional()?;
        match state {
            None => Err(DatumError::NotFound(entry.slug.clone())),
            Some(Some(_)) => Err(DatumError::Tombstoned(entry.slug.clone())),
            Some(None) => self.raw_overwrite_mutables(&entry.slug, entry, None),
        }
    }

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

    /// Query live entries for a field, ordered by time_start descending.
    pub fn entries_for_field(&self, field: &str, limit: Option<usize>) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1 AND deleted_at IS NULL
             ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) DESC{}",
            limit.map(|l| format!(" LIMIT {l}")).unwrap_or_default()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![field], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Query live entries for a field, ordered by time_start ascending (oldest first).
    pub fn entries_for_field_asc(&self, field: &str, limit: Option<usize>) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT * FROM entries
             WHERE field = ?1 AND deleted_at IS NULL
             ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) ASC{}",
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
        let metric_expr: &str = match params.metric {
            TimeMetric::Hybrid => "COALESCE(time_start_utc, modified_at)",
            TimeMetric::Occur => "time_start_utc",
            TimeMetric::Create => "created_at",
            TimeMetric::Modify => "modified_at",
        };

        let mut sql = String::from("SELECT * FROM entries WHERE deleted_at IS NULL");
        let mut bindings: Vec<String> = Vec::new();

        if params.metric == TimeMetric::Occur {
            sql.push_str(" AND time_start_utc IS NOT NULL");
        }
        if let Some(field) = &params.field {
            sql.push_str(" AND field = ?");
            bindings.push(field.clone());
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
        let rows = stmt.query_map(
            rusqlite::params_from_iter(bindings.iter()),
            row_to_entry,
        )?;
        let mut results: Vec<Entry> =
            rows.map(|r| r.map_err(DatumError::Db)).collect::<Result<_>>()?;

        // Normalize to oldest-first; then honour the caller's reverse.
        if !params.head {
            results.reverse();
        }
        if params.reverse {
            results.reverse();
        }
        Ok(results)
    }

    /// Get all live entries within a UTC time range (inclusive).
    pub fn entries_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT * FROM entries
               WHERE COALESCE(time_start_utc, time_end_utc, created_at) >= ?1
                 AND COALESCE(time_start_utc, time_end_utc, created_at) <= ?2
                 AND deleted_at IS NULL
               ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) ASC"#,
        )?;
        let rows = stmt.query_map(
            params![start.to_rfc3339(), end.to_rfc3339()],
            row_to_entry,
        )?;
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

    /// Count of live (non-tombstoned) entries in the store.
    pub fn count_entries(&self) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(n as usize)
    }

    /// Every entry in the store, **including tombstoned rows**. Undo uses this
    /// to locate deleted entries by slug/hid before restoring them.
    pub fn list_including_deleted(&self) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare("SELECT * FROM entries ORDER BY slug ASC")?;
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

    /// Clear a tombstone by slug or hid — the inverse of [`Db::soft_delete_entry`].
    /// Errors with `NotFound` if no matching tombstone exists.
    pub fn undelete(&self, slug_or_hid: &str) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE entries SET deleted_at = NULL
             WHERE (slug = ?1 OR hid = ?1) AND deleted_at IS NOT NULL",
            params![slug_or_hid],
        )?;
        if affected == 0 {
            Err(DatumError::NotFound(slug_or_hid.to_string()))
        } else {
            Ok(())
        }
    }

    /// Hard-delete an entry by slug or hid. Returns Err(NotFound) if no rows
    /// affected. Bypasses the soft-delete tombstone — used by tests and
    /// admin paths that need the row physically gone.
    pub fn delete_entry(&self, slug_or_hid: &str) -> Result<()> {
        let affected = self.conn.execute(
            "DELETE FROM entries WHERE slug = ?1 OR hid = ?1",
            params![slug_or_hid],
        )?;
        if affected == 0 {
            Err(DatumError::NotFound(slug_or_hid.to_string()))
        } else {
            Ok(())
        }
    }

    // -------------------------------------------------------------------------
    // Aliases
    // -------------------------------------------------------------------------

    /// Look up a single alias by name. Returns None if not found.
    pub fn lookup_alias(&self, name: &str) -> Result<Option<Alias>> {
        let result: Option<String> = self.conn
            .query_row(
                "SELECT alias_json FROM aliases WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?;
        match result {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Delete a user-created alias. Fails silently if it doesn't exist.
    /// Does NOT delete derived aliases (those are managed by their owning spec).
    pub fn delete_alias(&self, name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM aliases WHERE name = ?1 AND derived_from_field IS NULL",
            params![name],
        )?;
        Ok(())
    }

    /// Save a user-created alias (derived_from_field = NULL).
    pub fn save_alias(&self, alias: &Alias) -> Result<()> {
        let json = serde_json::to_string(alias)?;
        self.conn.execute(
            "INSERT INTO aliases (name, alias_json, derived_from_field) VALUES (?1, ?2, NULL)
             ON CONFLICT(name) DO UPDATE SET alias_json = excluded.alias_json,
                                             derived_from_field = NULL",
            params![alias.name, json],
        )?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Field specs (source of truth)
    // -------------------------------------------------------------------------

    /// Load a field spec by name. None if not defined.
    pub fn load_spec(&self, field: &str) -> Result<Option<FieldSpec>> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT spec_json FROM field_specs WHERE field = ?1",
                params![field],
                |row| row.get(0),
            )
            .optional()?;
        match json {
            Some(j) => Ok(Some(serde_json::from_str(&j)?)),
            None => Ok(None),
        }
    }

    /// Save a field spec and sync its derived aliases into the aliases table.
    /// Any previously-derived aliases for this field are removed first (so
    /// states that were dropped lose their alias).
    pub fn save_field_spec(&self, spec: &FieldSpec) -> Result<()> {
        let json = serde_json::to_string(spec)?;
        let tx = self.conn.unchecked_transaction()?;

        tx.execute(
            "INSERT OR REPLACE INTO field_specs (field, spec_json) VALUES (?1, ?2)",
            params![spec.field, json],
        )?;

        // Replace the set of derived aliases for this field.
        tx.execute(
            "DELETE FROM aliases WHERE derived_from_field = ?1",
            params![spec.field],
        )?;

        for d in &spec.derived_aliases {
            let alias = Alias {
                name: d.state.clone(),
                expands_to: d.expands_to.clone(),
                description: Some(format!("Derived from {} spec", spec.field)),
            };
            let alias_json = serde_json::to_string(&alias)?;
            // `INSERT OR REPLACE` so a derived alias overrides any stale
            // user alias with the same name. Collisions between derived
            // aliases from different fields are last-write-wins.
            tx.execute(
                "INSERT OR REPLACE INTO aliases (name, alias_json, derived_from_field) VALUES (?1, ?2, ?3)",
                params![alias.name, alias_json, spec.field],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Delete a field spec. Cascades to its derived aliases.
    pub fn delete_field_spec(&self, field: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM aliases WHERE derived_from_field = ?1",
            params![field],
        )?;
        tx.execute(
            "DELETE FROM field_specs WHERE field = ?1",
            params![field],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Shift `time_start` / `time_end` on every live entry matching `filter`
    /// by `offset`. Recomputes `content_hash` and bumps `modified_at` on each
    /// affected row. Slug is left unchanged.
    ///
    /// Returns the number of rows updated.
    pub fn update_times(&self, filter: &RetimeFilter, offset: Duration) -> Result<usize> {
        let mut sql = String::from(
            "SELECT * FROM entries WHERE deleted_at IS NULL",
        );
        let mut bindings: Vec<String> = Vec::new();

        if let Some(needle) = filter.slug_or_hid {
            sql.push_str(" AND (slug = ? OR hid = ?)");
            bindings.push(needle.to_string());
            bindings.push(needle.to_string());
        }
        if let Some(field) = filter.field {
            sql.push_str(" AND field = ?");
            bindings.push(field.to_string());
        }
        if let Some(after) = filter.after_utc {
            sql.push_str(" AND time_start_utc IS NOT NULL AND time_start_utc >= ?");
            bindings.push(after.to_rfc3339());
        }
        if let Some(before) = filter.before_utc {
            sql.push_str(" AND time_start_utc IS NOT NULL AND time_start_utc <= ?");
            bindings.push(before.to_rfc3339());
        }

        let tx = self.conn.unchecked_transaction()?;
        let entries: Vec<Entry> = {
            let mut stmt = tx.prepare(&sql)?;
            let rows = stmt.query_map(
                rusqlite::params_from_iter(bindings.iter()),
                row_to_entry,
            )?;
            rows.map(|r| r.map_err(DatumError::Db))
                .collect::<Result<_>>()?
        };

        let now = Utc::now().to_rfc3339();
        let mut count = 0usize;
        for mut e in entries {
            if let Some(t) = e.time_start.as_mut() {
                t.utc += offset;
            }
            if let Some(t) = e.time_end.as_mut() {
                t.utc += offset;
            }
            let new_hash = e.expected_content_hash();
            tx.execute(
                r#"UPDATE entries SET
                    time_start_utc = ?2,
                    time_end_utc   = ?3,
                    content_hash   = ?4,
                    modified_at    = ?5
                   WHERE slug = ?1"#,
                params![
                    e.slug,
                    e.time_start.as_ref().map(|t| t.utc.to_rfc3339()),
                    e.time_end.as_ref().map(|t| t.utc.to_rfc3339()),
                    new_hash,
                    now,
                ],
            )?;
            count += 1;
        }
        tx.commit()?;
        Ok(count)
    }

    /// Soft-delete an entry by slug or hid (tombstone).
    ///
    /// Sets `deleted_at` to now; read paths that care about soft-delete (such
    /// as `get_active_state`) will ignore tombstoned rows. Errors if no row
    /// matches. If the row is already tombstoned, the tombstone is refreshed.
    pub fn soft_delete_entry(&self, slug_or_hid: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let affected = self.conn.execute(
            "UPDATE entries SET deleted_at = ?1 WHERE slug = ?2 OR hid = ?2",
            params![now, slug_or_hid],
        )?;
        if affected == 0 {
            Err(DatumError::NotFound(slug_or_hid.to_string()))
        } else {
            Ok(())
        }
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
        let mut stmt = self.conn.prepare(
            r#"SELECT * FROM entries
               WHERE field = ?1
                 AND deleted_at IS NULL
                 AND verb IN ('switch', 'start', 'stop', 'occur')
                 AND (
                      (time_start_utc IS NOT NULL AND time_start_utc <= ?2)
                   OR (time_end_utc   IS NOT NULL AND time_end_utc   <= ?2)
                 )
               ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) ASC"#,
        )?;
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
                 AND time_start_utc IS NOT NULL
                 AND time_start_utc <= ?2
               ORDER BY time_start_utc DESC, created_at DESC
               LIMIT 1"#,
        )?;
        let row = stmt
            .query_row(params![field, at_str], row_to_entry)
            .optional()?;
        Ok(row)
    }

    // -------------------------------------------------------------------------
    // Consistency checks
    // -------------------------------------------------------------------------

    /// Rows where both time bounds exist and `time_end` precedes `time_start`.
    pub fn find_time_end_before_start(&self) -> Result<Vec<TimeOrderIssue>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT slug, field, time_start_utc, time_end_utc FROM entries
               WHERE time_start_utc IS NOT NULL
                 AND time_end_utc IS NOT NULL
                 AND time_end_utc < time_start_utc
                 AND deleted_at IS NULL
               ORDER BY slug ASC"#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TimeOrderIssue {
                slug: row.get(0)?,
                field: row.get(1)?,
                time_start_utc: row.get(2)?,
                time_end_utc: row.get(3)?,
            })
        })?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Distinct field names across all non-deleted entries.
    pub fn distinct_fields(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT field FROM entries WHERE deleted_at IS NULL ORDER BY field",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// All entries (including tombstoned), for bulk checks.
    pub fn all_entries(&self) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare("SELECT * FROM entries ORDER BY slug ASC")?;
        let rows = stmt.query_map([], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Slugs of rows with `deleted_at` set (tombstones).
    pub fn tombstone_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT slug FROM entries WHERE deleted_at IS NOT NULL ORDER BY slug ASC",
        )?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    pub fn load_registry(&self) -> Result<SpecRegistry> {
        let mut registry = SpecRegistry::new();

        let mut stmt = self.conn.prepare("SELECT spec_json FROM field_specs")?;
        let specs = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for spec_json in specs {
            let spec: FieldSpec = serde_json::from_str(&spec_json?)?;
            registry.register_field(spec);
        }

        let mut stmt = self.conn.prepare("SELECT alias_json FROM aliases")?;
        let aliases = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for alias_json in aliases {
            let alias: Alias = serde_json::from_str(&alias_json?)?;
            registry.register_alias(alias);
        }

        Ok(registry)
    }

    // -------------------------------------------------------------------------
    // Tracking / colors — thin projection over FieldSpec
    // -------------------------------------------------------------------------

    fn load_or_new_spec(&self, field: &str) -> Result<FieldSpec> {
        match self.load_spec(field)? {
            Some(s) => Ok(s),
            None => Ok(FieldSpec::new(field, Verb::Occur)),
        }
    }

    /// Set a field's tracking state (and optionally its color).
    /// Creates the spec if it doesn't exist yet.
    pub fn set_tracked(&self, field: &str, ignored: bool, color: Option<&str>) -> Result<()> {
        let mut spec = self.load_or_new_spec(field)?;
        spec.tracked = true;
        spec.ignored = ignored;
        if let Some(c) = color {
            spec.color = Some(c.to_string());
        }
        self.save_field_spec(&spec)
    }

    /// Ensure a field has a tracked spec. Does nothing if a spec already exists.
    pub fn ensure_tracked(&self, field: &str) -> Result<()> {
        if self.load_spec(field)?.is_some() {
            return Ok(());
        }
        let mut spec = FieldSpec::new(field, Verb::Occur);
        spec.tracked = true;
        self.save_field_spec(&spec)
    }

    /// Return all specs flagged `tracked`, projected to TrackedField.
    /// Ordered alphabetically by field name.
    pub fn get_tracked_fields(&self) -> Result<Vec<TrackedField>> {
        let mut stmt = self
            .conn
            .prepare("SELECT spec_json FROM field_specs ORDER BY field")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            let spec: FieldSpec = serde_json::from_str(&r.map_err(DatumError::Db)?)?;
            if !spec.tracked {
                continue;
            }
            out.push(TrackedField {
                field: spec.field,
                ignored: spec.ignored,
                color: spec.color,
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

    /// Latest non-Note, non-tombstoned entry for a field, for use in the now view.
    pub fn latest_entry_for_now(&self, field: &str) -> Result<Option<Entry>> {
        self.conn
            .query_row(
                "SELECT * FROM entries
                 WHERE field = ?1 AND verb != 'note' AND deleted_at IS NULL
                 ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) DESC
                 LIMIT 1",
                params![field],
                row_to_entry,
            )
            .optional()
            .map_err(DatumError::Db)
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    let verb_str: String = row.get("verb")?;
    let verb: Verb = verb_str.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
    })?;

    let value: Option<JsonValue> = row
        .get::<_, Option<String>>("value")?
        .and_then(|s| serde_json::from_str(&s).ok());

    let extra_data: Option<JsonValue> = row
        .get::<_, Option<String>>("extra_data")?
        .and_then(|s| serde_json::from_str(&s).ok());

    let time_start = build_datum_time(
        row.get("time_start_utc")?,
        row.get("time_start_offset")?,
        row.get("time_start_tz")?,
    );
    let time_end = build_datum_time(
        row.get("time_end_utc")?,
        row.get("time_end_offset")?,
        row.get("time_end_tz")?,
    );

    let created_at: DateTime<Utc> = row
        .get::<_, String>("created_at")?
        .parse()
        .map_err(|e: chrono::ParseError| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
        })?;
    let modified_at: DateTime<Utc> = row
        .get::<_, String>("modified_at")?
        .parse()
        .map_err(|e: chrono::ParseError| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, e.into())
        })?;
    let deleted_at: Option<DateTime<Utc>> = row
        .get::<_, Option<String>>("deleted_at")?
        .and_then(|s| s.parse().ok());

    Ok(Entry {
        slug: row.get("slug")?,
        hid: row.get("hid")?,
        content_hash: row.get("content_hash")?,
        field: row.get("field")?,
        verb,
        value,
        extra_data,
        unit: row.get("unit")?,
        time_start,
        time_end,
        lat: row.get("lat")?,
        lon: row.get("lon")?,
        location: row.get("location")?,
        created_at,
        modified_at,
        device_id: row.get("device_id")?,
        deleted_at,
        id_structure: row.get("id_structure")?,
        field_structure: row.get("field_structure")?,
    })
}

/// Append a short random base36 suffix to a slug so a retried insert can
/// dodge a PK collision without looking indistinguishable from the original.
fn suffixed_slug(base: &str) -> String {
    let mut rng = rand::thread_rng();
    let suffix: String = (0..4)
        .map(|_| {
            let n = rng.gen_range(0..36u8);
            if n < 10 {
                (b'0' + n) as char
            } else {
                (b'a' + (n - 10)) as char
            }
        })
        .collect();
    format!("{base}_{suffix}")
}

/// SQLite returns a constraint-violation error when a UNIQUE or PRIMARY KEY
/// is about to be duplicated. We don't try to distinguish slug vs hid here —
/// the caller rotates both on retry.
fn is_pk_collision(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _) if e.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

/// Deep-merge `incoming` into `existing` for use by [`ConflictStrategy::Merge`].
///
/// - If either side is `None`, the other is returned.
/// - Object + object: keys are unioned; nested objects recurse; scalar
///   conflicts → incoming wins (LWW).
/// - Any non-object pair: incoming replaces existing.
fn merge_extra_data(
    existing: Option<&JsonValue>,
    incoming: Option<&JsonValue>,
) -> Option<JsonValue> {
    match (existing, incoming) {
        (None, None) => None,
        (Some(e), None) => Some(e.clone()),
        (None, Some(n)) => Some(n.clone()),
        (Some(e), Some(n)) => Some(merge_json(e.clone(), n.clone())),
    }
}

fn merge_json(existing: JsonValue, incoming: JsonValue) -> JsonValue {
    match (existing, incoming) {
        (JsonValue::Object(mut e_map), JsonValue::Object(n_map)) => {
            for (k, v) in n_map {
                match e_map.remove(&k) {
                    Some(existing_v) => {
                        e_map.insert(k, merge_json(existing_v, v));
                    }
                    None => {
                        e_map.insert(k, v);
                    }
                }
            }
            JsonValue::Object(e_map)
        }
        (_, incoming) => incoming,
    }
}

fn build_datum_time(
    utc_str: Option<String>,
    offset: Option<i32>,
    tz: Option<String>,
) -> Option<DatumTime> {
    let utc_str = utc_str?;
    let utc: DateTime<Utc> = utc_str.parse().ok()?;
    Some(DatumTime {
        utc,
        offset_minutes: offset.unwrap_or(0),
        timezone: tz.unwrap_or_else(|| "UTC".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Entry;
    use crate::field_spec::DerivedAlias;
    use crate::time::DatumTime;

    #[test]
    fn test_insert_and_fetch() {
        let db = Db::open_in_memory().unwrap();
        let t = DatumTime::now();
        let entry = Entry::new(
            "coffee",
            Verb::Occur,
            Some(serde_json::json!(2)),
            None,
            Some("cups".to_string()),
            Some(t),
            None,
            "test-device",
        );
        let slug = entry.slug.clone();
        let hid = entry.hid.clone();

        db.insert_entry(&entry).unwrap();

        // Look up by slug (primary) and by hid (secondary).
        let fetched = db.get_entry(&slug).unwrap();
        assert_eq!(fetched.field, "coffee");
        assert_eq!(fetched.unit, Some("cups".to_string()));

        let fetched_by_hid = db.get_entry(&hid).unwrap();
        assert_eq!(fetched_by_hid.slug, slug);
    }

    #[test]
    fn test_duplicate_rejected() {
        let db = Db::open_in_memory().unwrap();
        let t = DatumTime::now();
        let entry = Entry::new("mood", Verb::Switch, Some(serde_json::json!("happy")), None, None, Some(t), None, "d1");
        db.insert_entry(&entry).unwrap();
        // Same content_hash → should fail
        let err = db.insert_entry(&entry);
        assert!(matches!(err, Err(DatumError::Duplicate(_))));
    }

    #[test]
    fn test_entries_for_field() {
        let db = Db::open_in_memory().unwrap();
        for state in &["happy", "sad", "focused"] {
            let t = DatumTime::now();
            let e = Entry::new("mood", Verb::Switch, Some(serde_json::json!(state)), None, None, Some(t), None, "d1");
            db.insert_entry(&e).unwrap();
        }
        let entries = db.entries_for_field("mood", None).unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_entries_for_field_asc() {
        let db = Db::open_in_memory().unwrap();
        let base: chrono::DateTime<chrono::Utc> =
            "2024-01-01T00:00:00Z".parse().unwrap();
        for secs in &[300i64, 100, 200] {
            let utc = base + chrono::Duration::seconds(*secs);
            let t = DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() };
            let e = Entry::new(
                "mood",
                Verb::Occur,
                Some(serde_json::json!(*secs)),
                None,
                None,
                Some(t),
                None,
                "d1",
            );
            db.insert_entry(&e).unwrap();
        }
        let entries = db.entries_for_field_asc("mood", None).unwrap();
        assert_eq!(entries.len(), 3);
        let values: Vec<i64> = entries
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![100, 200, 300]);

        let limited = db.entries_for_field_asc("mood", Some(2)).unwrap();
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].value.as_ref().unwrap().as_i64().unwrap(), 100);
        assert_eq!(limited[1].value.as_ref().unwrap().as_i64().unwrap(), 200);
    }

    #[test]
    fn test_set_tracked_and_get() {
        let db = Db::open_in_memory().unwrap();
        db.set_tracked("coffee", false, None).unwrap();
        db.set_tracked("mood", false, Some("ff6600")).unwrap();
        db.set_tracked("youtube", true, None).unwrap();

        let fields = db.get_tracked_fields().unwrap();
        assert_eq!(fields.len(), 3);

        let coffee = fields.iter().find(|f| f.field == "coffee").unwrap();
        assert!(!coffee.ignored);
        assert!(coffee.color.is_none());

        let mood = fields.iter().find(|f| f.field == "mood").unwrap();
        assert!(!mood.ignored);
        assert_eq!(mood.color.as_deref(), Some("ff6600"));

        let yt = fields.iter().find(|f| f.field == "youtube").unwrap();
        assert!(yt.ignored);
    }

    #[test]
    fn test_ensure_tracked_inserts_and_idempotent() {
        let db = Db::open_in_memory().unwrap();
        db.ensure_tracked("coffee").unwrap();
        let fields = db.get_tracked_fields().unwrap();
        assert_eq!(fields.len(), 1);
        assert!(!fields[0].ignored);

        db.ensure_tracked("coffee").unwrap();
        let fields2 = db.get_tracked_fields().unwrap();
        assert_eq!(fields2.len(), 1);
    }

    #[test]
    fn test_ensure_tracked_does_not_override_ignored() {
        let db = Db::open_in_memory().unwrap();
        db.set_tracked("youtube", true, None).unwrap();
        db.ensure_tracked("youtube").unwrap();
        let fields = db.get_tracked_fields().unwrap();
        assert!(fields[0].ignored, "ignored flag must be preserved");
    }

    #[test]
    fn test_state_colors() {
        let db = Db::open_in_memory().unwrap();
        db.set_state_color("mood", "happy", "00ff00").unwrap();
        db.set_state_color("mood", "sad", "ff0000").unwrap();

        let map = db.get_all_state_colors().unwrap();
        assert_eq!(map.get(&("mood".to_string(), "happy".to_string())).map(|s| s.as_str()), Some("00ff00"));
        assert_eq!(map.get(&("mood".to_string(), "sad".to_string())).map(|s| s.as_str()), Some("ff0000"));
    }

    // ------------------------------------------------------------------
    // get_active_state — A5 acceptance tests
    // ------------------------------------------------------------------

    fn utc_at(h: u32, m: u32) -> DatumTime {
        let utc = chrono::DateTime::parse_from_rfc3339(
            &format!("2024-01-01T{h:02}:{m:02}:00Z"),
        )
        .unwrap()
        .with_timezone(&Utc);
        DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        }
    }

    fn as_utc(h: u32, m: u32) -> chrono::DateTime<Utc> {
        utc_at(h, m).utc
    }

    #[test]
    fn active_state_returns_none_before_any_entry() {
        let db = Db::open_in_memory().unwrap();
        let got = db.get_active_state("mood", as_utc(12, 0)).unwrap();
        assert_eq!(got, JsonValue::Null);
    }

    #[test]
    fn active_state_returns_last_switch_before_time() {
        let db = Db::open_in_memory().unwrap();
        let e1 = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            None,
            Some(utc_at(9, 0)),
            None,
            "d",
        );
        let e2 = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("focused")),
            None,
            None,
            Some(utc_at(10, 0)),
            None,
            "d",
        );
        let e3 = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("tired")),
            None,
            None,
            Some(utc_at(15, 0)),
            None,
            "d",
        );
        db.insert_entry(&e1).unwrap();
        db.insert_entry(&e2).unwrap();
        db.insert_entry(&e3).unwrap();

        // At 12:00 → "focused"; at 09:30 → "happy"; at 16:00 → "tired"; at 08:59 → null
        assert_eq!(
            db.get_active_state("mood", as_utc(12, 0)).unwrap(),
            serde_json::json!("focused")
        );
        assert_eq!(
            db.get_active_state("mood", as_utc(9, 30)).unwrap(),
            serde_json::json!("happy")
        );
        assert_eq!(
            db.get_active_state("mood", as_utc(16, 0)).unwrap(),
            serde_json::json!("tired")
        );
        assert_eq!(
            db.get_active_state("mood", as_utc(8, 59)).unwrap(),
            JsonValue::Null
        );
    }

    #[test]
    fn active_state_returns_none_after_stop() {
        let db = Db::open_in_memory().unwrap();
        let start = Entry::new(
            "run",
            Verb::Start,
            None,
            None,
            None,
            Some(utc_at(9, 0)),
            None,
            "d",
        );
        let stop = Entry::new(
            "run",
            Verb::Stop,
            None,
            None,
            None,
            Some(utc_at(10, 0)),
            None,
            "d",
        );
        db.insert_entry(&start).unwrap();
        db.insert_entry(&stop).unwrap();

        // During the run → true
        assert_eq!(
            db.get_active_state("run", as_utc(9, 30)).unwrap(),
            serde_json::json!(true),
        );
        // After stop → false (not null — "stopped" is a distinct state)
        assert_eq!(
            db.get_active_state("run", as_utc(10, 30)).unwrap(),
            serde_json::json!(false),
        );
    }

    #[test]
    fn active_state_handles_concurrent_blocks() {
        // Two overlapping start entries: the LATER start wins while both are
        // active; when one stops, the other's state still governs.
        let db = Db::open_in_memory().unwrap();
        // First start: switch to "a" at 09:00, ends 12:00
        let a = Entry::new(
            "task",
            Verb::Switch,
            Some(serde_json::json!("a")),
            None,
            None,
            Some(utc_at(9, 0)),
            Some(utc_at(12, 0)),
            "d",
        );
        // Second start: switch to "b" at 10:00, ends 11:00
        let b = Entry::new(
            "task",
            Verb::Switch,
            Some(serde_json::json!("b")),
            None,
            None,
            Some(utc_at(10, 0)),
            Some(utc_at(11, 0)),
            "d",
        );
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();

        // 09:30: only "a" active → "a"
        assert_eq!(
            db.get_active_state("task", as_utc(9, 30)).unwrap(),
            serde_json::json!("a"),
        );
        // 10:30: "b" started most recently → "b"
        assert_eq!(
            db.get_active_state("task", as_utc(10, 30)).unwrap(),
            serde_json::json!("b"),
        );
        // 11:30: "b" ended (→ false), "a" still running but its start is older → false wins as latest event
        assert_eq!(
            db.get_active_state("task", as_utc(11, 30)).unwrap(),
            serde_json::json!(false),
        );
        // 12:30: both ended → still false
        assert_eq!(
            db.get_active_state("task", as_utc(12, 30)).unwrap(),
            serde_json::json!(false),
        );
    }

    #[test]
    fn active_state_skips_tombstoned_rows() {
        let db = Db::open_in_memory().unwrap();
        let older = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("calm")),
            None,
            None,
            Some(utc_at(9, 0)),
            None,
            "d",
        );
        let newer = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("angry")),
            None,
            None,
            Some(utc_at(12, 0)),
            None,
            "d",
        );
        let newer_slug = newer.slug.clone();
        db.insert_entry(&older).unwrap();
        db.insert_entry(&newer).unwrap();

        // Before tombstone: active at 13:00 is "angry"
        assert_eq!(
            db.get_active_state("mood", as_utc(13, 0)).unwrap(),
            serde_json::json!("angry"),
        );

        db.soft_delete_entry(&newer_slug).unwrap();

        // After tombstone: newer entry excluded → falls back to "calm"
        assert_eq!(
            db.get_active_state("mood", as_utc(13, 0)).unwrap(),
            serde_json::json!("calm"),
        );
    }

    #[test]
    fn active_state_ignores_other_fields() {
        let db = Db::open_in_memory().unwrap();
        let other = Entry::new(
            "different",
            Verb::Switch,
            Some(serde_json::json!("x")),
            None,
            None,
            Some(utc_at(9, 0)),
            None,
            "d",
        );
        db.insert_entry(&other).unwrap();
        assert_eq!(
            db.get_active_state("mood", as_utc(12, 0)).unwrap(),
            JsonValue::Null,
        );
    }

    #[test]
    fn test_latest_entry_for_now_skips_note() {
        let db = Db::open_in_memory().unwrap();
        let t1 = DatumTime::now();
        let switch_entry = Entry::new("mood", Verb::Switch, Some(serde_json::json!("happy")), None, None, Some(t1), None, "d1");
        db.insert_entry(&switch_entry).unwrap();
        let t2 = DatumTime::now();
        let note_entry = Entry::new("mood", Verb::Note, Some(serde_json::json!("journal entry")), None, None, Some(t2), None, "d1");
        db.insert_entry(&note_entry).unwrap();

        let latest = db.latest_entry_for_now("mood").unwrap();
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().verb, Verb::Switch);
    }

    // ─── New FieldSpec-source-of-truth tests (A0 acceptance) ──────────────────

    #[test]
    fn field_spec_stores_id_template() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.id_template = Some("%field%:%state%".to_string());
        db.save_field_spec(&spec).unwrap();

        let loaded = db.load_spec("mood").unwrap().expect("spec present");
        assert_eq!(loaded.id_template.as_deref(), Some("%field%:%state%"));
    }

    #[test]
    fn field_spec_stores_color_and_states() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.color = Some("ff6600".into());
        spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
        spec.state_colors.insert("happy".into(), "00ff00".into());
        db.save_field_spec(&spec).unwrap();

        let loaded = db.load_spec("mood").unwrap().unwrap();
        assert_eq!(loaded.color.as_deref(), Some("ff6600"));
        let states = loaded.allowed_states.as_ref().unwrap();
        assert!(states.contains(&"happy".to_string()));
        assert!(states.contains(&"sad".to_string()));
        assert_eq!(loaded.state_colors.get("happy").map(|s| s.as_str()), Some("00ff00"));
    }

    #[test]
    fn adding_state_creates_derived_alias() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.allowed_states = Some(vec!["happy".into()]);
        spec.regenerate_derived_aliases();
        db.save_field_spec(&spec).unwrap();

        let alias = db.lookup_alias("happy").unwrap().expect("derived alias present");
        assert_eq!(alias.expands_to, "switch mood happy");

        // Registry also sees it as an alias.
        let reg = db.load_registry().unwrap();
        assert!(reg.aliases.contains_key("happy"));
    }

    #[test]
    fn removing_state_drops_derived_alias() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
        spec.regenerate_derived_aliases();
        db.save_field_spec(&spec).unwrap();
        assert!(db.lookup_alias("sad").unwrap().is_some());

        // Drop "sad" from allowed_states and re-save.
        let mut updated = db.load_spec("mood").unwrap().unwrap();
        updated.allowed_states = Some(vec!["happy".into()]);
        updated.regenerate_derived_aliases();
        db.save_field_spec(&updated).unwrap();

        assert!(db.lookup_alias("sad").unwrap().is_none(), "sad alias must be dropped");
        assert!(db.lookup_alias("happy").unwrap().is_some(), "happy alias must remain");
    }

    #[test]
    fn deleting_spec_cascades_to_derived_aliases() {
        let db = Db::open_in_memory().unwrap();
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.allowed_states = Some(vec!["happy".into()]);
        spec.regenerate_derived_aliases();
        db.save_field_spec(&spec).unwrap();

        db.delete_field_spec("mood").unwrap();
        assert!(db.lookup_alias("happy").unwrap().is_none());
        assert!(db.load_spec("mood").unwrap().is_none());
    }

    #[test]
    fn track_ignore_round_trip_via_spec() {
        let db = Db::open_in_memory().unwrap();

        // Track via the projection helper.
        db.set_tracked("mood", false, Some("ff6600")).unwrap();
        let spec = db.load_spec("mood").unwrap().unwrap();
        assert!(spec.tracked);
        assert!(!spec.ignored);
        assert_eq!(spec.color.as_deref(), Some("ff6600"));

        // Ignore flips the ignored bit but preserves everything else.
        db.set_tracked("mood", true, None).unwrap();
        let spec2 = db.load_spec("mood").unwrap().unwrap();
        assert!(spec2.tracked);
        assert!(spec2.ignored);
        assert_eq!(spec2.color.as_deref(), Some("ff6600"));

        // Re-track clears the ignored bit.
        db.set_tracked("mood", false, None).unwrap();
        let spec3 = db.load_spec("mood").unwrap().unwrap();
        assert!(!spec3.ignored);
        assert_eq!(spec3.color.as_deref(), Some("ff6600"));
    }

    #[test]
    fn user_alias_replaced_by_derived_alias() {
        // User saves alias "happy" → "note journal happy"
        // Then a field spec with allowed_state "happy" is created.
        // The derived alias takes over (last-write-wins).
        let db = Db::open_in_memory().unwrap();
        db.save_alias(&Alias {
            name: "happy".into(),
            expands_to: "note journal happy".into(),
            description: None,
        })
        .unwrap();

        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.allowed_states = Some(vec!["happy".into()]);
        spec.derived_aliases = vec![DerivedAlias {
            state: "happy".into(),
            expands_to: "switch mood happy".into(),
        }];
        db.save_field_spec(&spec).unwrap();

        let alias = db.lookup_alias("happy").unwrap().unwrap();
        assert_eq!(alias.expands_to, "switch mood happy");
    }

    // ────────────────────────────────────────────────────────────────────────
    // A4: Conflict strategies and soft-delete
    // ────────────────────────────────────────────────────────────────────────

    /// Build two entries whose content_hash collides (same field / time /
    /// value), with different hids so we can tell them apart. Their default
    /// slugs also collide — the insert path's retry-with-suffix logic will
    /// disambiguate on Rekey.
    fn colliding_pair() -> (Entry, Entry) {
        let t = utc_at(9, 0);
        let first = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"source": "morning"})),
            None,
            Some(t.clone()),
            None,
            "device-a",
        );
        let second = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"source": "morning"})),
            None,
            Some(t),
            None,
            "device-b",
        );
        assert_eq!(
            first.content_hash, second.content_hash,
            "fixture sanity: colliding entries must share a content_hash"
        );
        assert_ne!(first.hid, second.hid, "fixture sanity: hids must differ");
        (first, second)
    }

    #[test]
    fn insert_conflict_update_mutates_in_place() {
        let db = Db::open_in_memory().unwrap();
        let (first, second) = colliding_pair();
        let first_slug = first.slug.clone();
        db.insert_entry(&first).unwrap();

        let before_modified = db.get_entry(&first_slug).unwrap().modified_at;
        // Sleep a tick so modified_at can advance even on fast clocks.
        std::thread::sleep(std::time::Duration::from_millis(5));

        let outcome = db
            .insert_entry_with_strategy(&second, ConflictStrategy::Update)
            .unwrap();
        assert_eq!(outcome, InsertOutcome::Updated { slug: first_slug.clone() });

        // Still exactly one row; it mutated in place (original slug preserved).
        let all = db.entries_for_field("mood", None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].slug, first_slug);
        // The incoming `device_id` wins on Update.
        assert_eq!(all[0].device_id, "device-b");
        // modified_at advanced.
        assert!(
            all[0].modified_at > before_modified,
            "Update must bump modified_at"
        );
    }

    #[test]
    fn insert_conflict_merge_unions_extra_data() {
        let db = Db::open_in_memory().unwrap();
        // Existing row carries one tag; incoming brings another. Merge must keep
        // both and, on scalar conflicts, let the incoming value win.
        let t = utc_at(10, 0);
        let existing = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            Some(serde_json::json!({"source": "morning", "shared": "old"})),
            None,
            Some(t.clone()),
            None,
            "device-a",
        );
        let existing_slug = existing.slug.clone();
        db.insert_entry(&existing).unwrap();

        // Craft an incoming entry with the same content_hash but different
        // extra. Starting from a clone keeps field/time/value identical;
        // swap the extra after construction so merge has distinct payloads
        // to combine while content_hash stays equal.
        let mut incoming = existing.clone();
        incoming.hid = new_hid();
        incoming.extra_data = Some(serde_json::json!({"shared": "new", "weather": "sunny"}));

        let outcome = db
            .insert_entry_with_strategy(&incoming, ConflictStrategy::Merge)
            .unwrap();
        assert_eq!(outcome, InsertOutcome::Merged { slug: existing_slug.clone() });

        // Still one row. extra_data = union, scalar conflict → incoming wins.
        let all = db.entries_for_field("mood", None).unwrap();
        assert_eq!(all.len(), 1);
        let merged_extra = all[0].extra_data.as_ref().unwrap();
        assert_eq!(merged_extra["source"], serde_json::json!("morning"));
        assert_eq!(merged_extra["shared"], serde_json::json!("new"));
        assert_eq!(merged_extra["weather"], serde_json::json!("sunny"));
    }

    #[test]
    fn insert_conflict_rekey_creates_new_slug_keeps_old() {
        let db = Db::open_in_memory().unwrap();
        let (first, second) = colliding_pair();
        let first_slug = first.slug.clone();
        db.insert_entry(&first).unwrap();

        let outcome = db
            .insert_entry_with_strategy(&second, ConflictStrategy::Rekey)
            .unwrap();
        // Rekey inserts a fresh row; because the default slug collides, the
        // raw insert retries with a suffixed slug (`<slug>_xxxx`). The
        // outcome carries that concrete stored slug.
        let (old_slug, new_slug) = match outcome {
            InsertOutcome::Rekeyed { old_slug, new_slug } => (old_slug, new_slug),
            other => panic!("expected Rekeyed, got {other:?}"),
        };
        assert_eq!(old_slug, first_slug);
        assert_ne!(new_slug, first_slug, "rekeyed slug must be distinct");
        assert!(
            new_slug.starts_with(&first_slug),
            "rekeyed slug should start with the original: {new_slug}"
        );

        // Both rows coexist, same content_hash, distinct slugs.
        let all = db.entries_for_field("mood", None).unwrap();
        assert_eq!(all.len(), 2);
        let slugs: std::collections::HashSet<_> = all.iter().map(|e| e.slug.clone()).collect();
        assert!(slugs.contains(&first_slug));
        assert!(slugs.contains(&new_slug));
        assert_eq!(all[0].content_hash, all[1].content_hash);
    }

    #[test]
    fn insert_conflict_ignore_returns_existing() {
        let db = Db::open_in_memory().unwrap();
        let (first, second) = colliding_pair();
        let first_slug = first.slug.clone();
        db.insert_entry(&first).unwrap();

        let outcome = db
            .insert_entry_with_strategy(&second, ConflictStrategy::Ignore)
            .unwrap();
        assert_eq!(outcome, InsertOutcome::Ignored { slug: first_slug.clone() });

        // Original row untouched; second row not inserted.
        let all = db.entries_for_field("mood", None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].slug, first_slug);
        assert_eq!(all[0].device_id, "device-a");
    }

    #[test]
    fn soft_deleted_rows_excluded_from_list() {
        let db = Db::open_in_memory().unwrap();
        let live = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            None,
            Some(utc_at(9, 0)),
            None,
            "d",
        );
        let doomed = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("sad")),
            None,
            None,
            Some(utc_at(10, 0)),
            None,
            "d",
        );
        let doomed_slug = doomed.slug.clone();
        db.insert_entry(&live).unwrap();
        db.insert_entry(&doomed).unwrap();

        db.soft_delete_entry(&doomed_slug).unwrap();

        // All the live-read paths hide the tombstoned row.
        let by_field = db.entries_for_field("mood", None).unwrap();
        assert_eq!(by_field.len(), 1);
        assert_ne!(by_field[0].slug, doomed_slug);

        let by_field_asc = db.entries_for_field_asc("mood", None).unwrap();
        assert_eq!(by_field_asc.len(), 1);
        assert_ne!(by_field_asc[0].slug, doomed_slug);

        let in_range = db
            .entries_in_range(as_utc(0, 0), as_utc(23, 59))
            .unwrap();
        assert_eq!(in_range.len(), 1);
        assert_ne!(in_range[0].slug, doomed_slug);

        let got = db.get_entry(&doomed_slug);
        assert!(matches!(got, Err(DatumError::NotFound(_))));
        assert_eq!(db.count_entries().unwrap(), 1);

        // ...but the "including deleted" view keeps the row.
        let including = db.list_including_deleted().unwrap();
        assert_eq!(including.len(), 2);
        assert!(including.iter().any(|e| e.slug == doomed_slug));
        let tombstoned = db.get_entry_including_deleted(&doomed_slug).unwrap();
        assert!(tombstoned.deleted_at.is_some());
    }

    #[test]
    fn undelete_restores_row() {
        let db = Db::open_in_memory().unwrap();
        let e = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            None,
            Some(utc_at(9, 0)),
            None,
            "d",
        );
        let slug = e.slug.clone();
        db.insert_entry(&e).unwrap();
        db.soft_delete_entry(&slug).unwrap();
        assert!(matches!(db.get_entry(&slug), Err(DatumError::NotFound(_))));

        db.undelete(&slug).unwrap();
        // Row is visible again, tombstone cleared.
        let back = db.get_entry(&slug).unwrap();
        assert_eq!(back.slug, slug);
        assert!(back.deleted_at.is_none());

        // Undeleting something that isn't tombstoned (or doesn't exist) errors.
        let err = db.undelete(&slug);
        assert!(matches!(err, Err(DatumError::NotFound(_))));
        let err = db.undelete("nonexistent");
        assert!(matches!(err, Err(DatumError::NotFound(_))));
    }

    #[test]
    fn tombstoned_row_does_not_block_new_insert() {
        // Soft-deleting a row should free its content_hash slot: re-inserting
        // identical content is treated as a fresh insert, not a conflict.
        let db = Db::open_in_memory().unwrap();
        let (first, second) = colliding_pair();
        let first_slug = first.slug.clone();
        db.insert_entry(&first).unwrap();
        db.soft_delete_entry(&first_slug).unwrap();

        let outcome = db
            .insert_entry_with_strategy(&second, ConflictStrategy::Ignore)
            .unwrap();
        assert!(matches!(outcome, InsertOutcome::Inserted { .. }));
        // Original tombstone + new row = 2 total rows in the store.
        assert_eq!(db.list_including_deleted().unwrap().len(), 2);
        // But only the new one is live.
        assert_eq!(db.entries_for_field("mood", None).unwrap().len(), 1);
    }

    #[test]
    fn migrates_pre_a0_aliases_table_adds_derived_from_field() {
        // Regression (da-ndd.6): DBs created before a0 (da-n4bi) have an
        // `aliases` table with only (name, alias_json). `Db::open` must add
        // the `derived_from_field` column and index without erroring, so that
        // downstream queries like `lookup_alias` keep working.
        let path = std::env::temp_dir()
            .join(format!("datum-migrate-{}.db", new_hid()));
        let path_str = path.to_str().unwrap().to_string();

        {
            // Hand-craft the pre-a0 schema on a raw connection.
            let raw = Connection::open(&path_str).unwrap();
            raw.execute_batch(
                "CREATE TABLE aliases (\
                    name       TEXT NOT NULL PRIMARY KEY, \
                    alias_json TEXT NOT NULL);",
            )
            .unwrap();
            raw.execute(
                "INSERT INTO aliases (name, alias_json) VALUES (?1, ?2)",
                params![
                    "legacy",
                    r#"{"name":"legacy","expands_to":"note legacy","description":null}"#,
                ],
            )
            .unwrap();
        }

        let db = Db::open(&path_str)
            .expect("Db::open must migrate a pre-a0 aliases table");

        // The pre-existing row is still queryable (this hits the SELECT that
        // surfaced the original user-facing error).
        let legacy = db
            .lookup_alias("legacy")
            .expect("lookup_alias must not error on migrated DB")
            .expect("pre-existing alias row must survive migration");
        assert_eq!(legacy.name, "legacy");
        assert_eq!(legacy.expands_to, "note legacy");

        // New derived aliases can now be written (column is live).
        let derived = crate::field_spec::Alias {
            name: "happy".to_string(),
            expands_to: "switch mood happy".to_string(),
            description: None,
        };
        let derived_json = serde_json::to_string(&derived).unwrap();
        db.conn
            .execute(
                "INSERT INTO aliases (name, alias_json, derived_from_field) \
                 VALUES (?1, ?2, ?3)",
                params!["happy", derived_json, "mood"],
            )
            .expect("insert into migrated aliases table must succeed");

        // And the supporting index exists (sqlite_master lookup).
        let has_index: bool = db
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master \
                 WHERE type = 'index' AND name = 'idx_aliases_derived_from_field'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .unwrap()
            .is_some();
        assert!(
            has_index,
            "idx_aliases_derived_from_field must be created during migration"
        );

        // Reopening the migrated DB is a no-op (idempotent).
        drop(db);
        let db2 = Db::open(&path_str).expect("reopen of migrated DB must succeed");
        assert!(db2.lookup_alias("legacy").unwrap().is_some());
        assert!(db2.lookup_alias("happy").unwrap().is_some());

        std::fs::remove_file(&path_str).ok();
    }

    // ─── resolve_prefix: slug and hid prefix matching ──────────────────────

    /// Insert an entry with a handcrafted `slug` + `hid` so the tests can
    /// reason about exact prefix matches instead of random strings.
    fn insert_fixture(db: &Db, slug: &str, hid: &str, field: &str) -> Entry {
        use chrono::Utc;
        let now = Utc::now();
        let e = Entry {
            slug: slug.to_string(),
            hid: hid.to_string(),
            content_hash: format!("hash-{slug}"),
            field: field.to_string(),
            verb: Verb::Note,
            value: None,
            extra_data: None,
            unit: None,
            time_start: None,
            time_end: None,
            lat: None,
            lon: None,
            location: None,
            created_at: now,
            modified_at: now,
            device_id: "test".to_string(),
            id_structure: None,
            field_structure: None,
            deleted_at: None,
        };
        db.insert_entry(&e).unwrap();
        e
    }

    #[test]
    fn resolve_unique_prefix() {
        let db = Db::open_in_memory().unwrap();
        let e = insert_fixture(&db, "coffee:a1b2c", "abcdefghijkl", "coffee");
        // Another entry that doesn't share the prefix being tested.
        insert_fixture(&db, "tea:qrstu", "qrstuvwxyz01", "tea");

        // Slug prefix → unique hit.
        let got = db.resolve_prefix("coffee:a1").unwrap();
        assert_eq!(got.slug, e.slug);

        // Hid prefix → unique hit.
        let got = db.resolve_prefix("abcd").unwrap();
        assert_eq!(got.slug, e.slug);

        // Tombstoned rows are excluded — creating a ghost with the same prefix
        // would otherwise make this ambiguous.
        let ghost = insert_fixture(&db, "coffee:abgst", "abcdghost000", "coffee");
        db.soft_delete_entry(&ghost.slug).unwrap();
        let got = db.resolve_prefix("coffee:a1").unwrap();
        assert_eq!(got.slug, e.slug);
    }

    #[test]
    fn resolve_ambiguous_prefix_errors_with_candidates() {
        let db = Db::open_in_memory().unwrap();
        let e1 = insert_fixture(&db, "coffee:abcde", "aaaabbbbcccc", "coffee");
        let e2 = insert_fixture(&db, "coffee:abxyz", "ddddeeeeffff", "coffee");

        let err = db.resolve_prefix("coffee:ab").unwrap_err();
        match err {
            DatumError::AmbiguousPrefix { prefix, candidates } => {
                assert_eq!(prefix, "coffee:ab");
                let got_slugs: std::collections::HashSet<_> =
                    candidates.iter().map(|c| c.slug.clone()).collect();
                assert_eq!(
                    got_slugs,
                    std::collections::HashSet::from([e1.slug.clone(), e2.slug.clone()]),
                );
                let got_hids: std::collections::HashSet<_> =
                    candidates.iter().map(|c| c.hid.clone()).collect();
                assert_eq!(
                    got_hids,
                    std::collections::HashSet::from([e1.hid.clone(), e2.hid.clone()]),
                );
            }
            other => panic!("expected AmbiguousPrefix, got {other:?}"),
        }

        // The Display of the error must surface each candidate (slug + hid)
        // so the CLI output is self-disambiguating.
        let err = db.resolve_prefix("coffee:ab").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("ambiguous"), "expected 'ambiguous' in: {msg}");
        assert!(msg.contains(&e1.slug), "candidate slug missing: {msg}");
        assert!(msg.contains(&e2.slug), "candidate slug missing: {msg}");
        assert!(msg.contains(&e1.hid), "candidate hid missing: {msg}");
        assert!(msg.contains(&e2.hid), "candidate hid missing: {msg}");
    }

    #[test]
    fn resolve_full_slug_or_hid() {
        let db = Db::open_in_memory().unwrap();
        let e = insert_fixture(&db, "coffee:abcde", "aaaabbbbcccc", "coffee");
        // Another entry that shares the slug's field prefix but not the full slug.
        insert_fixture(&db, "coffee:qrstu", "qqqqrrrrssss", "coffee");

        // A full slug is a prefix of itself → must resolve uniquely.
        let got = db.resolve_prefix(&e.slug).unwrap();
        assert_eq!(got.slug, e.slug);

        // A full hid resolves too.
        let got = db.resolve_prefix(&e.hid).unwrap();
        assert_eq!(got.slug, e.slug);
    }

    #[test]
    fn resolve_unknown_errors() {
        let db = Db::open_in_memory().unwrap();
        insert_fixture(&db, "coffee:abcde", "aaaabbbbcccc", "coffee");

        let err = db.resolve_prefix("nope").unwrap_err();
        assert!(
            matches!(err, DatumError::NotFound(ref s) if s == "nope"),
            "expected NotFound, got {err:?}",
        );
        // The message should read as "no such entry" for humans.
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("not found"), "expected 'not found' in: {msg}");
    }

    #[test]
    fn resolve_prefix_escapes_like_wildcards() {
        // `_` and `%` are SQL LIKE wildcards; the prefix helper must escape them
        // so a user-typed underscore matches literally and doesn't sweep in
        // unrelated entries.
        let db = Db::open_in_memory().unwrap();
        let literal = insert_fixture(&db, "my_field:abcde", "aaaabbbbcccc", "my_field");
        // `myXfield:...` — would be matched by an un-escaped `_` wildcard.
        insert_fixture(&db, "myXfield:qrstu", "qqqqrrrrssss", "myXfield");

        let got = db.resolve_prefix("my_field:").unwrap();
        assert_eq!(got.slug, literal.slug);
    }

    #[test]
    fn slug_collision_retries_with_suffix() {
        // Two distinct entries that happen to produce identical default slugs
        // (same field + same timestamp, different content_hash). The second
        // insert is a fresh insert, not a content collision — the raw insert
        // must dodge the slug PK collision via a suffixed retry.
        let db = Db::open_in_memory().unwrap();
        let t = utc_at(9, 0);
        let a = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("a")),
            None,
            None,
            Some(t.clone()),
            None,
            "d",
        );
        let b = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("b")),
            None,
            None,
            Some(t),
            None,
            "d",
        );
        assert_eq!(a.slug, b.slug, "fixture: default slugs must collide");
        assert_ne!(a.content_hash, b.content_hash, "fixture: hashes distinct");

        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();

        let all = db.entries_for_field("mood", None).unwrap();
        assert_eq!(all.len(), 2);
        let slugs: std::collections::HashSet<_> = all.iter().map(|e| e.slug.clone()).collect();
        assert_eq!(slugs.len(), 2, "stored slugs must be distinct");
        assert!(slugs.contains(&a.slug));
        assert!(
            slugs.iter().any(|s| s.starts_with(&a.slug) && s != &a.slug),
            "expected a suffixed variant of {} in {slugs:?}",
            a.slug,
        );
    }

    #[test]
    fn legacy_entries_schema_is_rejected() {
        // Databases created by pre-refactor datum carry an `id` column as
        // the entries PK. This version drops ULID; silent migration would
        // destroy identity semantics, so `Db::open` must error out instead.
        let path = std::env::temp_dir()
            .join(format!("datum-legacy-{}.db", new_hid()));
        let path_str = path.to_str().unwrap().to_string();

        {
            let raw = Connection::open(&path_str).unwrap();
            raw.execute_batch(
                "CREATE TABLE entries (\
                    id           TEXT NOT NULL PRIMARY KEY, \
                    slug         TEXT NOT NULL, \
                    content_hash TEXT NOT NULL, \
                    field        TEXT NOT NULL, \
                    verb         TEXT NOT NULL, \
                    created_at   TEXT NOT NULL, \
                    modified_at  TEXT NOT NULL, \
                    device_id    TEXT NOT NULL\
                );",
            )
            .unwrap();
        }

        let result = Db::open(&path_str);
        let err = match result {
            Ok(_) => panic!("legacy schema must be rejected"),
            Err(e) => e,
        };
        assert!(
            matches!(err, DatumError::IncompatibleSchema(_)),
            "expected IncompatibleSchema, got {err:?}",
        );

        std::fs::remove_file(&path_str).ok();
    }
}
