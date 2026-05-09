//! Kit-installable Level 1 / Level 2 commands.
//!
//! Slice 6 (da-d4t.6) — every command is a row in `kit_commands` with
//! either:
//! - `expands_to` set (Level 1): the dispatcher splices the stored token
//!   list into argv before the parser runs.
//! - `view_name` set (Level 2): the dispatcher invokes a structured
//!   renderer that runs the named view, projects `columns_json`, applies
//!   labels/formats/sort/color_rules, and prints the result.
//!
//! Kit installs tag rows with `kit_id`; uninstall drops only the rows it
//! owns. Round-trip (install → export → diff) re-emits the same TOML.

use rusqlite::{params, OptionalExtension};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use crate::error::{DatumError, Result};
use crate::kit::{ColorRuleEntry, CommandEntry};

use super::Db;

/// One row of [`Db::list_kit_commands`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledKitCommand {
    pub name: String,
    pub kit_id: Option<String>,
    pub expands_to: Option<String>,
    pub view_name: Option<String>,
    pub help: Option<String>,
    pub columns: Vec<String>,
    pub labels: BTreeMap<String, String>,
    pub formats: BTreeMap<String, String>,
    pub sort_col: Option<String>,
    pub flags: Vec<String>,
    pub color_rules: Vec<ColorRuleEntry>,
    pub script_text: Option<String>,
}

/// Per-command outcome reported by [`Db::install_kit_commands`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitCommandOutcome {
    Installed,
    SkippedExisting { existing_kit: Option<String> },
}

/// Aggregate report from [`Db::install_kit_commands`].
#[derive(Debug, Clone, Default)]
pub struct InstallCommandsReport {
    pub outcomes: Vec<(String, KitCommandOutcome)>,
}

impl InstallCommandsReport {
    pub fn installed(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter(|&(_name, o)| matches!(o, KitCommandOutcome::Installed))
            .map(|(name, _o)| name.as_str())
    }

    pub fn skipped(&self) -> impl Iterator<Item = &str> {
        self.outcomes
            .iter()
            .filter(|&(_name, o)| matches!(o, KitCommandOutcome::SkippedExisting { .. }))
            .map(|(name, _o)| name.as_str())
    }
}

/// Validate one command entry. Surface install-time errors so the user
/// sees the problem before the row hits the DB. Reject Level 3 (`script`)
/// explicitly so kits that ship one fail clearly rather than silently
/// installing dead rows.
pub fn validate_command(c: &CommandEntry) -> Result<()> {
    if c.name.trim().is_empty() {
        return Err(DatumError::Other(
            "kit command: missing or empty `name`".to_string(),
        ));
    }
    if c.script.is_some() {
        return Err(DatumError::Other(format!(
            "kit command '{}': scripted commands not yet supported (Level 3 deferred)",
            c.name
        )));
    }
    let has_expands = c.expands_to.is_some();
    let has_view = c.view.is_some();
    if has_expands == has_view {
        return Err(DatumError::Other(format!(
            "kit command '{}': must set exactly one of `expands_to` (Level 1) or `view` (Level 2)",
            c.name
        )));
    }
    if has_expands {
        // Level 1: forbid Level 2 fields so an author who fat-fingers
        // `view = "..."` doesn't get a half-configured row.
        if !c.columns.is_empty()
            || !c.labels.is_empty()
            || !c.formats.is_empty()
            || c.sort.is_some()
            || !c.color_rules.is_empty()
        {
            return Err(DatumError::Other(format!(
                "kit command '{}': Level 1 alias commands cannot set columns/labels/formats/sort/color_rules",
                c.name
            )));
        }
    } else {
        // Level 2: parse every color rule expression up front so a bad
        // grammar fails install rather than runtime.
        for r in &c.color_rules {
            crate::kit_color_rule::parse_expr(&r.when).map_err(|e| {
                DatumError::Other(format!(
                    "kit command '{}': bad color rule `when = \"{}\"`: {}",
                    c.name, r.when, e
                ))
            })?;
        }
    }
    Ok(())
}

