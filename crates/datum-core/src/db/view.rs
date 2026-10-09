//! Kit-installable named SELECT views.
//!
//! Slice 4 (da-d4t.4) — every view is named raw SQL text, kept since
//! da-fwwtn as the entry `view:<name>` (`definition.rs`). Kit installs mark
//! theirs with `kit_id`; uninstall removes only its own. Execution goes through
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

use chrono::{DateTime, Utc};
use rusqlite::functions::FunctionFlags;
use rusqlite::types::ValueRef;
use serde_json::{Map, Number, Value as JsonValue};

use crate::dur::Dur;
use crate::error::{DatumError, Result};
use crate::kit::ViewEntry;
use crate::time::DatumTime;

use super::definition::{definition_data, KIT_KEY, VIEW_FIELD};
use super::Db;

/// One row of [`Db::list_views`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledView {
    pub name: String,
    pub kit_id: Option<String>,
    pub sql_text: String,
    /// The views it reads by name (da-3mzyu).
    pub uses: Vec<String>,
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
    /// Install a kit's views, each the entry `view:<name>` marked with
    /// `kit_id` (the kit's `[kit].name`) — da-fwwtn, `definition.rs`.
    ///
    /// Conflict policy mirrors [`Db::install_kit`]:
    /// - `force = false` → leave any existing view, record `SkippedExisting`.
    /// - `force = true`  → overwrite it and re-mark it with this kit.
    pub fn install_kit_views(
        &self,
        kit_id: &str,
        views: &[ViewEntry],
        force: bool,
    ) -> Result<InstallViewsReport> {
        self.check_kit_views(views)?;
        let defs = views
            .iter()
            .map(|v| Ok((v.name.clone(), definition_data(v, None)?)))
            .collect::<Result<Vec<_>>>()?;
        let outcomes = self.install_definitions(VIEW_FIELD, kit_id, defs, force)?;
        Ok(InstallViewsReport {
            outcomes: outcomes
                .into_iter()
                .map(|(name, skipped)| {
                    let outcome = match skipped {
                        None => KitViewOutcome::Installed,
                        Some(existing_kit) => KitViewOutcome::SkippedExisting { existing_kit },
                    };
                    (name, outcome)
                })
                .collect(),
        })
    }

    /// Install-time screen of a kit's views (da-w3xg), writing nothing, so
    /// `kit install` runs it before its first write and `--dry-run` runs it
    /// at all (da-45amw). execute_view re-checks every run, so a synced
    /// view is held too.
    pub fn check_kit_views(&self, views: &[ViewEntry]) -> Result<()> {
        for v in views {
            crate::kit::validate_view_entry(v)?;
        }
        // What each uses must resolve — to this kit's views first, then the
        // installed ones — without a cycle or shadowing a table (da-3mzyu).
        let lookup = |n: &str| -> Result<Option<(String, Vec<String>)>> {
            match views.iter().find(|v| v.name == n) {
                Some(v) => Ok(Some((v.sql.clone(), v.uses.clone()))),
                None => Ok(self.get_view(n)?.map(|v| (v.sql_text, v.uses))),
            }
        };
        for v in views {
            inline_uses(&v.name, &v.sql, &v.uses, &lookup, &|n| self.is_table(n))?;
        }
        Ok(())
    }

    /// Delete (softly) every view marked with this `kit_id`. The person's
    /// own are untouched. Returns the names removed.
    pub fn uninstall_kit_views(&self, kit_id: &str) -> Result<Vec<String>> {
        self.uninstall_definitions(VIEW_FIELD, kit_id)
    }

    /// Every view marked with `kit_id`, for `kit export`.
    pub fn load_views_by_kit(&self, kit_id: &str) -> Result<Vec<ViewEntry>> {
        self.views(Some(kit_id))?
            .into_iter()
            .map(|v| {
                Ok(ViewEntry {
                    name: v.name,
                    sql: v.sql_text,
                    uses: v.uses,
                })
            })
            .collect()
    }

    /// Look up a single view by name, with the kit that shipped it.
    pub fn get_view(&self, name: &str) -> Result<Option<InstalledView>> {
        let slug = format!("{VIEW_FIELD}:{name}");
        match self.definition_entry(&slug)?.and_then(|e| e.data) {
            Some(data) => Ok(Some(installed_view(data)?)),
            None => Ok(None),
        }
    }

    /// List every view in name order.
    pub fn list_views(&self) -> Result<Vec<InstalledView>> {
        self.views(None)
    }

    /// The views, less any whose entry is not one (edited or synced past
    /// the write check), which would otherwise hide every other.
    fn views(&self, kit_id: Option<&str>) -> Result<Vec<InstalledView>> {
        Ok(self
            .definitions(VIEW_FIELD, kit_id)?
            .into_iter()
            .filter_map(|data| installed_view(data).ok())
            .collect())
    }

    /// Run an installed view, every view it `uses` inlined ahead of its SQL
    /// as a CTE (da-3mzyu) — still one SELECT, so [`Db::execute_view`]'s
    /// fence holds. Every runner of a named view comes through here.
    ///
    /// `params` are bound as SQLite named parameters (`:key`), never spliced
    /// into the text (da-7ramf): one the SQL does not name is ignored, and a
    /// parameter it names that `params` lacks is NULL.
    pub fn run_view(
        &self,
        view: &InstalledView,
        params: &Map<String, JsonValue>,
    ) -> Result<ViewResult> {
        let lookup = |n: &str| Ok(self.get_view(n)?.map(|v| (v.sql_text, v.uses)));
        let sql = inline_uses(&view.name, &view.sql_text, &view.uses, &lookup, &|n| {
            self.is_table(n)
        })?;
        self.execute_view_with_budget(&sql, params, VIEW_TIME_BUDGET)
    }

    /// Whether `name` is a table or SQLite view a CTE of that name would
    /// shadow (SQLite resolves names case-insensitively).
    fn is_table(&self, name: &str) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master \
             WHERE type IN ('table', 'view') AND name = ?1 COLLATE NOCASE)",
            [name],
            |r| r.get(0),
        )?)
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
        self.execute_view_with_budget(sql, &Map::new(), VIEW_TIME_BUDGET)
    }

    /// [`Db::execute_view`] with an explicit time budget, split out so
    /// tests can exercise the interrupt path without waiting out the
    /// production budget.
    fn execute_view_with_budget(
        &self,
        sql: &str,
        params: &Map<String, JsonValue>,
        budget: Duration,
    ) -> Result<ViewResult> {
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

        register_local_day(&self.conn)?;
        register_local_today(&self.conn)?;
        register_dur_seconds(&self.conn)?;
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
        let result = run_view_query(&self.conn, sql, params);
        // Remove the handler so the shared connection's normal work (the
        // logging hot path) doesn't keep paying the per-callback cost.
        self.conn.progress_handler(0, None::<fn() -> bool>);
        // `local_day`, `local_today` and `dur_seconds` are the views' alone,
        // never a writer's. A failed removal leaves only a pure read-only
        // function behind.
        let _ = self.conn.remove_function(LOCAL_DAY, 2);
        let _ = self.conn.remove_function(LOCAL_TODAY, 0);
        let _ = self.conn.remove_function(DUR_SECONDS, 1);
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

const LOCAL_DAY: &str = "local_day";
const LOCAL_TODAY: &str = "local_today";
const DUR_SECONDS: &str = "dur_seconds";

/// `dur_seconds(dur)` → the seconds of an entry's duration, as `tail`'s
/// `dur_seconds` reads it (`Dur::as_seconds`): the column is ISO text, which
/// SQL cannot add up. Null for null, `MOMENT`, `UNKNOWN` and anything that
/// is no duration, so `SUM` passes over them (da-es4em).
fn register_dur_seconds(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        DUR_SECONDS,
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let dur = ctx.get_raw(0).as_str().ok().and_then(Dur::parse);
            Ok(dur.and_then(|d| d.as_seconds()))
        },
    )
}

