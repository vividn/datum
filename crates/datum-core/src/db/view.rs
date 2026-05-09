//! Kit-installable named SELECT views.
//!
//! Slice 4 (da-d4t.4) — every view is a named row in the `views` table
//! holding raw SQL text. Kit installs tag rows with `kit_id`; uninstall
//! drops only the rows it owns. Execution goes through
//! [`Db::execute_view`], which flips `PRAGMA query_only` for the duration
//! of the query so a malicious or buggy SELECT can't issue DDL/DML
//! against the entries DB.
//!
//! See `crate::kit::ViewEntry` for the TOML round-trip shape.

use rusqlite::{params, types::ValueRef, OptionalExtension};
use serde_json::{Number, Value as JsonValue};

use crate::error::{DatumError, Result};
use crate::kit::ViewEntry;

use super::Db;

/// One row of [`Db::list_views`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledView {
    pub name: String,
    pub kit_id: Option<String>,
    pub sql_text: String,
}

/// Per-view outcome reported by [`Db::install_kit_views`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitViewOutcome {
    /// View newly installed (or overwritten under `force`).
    Installed,
    /// A row for this view name already existed and the kit's view was
    /// dropped silently; the existing row remains the source of truth.
    SkippedExisting { existing_kit: Option<String> },
}

/// Aggregate report from [`Db::install_kit_views`].
#[derive(Debug, Clone, Default)]
pub struct InstallViewsReport {
    pub outcomes: Vec<(String, KitViewOutcome)>,
}

impl InstallViewsReport {
    pub fn installed(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter(|&(_name, o)| matches!(o, KitViewOutcome::Installed))
            .map(|(name, _o)| name.as_str())
    }

    pub fn skipped(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter(|&(_name, o)| matches!(o, KitViewOutcome::SkippedExisting { .. }))
            .map(|(name, _o)| name.as_str())
    }
}

/// Result of executing a view: column names plus rows of JSON-typed cells.
/// Cell types follow SQLite's storage class: TEXT/REAL/INTEGER/NULL map
/// to their natural JSON shape; BLOB renders as a hex-prefixed string so
/// it survives the JSON round-trip.
#[derive(Debug, Clone, Default)]
pub struct ViewResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<JsonValue>>,
}