impl Db {
    /// Install a kit's commands. `kit_id` tags every persisted row;
    /// callers should pass the kit's `[kit].name` from the TOML.
    ///
    /// Conflict policy mirrors [`Db::install_kit`]:
    /// - `force = false` → leave any existing row, record `SkippedExisting`.
    /// - `force = true`  → overwrite the row's payload and re-tag its
    ///   `kit_id`.
    pub fn install_kit_commands(
        &self,
        kit_id: &str,
        commands: &[CommandEntry],
        force: bool,
    ) -> Result<InstallCommandsReport> {
        for c in commands {
            validate_command(c)?;
        }

        let tx = self.conn.unchecked_transaction()?;
        let mut report = InstallCommandsReport::default();

        for c in commands {
            let existing_kit: Option<Option<String>> = tx
                .query_row(
                    "SELECT kit_id FROM kit_commands WHERE name = ?1",
                    params![c.name],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()?;

            if let Some(existing) = existing_kit {
                if !force {
                    report.outcomes.push((
                        c.name.clone(),
                        KitCommandOutcome::SkippedExisting {
                            existing_kit: existing,
                        },
                    ));
                    continue;
                }
            }

            let columns_json = if c.columns.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&c.columns)?)
            };
            let labels_json = if c.labels.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&c.labels)?)
            };
            let formats_json = if c.formats.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&c.formats)?)
            };
            let flags_json = if c.flags.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&c.flags)?)
            };
            let color_rules_json = if c.color_rules.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&c.color_rules)?)
            };

            tx.execute(
                "INSERT INTO kit_commands (
                    name, kit_id, expands_to, view_name, help,
                    columns_json, labels_json, formats_json,
                    sort_col, flags_json, color_rules_json, script_text
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(name) DO UPDATE SET
                    kit_id           = excluded.kit_id,
                    expands_to       = excluded.expands_to,
                    view_name        = excluded.view_name,
                    help             = excluded.help,
                    columns_json     = excluded.columns_json,
                    labels_json      = excluded.labels_json,
                    formats_json     = excluded.formats_json,
                    sort_col         = excluded.sort_col,
                    flags_json       = excluded.flags_json,
                    color_rules_json = excluded.color_rules_json,
                    script_text      = excluded.script_text",
                params![
                    c.name,
                    kit_id,
                    c.expands_to,
                    c.view,
                    c.help,
                    columns_json,
                    labels_json,
                    formats_json,
                    c.sort,
                    flags_json,
                    color_rules_json,
                    c.script,
                ],
            )?;

            report
                .outcomes
                .push((c.name.clone(), KitCommandOutcome::Installed));
        }

        tx.commit()?;
        Ok(report)
    }

    /// Drop every command tagged with this `kit_id`. Returns the names
    /// removed.
    pub fn uninstall_kit_commands(&self, kit_id: &str) -> Result<Vec<String>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt =
            tx.prepare("SELECT name FROM kit_commands WHERE kit_id = ?1 ORDER BY name")?;
        let names: Vec<String> = stmt
            .query_map(params![kit_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(DatumError::Db)?;
        drop(stmt);
        tx.execute(
            "DELETE FROM kit_commands WHERE kit_id = ?1",
            params![kit_id],
        )?;
        tx.commit()?;
        Ok(names)
    }

    /// Look up a single kit command by name.
    pub fn get_kit_command(&self, name: &str) -> Result<Option<InstalledKitCommand>> {
        let row = self
            .conn
            .query_row(
                "SELECT name, kit_id, expands_to, view_name, help,
                        columns_json, labels_json, formats_json,
                        sort_col, flags_json, color_rules_json, script_text
                 FROM kit_commands WHERE name = ?1",
                params![name],
                row_to_kit_command,
            )
            .optional()?;
        Ok(row)
    }

    /// List every kit command in name order.
    pub fn list_kit_commands(&self) -> Result<Vec<InstalledKitCommand>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, kit_id, expands_to, view_name, help,
                    columns_json, labels_json, formats_json,
                    sort_col, flags_json, color_rules_json, script_text
             FROM kit_commands ORDER BY name",
        )?;
        let rows = stmt.query_map([], row_to_kit_command)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(DatumError::Db)?);
        }
        Ok(out)
    }

    /// Load every command tagged with `kit_id` for `kit export`. Returns
    /// rows projected back into the TOML [`CommandEntry`] shape.
    pub fn load_commands_by_kit(&self, kit_id: &str) -> Result<Vec<CommandEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, kit_id, expands_to, view_name, help,
                    columns_json, labels_json, formats_json,
                    sort_col, flags_json, color_rules_json, script_text
             FROM kit_commands WHERE kit_id = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![kit_id], row_to_kit_command)?;
        let mut out = Vec::new();
        for r in rows {
            let row = r.map_err(DatumError::Db)?;
            out.push(installed_to_entry(row));
        }
        Ok(out)
    }
}

fn installed_to_entry(row: InstalledKitCommand) -> CommandEntry {
    CommandEntry {
        name: row.name,
        expands_to: row.expands_to,
        view: row.view_name,
        help: row.help,
        columns: row.columns,
        labels: row.labels,
        formats: row.formats,
        sort: row.sort_col,
        flags: row.flags,
        color_rules: row.color_rules,
        script: row.script_text,
    }
}

fn row_to_kit_command(row: &rusqlite::Row<'_>) -> rusqlite::Result<InstalledKitCommand> {
    let columns_json: Option<String> = row.get(5)?;
    let labels_json: Option<String> = row.get(6)?;
    let formats_json: Option<String> = row.get(7)?;
    let flags_json: Option<String> = row.get(9)?;
    let color_rules_json: Option<String> = row.get(10)?;

    let columns = parse_string_list(columns_json.as_deref());
    let labels = parse_string_map(labels_json.as_deref());
    let formats = parse_string_map(formats_json.as_deref());
    let flags = parse_string_list(flags_json.as_deref());
    let color_rules = parse_color_rules(color_rules_json.as_deref());

    Ok(InstalledKitCommand {
        name: row.get(0)?,
        kit_id: row.get(1)?,
        expands_to: row.get(2)?,
        view_name: row.get(3)?,
        help: row.get(4)?,
        columns,
        labels,
        formats,
        sort_col: row.get(8)?,
        flags,
        color_rules,
        script_text: row.get(11)?,
    })
}

fn parse_string_list(s: Option<&str>) -> Vec<String> {
    let Some(s) = s else { return Vec::new() };
    serde_json::from_str(s).unwrap_or_default()
}

fn parse_string_map(s: Option<&str>) -> BTreeMap<String, String> {
    let Some(s) = s else { return BTreeMap::new() };
    let v: std::result::Result<JsonValue, _> = serde_json::from_str(s);
    let Ok(v) = v else { return BTreeMap::new() };
    let Some(obj) = v.as_object() else {
        return BTreeMap::new();
    };
    obj.iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect()
}

fn parse_color_rules(s: Option<&str>) -> Vec<ColorRuleEntry> {
    let Some(s) = s else { return Vec::new() };
    serde_json::from_str(s).unwrap_or_default()
}
