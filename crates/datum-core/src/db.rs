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

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::entry::{Entry, Verb};
use crate::error::{DatumError, Result};
use crate::field_spec::{Alias, FieldSpec, SpecRegistry};
use crate::time::DatumTime;
use chrono::{DateTime, Utc};

/// Projection of a FieldSpec for the `now` view. Built from FieldSpec on read.
pub struct TrackedField {
    pub field: String,
    pub ignored: bool,
    pub color: Option<String>,
}

/// Row reported by [`Db::find_time_end_before_start`].
#[derive(Debug, Clone)]
pub struct TimeOrderIssue {
    pub id: String,
    pub field: String,
    pub time_start_utc: String,
    pub time_end_utc: String,
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
        self.conn.execute_batch(include_str!("schema.sql"))?;
        // Idempotently add columns that were introduced after the initial
        // schema. Duplicate-column errors are expected once the column is
        // present and are ignored here.
        let _ = self
            .conn
            .execute("ALTER TABLE entries ADD COLUMN deleted_at TEXT", []);
        let _ = self
            .conn
            .execute("ALTER TABLE entries ADD COLUMN id_structure TEXT", []);
        let _ = self
            .conn
            .execute("ALTER TABLE entries ADD COLUMN field_structure TEXT", []);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Entry operations
    // -------------------------------------------------------------------------