impl Db {
    /// Install a kit's views. `kit_id` tags every persisted row;
    /// callers should pass the kit's `[kit].name` from the TOML.
    ///
    /// Conflict policy mirrors [`Db::install_kit`]:
    /// - `force = false` → leave any existing row, record `SkippedExisting`.
    /// - `force = true`  → overwrite the row's sql_text and re-tag its
    ///   `kit_id` to the new owner.
    pub fn install_kit_views(
        &self,
        kit_id: &str,
        views: &[ViewEntry],
        force: bool,
    ) -> Result<InstallViewsReport> {
        let tx = self.conn.unchecked_transaction()?;
        let mut report = InstallViewsReport::default();

        for v in views {
            let existing_kit: Option<Option<String>> = tx
                .query_row(
                    "SELECT kit_id FROM views WHERE name = ?1",
                    params![v.name],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?;

            if let Some(existing) = existing_kit {
                if !force {
                    report.outcomes.push((
                        v.name.clone(),
                        KitViewOutcome::SkippedExisting {
                            existing_kit: existing,
                        },
                    ));
                    continue;
                }
            }

            tx.execute(
                "INSERT INTO views (name, sql_text, kit_id) VALUES (?1, ?2, ?3)
                 ON CONFLICT(name) DO UPDATE SET sql_text = excluded.sql_text,
                                                 kit_id   = excluded.kit_id",
                params![v.name, v.sql, kit_id],
            )?;

            report
                .outcomes
                .push((v.name.clone(), KitViewOutcome::Installed));
        }

        tx.commit()?;
        Ok(report)
    }

    /// Drop every view tagged with this `kit_id`. User-declared rows
    /// (`kit_id IS NULL`) are untouched. Returns the names removed.
    pub fn uninstall_kit_views(&self, kit_id: &str) -> Result<Vec<String>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = tx.prepare("SELECT name FROM views WHERE kit_id = ?1 ORDER BY name")?;
        let names: Vec<String> = stmt
            .query_map(params![kit_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatumError::Db)?;
        drop(stmt);
        tx.execute("DELETE FROM views WHERE kit_id = ?1", params![kit_id])?;
        tx.commit()?;
        Ok(names)
    }

    /// Load every view tagged with `kit_id` for `kit export`.
    pub fn load_views_by_kit(&self, kit_id: &str) -> Result<Vec<ViewEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, sql_text FROM views WHERE kit_id = ?1 ORDER BY name")?;
        let rows = stmt.query_map(params![kit_id], |row| {
            Ok(ViewEntry {
                name: row.get::<_, String>(0)?,
                sql: row.get::<_, String>(1)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(DatumError::Db)?);
        }
        Ok(out)
    }

    /// Look up a single view by name. Returns the full row including the
    /// owning kit (if any).
    pub fn get_view(&self, name: &str) -> Result<Option<InstalledView>> {
        let row = self
            .conn
            .query_row(
                "SELECT name, kit_id, sql_text FROM views WHERE name = ?1",
                params![name],
                |row| {
                    Ok(InstalledView {
                        name: row.get::<_, String>(0)?,
                        kit_id: row.get::<_, Option<String>>(1)?,
                        sql_text: row.get::<_, String>(2)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// List every view in name order.
    pub fn list_views(&self) -> Result<Vec<InstalledView>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, kit_id, sql_text FROM views ORDER BY name")?;
        let rows = stmt.query_map([], |row| {
            Ok(InstalledView {
                name: row.get::<_, String>(0)?,
                kit_id: row.get::<_, Option<String>>(1)?,
                sql_text: row.get::<_, String>(2)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(DatumError::Db)?);
        }
        Ok(out)
    }

    /// Execute a view's SQL with read-only enforcement.
    ///
    /// SQLite's `PRAGMA query_only = 1` rejects every DDL/DML statement on
    /// the connection while it's set. We toggle it on for the query and
    /// off afterward (best-effort on error paths so we don't leave the
    /// connection wedged in read-only mode).
    pub fn execute_view(&self, sql: &str) -> Result<ViewResult> {
        // Empty / whitespace-only queries would prepare into a no-op
        // statement that returns no columns; reject explicitly so the
        // caller can give the user a clear error.
        if sql.trim().is_empty() {
            return Err(DatumError::Other("view SQL is empty".to_string()));
        }

        self.conn.execute_batch("PRAGMA query_only = 1;")?;
        let result = run_view_query(&self.conn, sql);
        // Always clear query_only, even when the query failed, so a later
        // `datum log …` against the same connection can still write.
        let _ = self.conn.execute_batch("PRAGMA query_only = 0;");
        result
    }
}

/// The actual prepare + iterate. Split out so the caller can wrap the
/// PRAGMA toggle around it.
fn run_view_query(conn: &rusqlite::Connection, sql: &str) -> Result<ViewResult> {
    let mut stmt = conn.prepare(sql).map_err(DatumError::Db)?;
    let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let col_count = columns.len();
    let mut out_rows: Vec<Vec<JsonValue>> = Vec::new();

    let mut rows = stmt.query([]).map_err(DatumError::Db)?;
    while let Some(row) = rows.next().map_err(DatumError::Db)? {
        let mut cells: Vec<JsonValue> = Vec::with_capacity(col_count);
        for i in 0..col_count {
            let v = row.get_ref(i).map_err(DatumError::Db)?;
            cells.push(value_ref_to_json(v));
        }
        out_rows.push(cells);
    }

    Ok(ViewResult {
        columns,
        rows: out_rows,
    })
}

fn value_ref_to_json(v: ValueRef<'_>) -> JsonValue {
    match v {
        ValueRef::Null => JsonValue::Null,
        ValueRef::Integer(i) => JsonValue::Number(Number::from(i)),
        ValueRef::Real(f) => Number::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        ValueRef::Text(bytes) => match std::str::from_utf8(bytes) {
            Ok(s) => JsonValue::String(s.to_string()),
            Err(_) => JsonValue::String(format!("0x{}", hex_encode(bytes))),
        },
        ValueRef::Blob(bytes) => JsonValue::String(format!("0x{}", hex_encode(bytes))),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
