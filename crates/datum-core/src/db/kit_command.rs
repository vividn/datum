//! Kit-installable Level 1 / Level 2 commands.
//!
//! Slice 6 (da-d4t.6) — every command is, since da-fwwtn, the entry
//! `command:<name>` (`definition.rs`), its data the [`CommandEntry`] with
//! either:
//! - `expands_to` set (Level 1): the dispatcher splices the stored token
//!   list into argv before the parser runs.
//! - `view` set (Level 2): the dispatcher invokes a structured
//!   renderer that runs the named view, projects `columns`, applies
//!   labels/formats/sort/color_rules, and prints the result.
//!
//! Kit installs mark theirs with `kit_id`; uninstall removes only its own.
//! Round-trip (install → export → diff) re-emits the same TOML.

use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

use crate::error::{DatumError, Result};
use crate::kit::{ColorRuleEntry, CommandEntry};

use super::definition::{definition_data, COMMAND_FIELD, KIT_KEY};
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
/// sees the problem before the command is stored. Reject Level 3 (`script`)
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
    /// Install a kit's commands, each the entry `command:<name>` marked
    /// with `kit_id` (the kit's `[kit].name`) — da-fwwtn, `definition.rs`.
    ///
    /// Conflict policy mirrors [`Db::install_kit`]:
    /// - `force = false` → leave any existing command, record `SkippedExisting`.
    /// - `force = true`  → overwrite it and re-mark it with this kit.
    pub fn install_kit_commands(
        &self,
        kit_id: &str,
        commands: &[CommandEntry],
        force: bool,
    ) -> Result<InstallCommandsReport> {
        let mut defs = Vec::with_capacity(commands.len());
        for c in commands {
            validate_command(c)?;
            defs.push((c.name.clone(), definition_data(c, None)?));
        }
        let outcomes = self.install_definitions(COMMAND_FIELD, kit_id, defs, force)?;
        Ok(InstallCommandsReport {
            outcomes: outcomes
                .into_iter()
                .map(|(name, skipped)| {
                    let outcome = match skipped {
                        None => KitCommandOutcome::Installed,
                        Some(existing_kit) => KitCommandOutcome::SkippedExisting { existing_kit },
                    };
                    (name, outcome)
                })
                .collect(),
        })
    }

    /// Delete (softly) every command marked with this `kit_id`. Returns the
    /// names removed.
    pub fn uninstall_kit_commands(&self, kit_id: &str) -> Result<Vec<String>> {
        self.uninstall_definitions(COMMAND_FIELD, kit_id)
    }

    /// Look up a single kit command by name.
    pub fn get_kit_command(&self, name: &str) -> Result<Option<InstalledKitCommand>> {
        let slug = format!("{COMMAND_FIELD}:{name}");
        match self.definition_entry(&slug)?.and_then(|e| e.data) {
            Some(data) => Ok(Some(installed_command(data)?)),
            None => Ok(None),
        }
    }

    /// List every kit command in name order — less any whose entry is not
    /// one (edited or synced past the write check), which would otherwise
    /// hide every other.
    pub fn list_kit_commands(&self) -> Result<Vec<InstalledKitCommand>> {
        Ok(self
            .definitions(COMMAND_FIELD, None)?
            .into_iter()
            .filter_map(|data| installed_command(data).ok())
            .collect())
    }

    /// Every command marked with `kit_id`, in the TOML [`CommandEntry`]
    /// shape, for `kit export` — less any that is not one, as above.
    pub fn load_commands_by_kit(&self, kit_id: &str) -> Result<Vec<CommandEntry>> {
        Ok(self
            .definitions(COMMAND_FIELD, Some(kit_id))?
            .into_iter()
            .filter_map(|data| serde_json::from_value(data).ok())
            .collect())
    }
}

/// A command entry's data as an [`InstalledKitCommand`].
fn installed_command(data: JsonValue) -> Result<InstalledKitCommand> {
    let kit_id = data[KIT_KEY].as_str().map(str::to_string);
    let c: CommandEntry = serde_json::from_value(data)?;
    Ok(InstalledKitCommand {
        name: c.name,
        kit_id,
        expands_to: c.expands_to,
        view_name: c.view,
        help: c.help,
        columns: c.columns,
        labels: c.labels,
        formats: c.formats,
        sort_col: c.sort,
        flags: c.flags,
        color_rules: c.color_rules,
        script_text: c.script,
    })
}