/// `local_today()` → `YYYY-MM-DD`, today in the zone datum stamps entries
/// with (`DatumTime::now()`), read once per view. SQLite's own
/// `date('now', 'localtime')` is the UTC day on datum.day: wasi-libc's
/// `localtime` has no zone data (da-5fonh).
fn register_local_today(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    let now = DatumTime::now();
    let today = crate::views::duration_sum::local_date_for(&now.utc, now.offset_minutes);
    conn.create_scalar_function(LOCAL_TODAY, 0, FunctionFlags::SQLITE_UTF8, move |_| {
        Ok(today.clone())
    })
}

/// `local_day(occur_time, occur_time_offset)` → `YYYY-MM-DD`, the day in the
/// offset the entry was logged with (da-tgsqs), as `duration_sum` buckets it
/// and `stats` reads it: SQLite has no zone data, so `date(occur_time)` is
/// the UTC day. A null offset is UTC; a timeless entry gives null.
fn register_local_day(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        LOCAL_DAY,
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let Some(occur) = ctx.get::<Option<String>>(0)? else {
                return Ok(None);
            };
            let utc: DateTime<Utc> = occur
                .parse()
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
            let offset = ctx.get::<Option<i32>>(1)?.unwrap_or(0);
            Ok(Some(crate::views::duration_sum::local_date_for(
                &utc, offset,
            )))
        },
    )
}

