//! Kit-installable named SELECT views.
//!
//! Slice 4 (da-d4t.4) — every view is a named row in the `views` table
//! holding raw SQL text. Kit installs tag rows with `kit_id`; uninstall
//! drops only the rows it owns. Execution goes through
//! [`Db::execute_view`], which requires the SQL to be a single
//! SELECT/`WITH … SELECT` statement, flips `PRAGMA query_only` for
//! the duration of the query so a malicious or buggy view can't issue
//! DDL/DML against the entries DB, and interrupts the query if it
//! exceeds a wall-clock budget so a runaway view (unbounded recursive
//! CTE, cartesian join) can't hang datum (da-sky7). Kit installs
//! additionally screen the SQL up front via
//! `crate::kit::validate_view_entry` (da-w3xg).
//!
//! See `crate::kit::ViewEntry` for the TOML round-trip shape.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// Wall-clock budget for a single view execution (da-sky7). Generous for
/// any legitimate query over a personal-tracker-sized DB, while bounding
/// the damage a runaway view (unbounded recursive CTE, cartesian join)
/// can do — such SQL is entirely legal SELECT and passes every static
/// check in `crate::kit`.
const VIEW_TIME_BUDGET: Duration = Duration::from_secs(10);

/// VM instructions between progress-handler callbacks. Each callback is
/// one `Instant::now()` comparison, so at this interval the overhead on
/// legitimate queries is unmeasurable.
const PROGRESS_OPS_INTERVAL: std::os::raw::c_int = 10_000;

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
        // Install-time screen (da-w3xg): reject before any row persists.
        // execute_view re-checks for pre-existing or directly-written rows.
        for v in views {
            crate::kit::validate_view_entry(v)?;
        }

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

    /// Execute a view's SQL with read-only enforcement, in four layers
    /// (da-w3xg, da-sky7):
    ///
    /// 1. Statement-kind check: the SQL must start with `SELECT` or
    ///    `WITH`. `PRAGMA query_only` only blocks statements that change
    ///    database *files*, so connection-scoped statements (`PRAGMA
    ///    case_sensitive_like = ON`, `ATTACH …`) would otherwise run fine
    ///    and their effects would survive the fence.
    /// 2. `PRAGMA query_only = 1` around the query, so DDL/DML embedded
    ///    in an otherwise SELECT-shaped statement can't write.
    /// 3. `sqlite3_stmt_readonly` after prepare (in [`run_view_query`]),
    ///    which catches e.g. `WITH … DELETE` that passes the prefix check.
    /// 4. A wall-clock execution budget ([`VIEW_TIME_BUDGET`]) enforced
    ///    via SQLite's progress handler (da-sky7). The install-time
    ///    keyword blocklist can't catch resource exhaustion expressed in
    ///    entirely legal SELECT SQL — an unbounded `WITH RECURSIVE` or a
    ///    cartesian self-join passes every static check — so runaway
    ///    views are interrupted here instead of hanging datum.
    ///
    /// If clearing `query_only` fails the error is surfaced rather than
    /// swallowed — a wedged read-only connection would otherwise make
    /// every later `datum <log>` fail far from the cause.
    pub fn execute_view(&self, sql: &str) -> Result<ViewResult> {
        self.execute_view_with_budget(sql, VIEW_TIME_BUDGET)
    }

    /// [`Db::execute_view`] with an explicit time budget, split out so
    /// tests can exercise the interrupt path without waiting out the
    /// production budget.
    fn execute_view_with_budget(&self, sql: &str, budget: Duration) -> Result<ViewResult> {
        // Empty / whitespace-only queries would prepare into a no-op
        // statement that returns no columns; reject explicitly so the
        // caller can give the user a clear error.
        let trimmed = sql.trim();
        if trimmed.is_empty() {
            return Err(DatumError::Other("view SQL is empty".to_string()));
        }
        if !crate::kit::is_select_like(trimmed) {
            return Err(DatumError::Other(
                "views must be a single SELECT (or WITH … SELECT) statement".to_string(),
            ));
        }

        self.conn.execute_batch("PRAGMA query_only = 1;")?;
        // Returning true from the progress handler interrupts the query;
        // the flag distinguishes our deadline from a genuine SQL error.
        let timed_out = Arc::new(AtomicBool::new(false));
        {
            let timed_out = Arc::clone(&timed_out);
            let deadline = Instant::now() + budget;
            self.conn.progress_handler(
                PROGRESS_OPS_INTERVAL,
                Some(move || {
                    let expired = Instant::now() >= deadline;
                    if expired {
                        timed_out.store(true, Ordering::Relaxed);
                    }
                    expired
                }),
            );
        }
        let result = run_view_query(&self.conn, sql);
        // Remove the handler so the shared connection's normal work (the
        // logging hot path) doesn't keep paying the per-callback cost.
        self.conn.progress_handler(0, None::<fn() -> bool>);
        // Always clear query_only, even when the query failed, so a later
        // `datum log …` against the same connection can still write. If
        // the clear itself fails, say so loudly: the shared connection
        // may be stuck read-only.
        if let Err(restore_err) = self.conn.execute_batch("PRAGMA query_only = 0;") {
            return Err(DatumError::Other(match result {
                Ok(_) => format!(
                    "view executed, but clearing query_only failed; the connection may be stuck read-only: {restore_err}"
                ),
                Err(query_err) => format!(
                    "view failed ({query_err}); clearing query_only also failed; the connection may be stuck read-only: {restore_err}"
                ),
            }));
        }
        match result {
            Err(_) if timed_out.load(Ordering::Relaxed) => Err(DatumError::Other(format!(
                "view exceeded its execution budget ({budget:?}) and was interrupted — \
                 a runaway query (e.g. unbounded WITH RECURSIVE or cartesian join)?"
            ))),
            other => other,
        }
    }
}