    /// Insert an entry. Returns Err(Duplicate) if content_hash already exists.
    pub fn insert_entry(&self, entry: &Entry) -> Result<()> {
        let existing: Option<String> = self.conn
            .query_row(
                "SELECT id FROM entries WHERE content_hash = ?1",
                params![entry.content_hash],
                |row| row.get(0),
            )
            .optional()?;

        if existing.is_some() {
            return Err(DatumError::Duplicate(format!(
                "Entry with identical content already exists (hash: {})",
                &entry.content_hash[..12]
            )));
        }

        self.conn.execute(
            r#"INSERT INTO entries (
                id, slug, content_hash, field, verb,
                value, extra_data, unit,
                time_start_utc, time_start_offset, time_start_tz,
                time_end_utc, time_end_offset, time_end_tz,
                lat, lon, location,
                created_at, modified_at, device_id,
                id_structure, field_structure
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14,
                ?15, ?16, ?17,
                ?18, ?19, ?20,
                ?21, ?22
            )"#,
            params![
                entry.id,
                entry.slug,
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
                entry.id_structure,
                entry.field_structure,
            ],
        )?;
        Ok(())
    }

    /// Fetch an entry by id or slug.
    pub fn get_entry(&self, id_or_slug: &str) -> Result<Entry> {
        self.conn
            .query_row(
                "SELECT * FROM entries WHERE id = ?1 OR slug = ?1 LIMIT 1",
                params![id_or_slug],
                row_to_entry,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DatumError::NotFound(id_or_slug.to_string())
                }
                other => DatumError::Db(other),
            })
    }

    /// Query entries for a field, ordered by time_start descending.
    pub fn entries_for_field(&self, field: &str, limit: Option<usize>) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT * FROM entries WHERE field = ?1 ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) DESC{}",
            limit.map(|l| format!(" LIMIT {l}")).unwrap_or_default()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![field], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Query entries for a field, ordered by time_start ascending (oldest first).
    pub fn entries_for_field_asc(&self, field: &str, limit: Option<usize>) -> Result<Vec<Entry>> {
        let sql = format!(
            "SELECT * FROM entries WHERE field = ?1 ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) ASC{}",
            limit.map(|l| format!(" LIMIT {l}")).unwrap_or_default()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![field], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Get all entries within a UTC time range (inclusive).
    pub fn entries_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT * FROM entries
               WHERE COALESCE(time_start_utc, time_end_utc, created_at) >= ?1
                 AND COALESCE(time_start_utc, time_end_utc, created_at) <= ?2
               ORDER BY COALESCE(time_start_utc, time_end_utc, created_at) ASC"#,
        )?;
        let rows = stmt.query_map(
            params![start.to_rfc3339(), end.to_rfc3339()],
            row_to_entry,
        )?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Delete an entry by id or slug. Returns Err(NotFound) if no rows affected.
    pub fn delete_entry(&self, id_or_slug: &str) -> Result<()> {
        let affected = self.conn.execute(
            "DELETE FROM entries WHERE id = ?1 OR slug = ?1",
            params![id_or_slug],
        )?;
        if affected == 0 {
            Err(DatumError::NotFound(id_or_slug.to_string()))
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

    /// Soft-delete an entry by id or slug (A4 tombstone).
    ///
    /// Sets `deleted_at` to now; read paths that care about soft-delete (such
    /// as `get_active_state`) will ignore tombstoned rows. Errors if no row
    /// matches. If the row is already tombstoned, the tombstone is refreshed.
    pub fn soft_delete_entry(&self, id_or_slug: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let affected = self.conn.execute(
            "UPDATE entries SET deleted_at = ?1 WHERE id = ?2 OR slug = ?2",
            params![now, id_or_slug],
        )?;
        if affected == 0 {
            Err(DatumError::NotFound(id_or_slug.to_string()))
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

    // -------------------------------------------------------------------------
    // Consistency checks
    // -------------------------------------------------------------------------

    /// Rows where both time bounds exist and `time_end` precedes `time_start`.
    pub fn find_time_end_before_start(&self) -> Result<Vec<TimeOrderIssue>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT id, field, time_start_utc, time_end_utc FROM entries
               WHERE time_start_utc IS NOT NULL
                 AND time_end_utc IS NOT NULL
                 AND time_end_utc < time_start_utc
                 AND deleted_at IS NULL
               ORDER BY id ASC"#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TimeOrderIssue {
                id: row.get(0)?,
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
        let mut stmt = self.conn.prepare("SELECT * FROM entries ORDER BY id ASC")?;
        let rows = stmt.query_map([], row_to_entry)?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// IDs of rows with `deleted_at` set (tombstones).
    pub fn tombstone_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM entries WHERE deleted_at IS NOT NULL ORDER BY id ASC",
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

    /// Latest non-Note entry for a field, for use in the now view.
    pub fn latest_entry_for_now(&self, field: &str) -> Result<Option<Entry>> {
        self.conn
            .query_row(
                "SELECT * FROM entries WHERE field = ?1 AND verb != 'note'
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

    // id_structure / field_structure are added by ALTER TABLE migration; older
    // rows may not have them. Tolerate the absence by mapping the specific
    // "column not found" error to None.
    let id_structure: Option<String> = match row.get::<_, Option<String>>("id_structure") {
        Ok(v) => v,
        Err(rusqlite::Error::InvalidColumnName(_)) => None,
        Err(e) => return Err(e),
    };
    let field_structure: Option<String> = match row.get::<_, Option<String>>("field_structure") {
        Ok(v) => v,
        Err(rusqlite::Error::InvalidColumnName(_)) => None,
        Err(e) => return Err(e),
    };

    Ok(Entry {
        id: row.get("id")?,
        slug: row.get("slug")?,
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
        id_structure,
        field_structure,
    })
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
        let id = entry.id.clone();
        let slug = entry.slug.clone();

        db.insert_entry(&entry).unwrap();

        let fetched = db.get_entry(&id).unwrap();
        assert_eq!(fetched.field, "coffee");
        assert_eq!(fetched.unit, Some("cups".to_string()));

        let fetched_by_slug = db.get_entry(&slug).unwrap();
        assert_eq!(fetched_by_slug.id, id);
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
        let newer_id = newer.id.clone();
        db.insert_entry(&older).unwrap();
        db.insert_entry(&newer).unwrap();

        // Before tombstone: active at 13:00 is "angry"
        assert_eq!(
            db.get_active_state("mood", as_utc(13, 0)).unwrap(),
            serde_json::json!("angry"),
        );

        db.soft_delete_entry(&newer_id).unwrap();

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
}