type ViewLookup<'a> = dyn Fn(&str) -> Result<Option<(String, Vec<String>)>> + 'a;

/// `sql` with the views it `uses`, transitively and each once, inlined
/// ahead of it as CTEs in dependency order: A uses B uses C →
/// `WITH C AS (…), B AS (…) <A>`. A view that opens with its own `WITH
/// [RECURSIVE]` keeps its CTEs after them in the one list. Refuses a name
/// that is no view, a cycle, and a name that would shadow a table.
fn inline_uses(
    name: &str,
    sql: &str,
    uses: &[String],
    lookup: &ViewLookup,
    is_table: &dyn Fn(&str) -> Result<bool>,
) -> Result<String> {
    let mut ctes = Vec::new();
    add_uses(
        uses,
        lookup,
        is_table,
        &mut vec![name.to_string()],
        &mut ctes,
    )?;
    if ctes.is_empty() {
        return Ok(sql.to_string());
    }
    let list = ctes
        .iter()
        .map(|(n, body)| format!("\"{}\" AS (\n{body}\n)", n.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",\n");
    let sql = sql.trim();
    if !crate::kit::starts_with_keyword(sql, "WITH") {
        return Ok(format!("WITH {list}\n{sql}"));
    }
    let rest = sql[4..].trim_start();
    Ok(if crate::kit::starts_with_keyword(rest, "RECURSIVE") {
        format!("WITH RECURSIVE {list},\n{}", rest[9..].trim_start())
    } else {
        format!("WITH {list},\n{rest}")
    })
}

/// Depth-first over `uses`, pushing each view after the ones it uses.
/// `path` is the chain being resolved, for the cycle check.
fn add_uses(
    uses: &[String],
    lookup: &ViewLookup,
    is_table: &dyn Fn(&str) -> Result<bool>,
    path: &mut Vec<String>,
    ctes: &mut Vec<(String, String)>,
) -> Result<()> {
    let user = path.last().cloned().unwrap_or_default();
    for used in uses {
        let refuse = |why: String| {
            Err(DatumError::Other(format!(
                "view '{user}' uses '{used}': {why}"
            )))
        };
        if path.contains(used) {
            return refuse(format!("a cycle ({} → {used})", path.join(" → ")));
        }
        if ctes.iter().any(|(n, _)| n == used) {
            continue;
        }
        if is_table(used)? {
            return refuse("that name is a table, which it would shadow".into());
        }
        let Some((sql, next)) = lookup(used)? else {
            return refuse("no view has that name".into());
        };
        path.push(used.clone());
        add_uses(&next, lookup, is_table, path, ctes)?;
        path.pop();
        ctes.push((used.clone(), sql));
    }
    Ok(())
}

/// A view entry's data as an [`InstalledView`].
fn installed_view(data: JsonValue) -> Result<InstalledView> {
    let kit_id = data[KIT_KEY].as_str().map(str::to_string);
    let view: ViewEntry = serde_json::from_value(data)?;
    Ok(InstalledView {
        name: view.name,
        kit_id,
        sql_text: view.sql,
        uses: view.uses,
    })
}

/// The actual prepare + iterate. Split out so the caller can wrap the
/// PRAGMA toggle around it.
fn run_view_query(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &Map<String, JsonValue>,
) -> Result<ViewResult> {
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

    for (name, value) in params {
        if let Some(i) = stmt.parameter_index(&format!(":{name}"))? {
            stmt.raw_bind_parameter(i, json_to_sql(value))?;
        }
    }
    let mut rows = stmt.raw_query();
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

/// A bound argument as SQLite holds it: a boolean is 0 or 1, as SQLite's
/// own `true`/`false` are, and an array or object its JSON text.
fn json_to_sql(v: &JsonValue) -> rusqlite::types::Value {
    use rusqlite::types::Value;
    match v {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Integer(i64::from(*b)),
        JsonValue::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Real(n.as_f64().unwrap_or(f64::NAN)),
        },
        JsonValue::String(s) => Value::Text(s.clone()),
        other => Value::Text(other.to_string()),
    }
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

    fn view(name: &str) -> ViewEntry {
        ViewEntry {
            name: name.into(),
            sql: "SELECT 1".into(),
            uses: vec![],
        }
    }

    /// A view is the entry `view:<name>`: a re-install under `force` is a
    /// new version of it, an uninstall a tombstone, and only the kit's own
    /// go.
    #[test]
    fn a_view_is_an_entry() {
        let db = db();
        db.install_kit_views("k", &[view("v")], false).unwrap();
        let row = db.get_entry("view:v").unwrap();
        assert_eq!(row.field, "view");
        assert_eq!(
            row.data.unwrap(),
            json!({"name": "v", "sql": "SELECT 1", "kit_id": "k"})
        );

        let other = ViewEntry {
            sql: "SELECT 2".into(),
            ..view("v")
        };
        let report = db
            .install_kit_views("j", std::slice::from_ref(&other), false)
            .unwrap();
        assert_eq!(report.skipped().collect::<Vec<_>>(), ["v"]);
        db.install_kit_views("j", &[other], true).unwrap();
        let got = db.get_view("v").unwrap().unwrap();
        assert_eq!(
            (got.kit_id.as_deref(), got.sql_text.as_str()),
            (Some("j"), "SELECT 2")
        );

        assert!(db.uninstall_kit_views("k").unwrap().is_empty());
        assert_eq!(db.uninstall_kit_views("j").unwrap(), ["v"]);
        assert!(db.get_view("v").unwrap().is_none());
    }

    /// A plain log of the reserved field, or a view at the wrong slug, is
    /// refused.
    #[test]
    fn a_write_under_the_view_field_is_validated_as_a_view() {
        let db = db();
        let plain = crate::test_util::entry_builder("view").build();
        let err = db.insert_entry(&plain).unwrap_err().to_string();
        assert!(err.contains("reserved for kit views"), "{err}");

        db.install_kit_views("k", &[view("v")], false).unwrap();
        let mut row = db.get_entry("view:v").unwrap();
        row.data = Some(json!({"name": "v"}));
        let err = db.save_existing_entry(&row).unwrap_err().to_string();
        assert!(err.contains("not a valid view"), "{err}");
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

    /// `local_day` is the day in the entry's own offset, UTC without one,
    /// and null for a timeless entry — and it exists only while a view
    /// runs, so nothing that writes can call it (da-tgsqs).
    #[test]
    fn local_day_is_a_views_alone() {
        let db = db();
        let r = db
            .execute_view(
                "SELECT local_day('2026-09-09T22:30:00Z', 120), \
                        local_day('2026-09-09T22:30:00Z', NULL), \
                        local_day(NULL, 120)",
            )
            .unwrap();
        assert_eq!(
            r.rows,
            vec![vec![json!("2026-09-10"), json!("2026-09-09"), json!(null)]]
        );

        let err = db
            .conn()
            .query_row("SELECT local_day('2026-09-09T22:30:00Z', 120)", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap_err()
            .to_string();
        assert!(err.contains("no such function"), "{err}");
    }

    /// `local_today` is the clock's day in the clock's zone, not the UTC
    /// day: 22:30Z in Berlin is already tomorrow (da-5fonh).
    #[test]
    fn local_today_is_the_clocks_day_in_its_zone() {
        use datum_parse::time::clock::{self, MockClock};
        let db = db();
        let at =
            DatumTime::from_utc_and_tz("2026-09-09T22:30:00Z".parse().unwrap(), "Europe/Berlin")
                .unwrap();
        clock::install(Arc::new(MockClock::new(at)));
        let r = db.execute_view("SELECT local_today()");
        clock::uninstall();
        assert_eq!(r.unwrap().rows, vec![vec![json!("2026-09-10")]]);

        let err = db
            .conn()
            .query_row("SELECT local_today()", [], |r| r.get::<_, String>(0))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no such function"), "{err}");
    }

    /// `dur_seconds` is the seconds of a stored duration and null for
    /// everything that has none, so a `SUM` over a field's entries totals
    /// the time spent (da-es4em).
    #[test]
    fn dur_seconds_totals_the_durations_and_passes_over_the_rest() {
        let db = db();
        let r = db
            .execute_view(
                "SELECT dur_seconds('PT1H30M'), dur_seconds('PT0S'), dur_seconds('MOMENT'), \
                        dur_seconds('UNKNOWN'), dur_seconds(NULL), dur_seconds('soon'), \
                        dur_seconds(90), \
                        (SELECT SUM(dur_seconds(column1)) \
                         FROM (VALUES ('PT30M'), ('PT1H30M'), ('MOMENT')))",
            )
            .unwrap();
        let null = json!(null);
        assert_eq!(
            r.rows,
            vec![vec![
                json!(5400),
                json!(0),
                null.clone(),
                null.clone(),
                null.clone(),
                null.clone(),
                null,
                json!(7200)
            ]]
        );

        let err = db
            .conn()
            .query_row("SELECT dur_seconds('PT30M')", [], |r| r.get::<_, i64>(0))
            .unwrap_err()
            .to_string();
        assert!(err.contains("no such function"), "{err}");
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
        // statement-kind check must (covers a view entry edited or synced
        // past the install screen).
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
        db.install_kit_views("t", &[view("after_ok")], false)
            .unwrap();

        db.execute_view("SELECT no_such_column FROM entries")
            .unwrap_err();
        db.install_kit_views("t", &[view("after_err")], false)
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
                &Map::new(),
                Duration::from_millis(20),
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("execution budget"), "unexpected error: {err}");

        // The interrupt path must still clear query_only and remove the
        // progress handler — the shared connection's normal work (the
        // logging hot path) continues afterwards.
        db.install_kit_views("t", &[view("after_interrupt")], false)
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
            .execute_view_with_budget(
                "SELECT no_such_column FROM entries",
                &Map::new(),
                VIEW_TIME_BUDGET,
            )
            .unwrap_err()
            .to_string();
        assert!(!err.contains("execution budget"), "unexpected error: {err}");
    }

    fn using(name: &str, sql: &str, uses: &[&str]) -> ViewEntry {
        ViewEntry {
            name: name.into(),
            sql: sql.into(),
            uses: uses.iter().map(|u| u.to_string()).collect(),
        }
    }

    fn run(db: &Db, name: &str) -> Result<ViewResult> {
        db.run_view(&db.get_view(name).unwrap().unwrap(), &Map::new())
    }

    fn ints(r: &ViewResult) -> Vec<i64> {
        r.rows.iter().map(|row| row[0].as_i64().unwrap()).collect()
    }

    /// A view reads another by name once it says it `uses` it; the
    /// control is the same SQL run alone (da-3mzyu).
    #[test]
    fn a_view_reads_the_views_it_uses() {
        let db = db();
        let base = using("base", "SELECT 1 AS v UNION ALL SELECT 2", &[]);
        let top = using("top", "SELECT v FROM base WHERE v > 1", &["base"]);
        db.install_kit_views("k", &[base, top.clone()], false)
            .unwrap();
        assert_eq!(ints(&run(&db, "top").unwrap()), [2]);

        let err = db.execute_view(&top.sql).unwrap_err().to_string();
        assert!(err.contains("no such table: base"), "{err}");
    }

    /// A uses B uses C: C first, then B, each once however often it is
    /// reached.
    #[test]
    fn uses_nest_in_dependency_order() {
        let db = db();
        db.install_kit_views(
            "k",
            &[
                using(
                    "a",
                    "SELECT v * 10 + (SELECT v FROM c) AS v FROM b",
                    &["b", "c"],
                ),
                using("b", "SELECT v + 1 AS v FROM c", &["c"]),
                using("c", "SELECT 1 AS v", &[]),
            ],
            false,
        )
        .unwrap();
        assert_eq!(ints(&run(&db, "a").unwrap()), [21]);

        let lookup = |n: &str| Ok(db.get_view(n)?.map(|v| (v.sql_text, v.uses)));
        let sql = inline_uses("a", "SELECT 1", &["b".into(), "c".into()], &lookup, &|_| {
            Ok(false)
        })
        .unwrap();
        let (c, b) = (sql.find("\"c\" AS").unwrap(), sql.find("\"b\" AS").unwrap());
        assert!(c < b, "{sql}");
        assert_eq!(sql.matches("\"c\" AS").count(), 1, "{sql}");
    }

    /// A view with its own `WITH` or `WITH RECURSIVE` keeps its CTEs in the
    /// one list, after the used ones, with `RECURSIVE` at the front.
    #[test]
    fn a_views_own_with_is_merged() {
        let db = db();
        db.install_kit_views(
            "k",
            &[
                using("base", "SELECT 1 AS v", &[]),
                using(
                    "own",
                    "WITH x AS (SELECT v + 1 AS v FROM base) SELECT v FROM x",
                    &["base"],
                ),
                using(
                    "rec",
                    "with recursive r(n) AS (SELECT v FROM base UNION ALL \
                     SELECT n + 1 FROM r WHERE n < 3) SELECT n FROM r",
                    &["base"],
                ),
            ],
            false,
        )
        .unwrap();
        assert_eq!(ints(&run(&db, "own").unwrap()), [2]);
        assert_eq!(ints(&run(&db, "rec").unwrap()), [1, 2, 3]);

        let rec = db.get_view("rec").unwrap().unwrap();
        let lookup = |n: &str| Ok(db.get_view(n)?.map(|v| (v.sql_text, v.uses)));
        let sql = inline_uses("rec", &rec.sql_text, &rec.uses, &lookup, &|_| Ok(false)).unwrap();
        assert!(sql.starts_with("WITH RECURSIVE \"base\" AS"), "{sql}");
        assert!(sql.contains("),\nr(n) AS"), "{sql}");
    }

    /// Install refuses a name that is no view, a cycle, and a name that
    /// would shadow a table — and persists none of the kit's views.
    #[test]
    fn install_refuses_unknown_cyclic_and_shadowing_uses() {
        let db = db();
        for (views, why) in [
            (
                vec![using("a", "SELECT 1", &["nope"])],
                "no view has that name",
            ),
            (
                vec![
                    using("a", "SELECT 1", &["b"]),
                    using("b", "SELECT 1", &["a"]),
                ],
                "a cycle (a → b → a)",
            ),
            (vec![using("a", "SELECT 1", &["a"])], "a cycle (a → a)"),
            (vec![using("a", "SELECT 1", &["Entries"])], "is a table"),
            (
                vec![using("a", "SELECT 1", &["entry_history"])],
                "is a table",
            ),
        ] {
            let err = db
                .install_kit_views("k", &views, false)
                .unwrap_err()
                .to_string();
            assert!(err.contains(why), "{why}: {err}");
            assert!(db.get_view("a").unwrap().is_none(), "{why}");
        }

        // An installed view is found too.
        db.install_kit_views("k", &[view("other")], false).unwrap();
        db.install_kit_views("j", &[using("a", "SELECT * FROM other", &["other"])], false)
            .unwrap();
    }

    /// A USED view edited past the install screen to write, or to break
    /// out of its CTE, is held by the fence; nothing is written.
    #[test]
    fn the_fence_holds_for_a_used_view() {
        let db = db();
        db.install_kit_views(
            "k",
            &[view("base"), using("top", "SELECT * FROM base", &["base"])],
            false,
        )
        .unwrap();
        let count = || -> i64 {
            db.conn()
                .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
                .unwrap()
        };
        let before = count();
        for sql in [
            "WITH x(v) AS (SELECT 1) DELETE FROM entries WHERE hid IN (SELECT v FROM x)",
            "SELECT 1) SELECT 1; DELETE FROM entries; SELECT (1",
            "DELETE FROM entries",
        ] {
            let mut row = db.get_entry("view:base").unwrap();
            row.data = Some(json!({"name": "base", "sql": sql, "kit_id": "k"}));
            db.save_existing_entry(&row).unwrap();
            assert!(run(&db, "top").is_err(), "{sql}");
            assert_eq!(count(), before, "{sql}");
        }
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
                    uses: vec![],
                }],
                false,
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("single SELECT"), "unexpected error: {err}");
        assert!(db.get_view("evil").unwrap().is_none());
    }
}
