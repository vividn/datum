//! SQLite database layer for datum.
//!
//! All entry data, field specs, and aliases live in a single SQLite file.
//! The schema is designed to be CRDT-friendly (append-mostly, ULID-ordered).

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::entry::{Entry, Verb};
use crate::error::{DatumError, Result};
use crate::field_spec::{Alias, FieldSpec, SpecRegistry};
use crate::time::DatumTime;
use chrono::{DateTime, Utc};

/// A row from the `tracked_fields` table.
pub struct TrackedField {
    pub field: String,
    pub ignored: bool,
    pub color: Option<String>,
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
                created_at, modified_at, device_id
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14,
                ?15, ?16, ?17,
                ?18, ?19, ?20
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

    // -------------------------------------------------------------------------
    // Field specs and aliases
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

    pub fn save_field_spec(&self, spec: &FieldSpec) -> Result<()> {
        let json = serde_json::to_string(spec)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO field_specs (field, spec_json) VALUES (?1, ?2)",
            params![spec.field, json],
        )?;
        Ok(())
    }

    pub fn save_alias(&self, alias: &Alias) -> Result<()> {
        let json = serde_json::to_string(alias)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO aliases (name, alias_json) VALUES (?1, ?2)",
            params![alias.name, json],
        )?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Tracking (tracked_fields + field_state_colors)
    // -------------------------------------------------------------------------

    /// Insert or update a field's tracking state and optional color.
    /// When `color` is None, the stored color (if any) is preserved.
    pub fn set_tracked(&self, field: &str, ignored: bool, color: Option<&str>) -> Result<()> {
        if let Some(c) = color {
            self.conn.execute(
                "INSERT INTO tracked_fields (field, ignored, color) VALUES (?1, ?2, ?3)
                 ON CONFLICT(field) DO UPDATE SET ignored = excluded.ignored, color = excluded.color",
                params![field, ignored as i32, c],
            )?;
        } else {
            self.conn.execute(
                "INSERT INTO tracked_fields (field, ignored) VALUES (?1, ?2)
                 ON CONFLICT(field) DO UPDATE SET ignored = excluded.ignored",
                params![field, ignored as i32],
            )?;
        }
        Ok(())
    }

    /// Ensure a field is in tracked_fields with ignored=0. Does nothing if already present.
    pub fn ensure_tracked(&self, field: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO tracked_fields (field, ignored) VALUES (?1, 0)",
            params![field],
        )?;
        Ok(())
    }

    /// Return all tracked fields, ordered alphabetically.
    pub fn get_tracked_fields(&self) -> Result<Vec<TrackedField>> {
        let mut stmt = self.conn.prepare(
            "SELECT field, ignored, color FROM tracked_fields ORDER BY field",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TrackedField {
                field: row.get(0)?,
                ignored: row.get::<_, i32>(1)? != 0,
                color: row.get(2)?,
            })
        })?;
        rows.map(|r| r.map_err(DatumError::Db)).collect()
    }

    /// Store or replace a per-state color override.
    pub fn set_state_color(&self, field: &str, state: &str, color: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO field_state_colors (field, state, color) VALUES (?1, ?2, ?3)",
            params![field, state, color],
        )?;
        Ok(())
    }

    /// Return all state color overrides as a (field, state) → color map.
    pub fn get_all_state_colors(
        &self,
    ) -> Result<std::collections::HashMap<(String, String), String>> {
        let mut stmt = self.conn.prepare(
            "SELECT field, state, color FROM field_state_colors",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut map = std::collections::HashMap::new();
        for r in rows {
            let (field, state, color) = r.map_err(DatumError::Db)?;
            map.insert((field, state), color);
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
        // First call inserts with ignored=0
        db.ensure_tracked("coffee").unwrap();
        let fields = db.get_tracked_fields().unwrap();
        assert_eq!(fields.len(), 1);
        assert!(!fields[0].ignored);

        // Second call is a no-op (INSERT OR IGNORE)
        db.ensure_tracked("coffee").unwrap();
        let fields2 = db.get_tracked_fields().unwrap();
        assert_eq!(fields2.len(), 1);
    }

    #[test]
    fn test_ensure_tracked_does_not_override_ignored() {
        let db = Db::open_in_memory().unwrap();
        db.set_tracked("youtube", true, None).unwrap();
        // ensure_tracked should do nothing because youtube already exists
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

    #[test]
    fn test_latest_entry_for_now_skips_note() {
        let db = Db::open_in_memory().unwrap();
        let t1 = DatumTime::now();
        let switch_entry = Entry::new("mood", Verb::Switch, Some(serde_json::json!("happy")), None, None, Some(t1), None, "d1");
        db.insert_entry(&switch_entry).unwrap();
        // Insert a Note after the switch
        let t2 = DatumTime::now();
        let note_entry = Entry::new("mood", Verb::Note, Some(serde_json::json!("journal entry")), None, None, Some(t2), None, "d1");
        db.insert_entry(&note_entry).unwrap();

        let latest = db.latest_entry_for_now("mood").unwrap();
        assert!(latest.is_some());
        // Should return the switch entry, not the note
        assert_eq!(latest.unwrap().verb, Verb::Switch);
    }
}