/// The actual prepare + iterate. Split out so the caller can wrap the
/// PRAGMA toggle around it.
fn run_view_query(conn: &rusqlite::Connection, sql: &str) -> Result<ViewResult> {
    // rusqlite's plain `prepare` silently keeps any trailing statements
    // (`SELECT 1; DELETE …` prepares the SELECT and drops the rest), so
    // go through `Batch`, which exposes sqlite's own tail parsing, and
    // reject chained statements explicitly (da-w3xg).
    let mut batch = rusqlite::Batch::new(conn, sql);
    let Some(mut stmt) = batch.next().map_err(DatumError::Db)? else {
        return Err(DatumError::Other("view SQL is empty".to_string()));
    };
    if batch.next().map_err(DatumError::Db)?.is_some() {
        return Err(DatumError::Other(
            "views must be a single SELECT statement (no statement chaining)".to_string(),
        ));
    }
    // Belt-and-suspenders (da-w3xg): sqlite3_stmt_readonly catches writes
    // that slip past the SELECT/WITH prefix check, e.g. `WITH … DELETE`.
    if !stmt.readonly() {
        return Err(DatumError::Other(
            "views must be read-only: this statement would write to the database".to_string(),
        ));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn execute_view_runs_select_and_with() {
        let db = db();
        let r = db.execute_view("SELECT 1 AS v").unwrap();
        assert_eq!(r.rows, vec![vec![json!(1)]]);
        let r = db
            .execute_view("WITH x(v) AS (SELECT 2) SELECT v FROM x")
            .unwrap();
        assert_eq!(r.rows, vec![vec![json!(2)]]);
    }

    #[test]
    fn execute_view_rejects_multi_statement() {
        // rusqlite's `prepare` silently drops trailing statements, so the
        // fence rejects chaining itself (via `Batch` tail detection) —
        // pin that here.
        let db = db();
        let err = db
            .execute_view("SELECT 1; DELETE FROM entries")
            .unwrap_err()
            .to_string()
            .to_lowercase();
        assert!(
            err.contains("statement") || err.contains("extra"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn execute_view_rejects_connection_state_pollution() {
        // query_only doesn't block connection-scoped statements; the
        // statement-kind check must (covers rows written directly to the
        // views table, bypassing the install screen).
        let db = db();
        for sql in ["PRAGMA case_sensitive_like = ON", "ATTACH ':memory:' AS x"] {
            let err = db.execute_view(sql).unwrap_err().to_string();
            assert!(
                err.contains("single SELECT"),
                "sql {sql:?}: unexpected error {err}"
            );
        }
    }

    #[test]
    fn execute_view_rejects_with_prefixed_write() {
        // Passes the SELECT/WITH prefix check; sqlite3_stmt_readonly must
        // catch it.
        let db = db();
        let err = db
            .execute_view(
                "WITH x(v) AS (SELECT 1) DELETE FROM entries WHERE hid IN (SELECT v FROM x)",
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("read-only"), "unexpected error: {err}");
    }

    #[test]
    fn execute_view_clears_query_only_after_success_and_failure() {
        let db = db();
        db.execute_view("SELECT 1").unwrap();
        // A write on the same connection must succeed after the fence.
        db.conn
            .execute(
                "INSERT INTO views (name, sql_text) VALUES ('after_ok', 'SELECT 1')",
                [],
            )
            .unwrap();

        db.execute_view("SELECT no_such_column FROM entries")
            .unwrap_err();
        db.conn
            .execute(
                "INSERT INTO views (name, sql_text) VALUES ('after_err', 'SELECT 1')",
                [],
            )
            .unwrap();
    }

    #[test]
    fn execute_view_interrupts_runaway_query() {
        // An unbounded recursive CTE is entirely legal SELECT SQL — it
        // passes every static check in `crate::kit` — so the runtime
        // budget is the only thing standing between it and a hang
        // (da-sky7). Tiny budget so the test is fast.
        let db = db();
        let err = db
            .execute_view_with_budget(
                "WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r) \
                 SELECT n FROM r",
                Duration::from_millis(20),
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("execution budget"), "unexpected error: {err}");

        // The interrupt path must still clear query_only and remove the
        // progress handler — the shared connection's normal work (the
        // logging hot path) continues afterwards.
        db.conn
            .execute(
                "INSERT INTO views (name, sql_text) VALUES ('after_interrupt', 'SELECT 1')",
                [],
            )
            .unwrap();
        let r = db.execute_view("SELECT 1 AS v").unwrap();
        assert_eq!(r.rows, vec![vec![json!(1)]]);
    }

    #[test]
    fn execute_view_budget_spares_legitimate_queries() {
        // A genuine SQL error inside the budget must surface as-is, not
        // be misreported as a timeout.
        let db = db();
        let err = db
            .execute_view_with_budget("SELECT no_such_column FROM entries", VIEW_TIME_BUDGET)
            .unwrap_err()
            .to_string();
        assert!(!err.contains("execution budget"), "unexpected error: {err}");
    }

    #[test]
    fn install_kit_views_screens_sql() {
        let db = db();
        let err = db
            .install_kit_views(
                "test",
                &[ViewEntry {
                    name: "evil".into(),
                    sql: "PRAGMA case_sensitive_like = ON".into(),
                }],
                false,
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("single SELECT"), "unexpected error: {err}");
        assert!(db.get_view("evil").unwrap().is_none());
    }
}
