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
    pub empty: Option<String>,
    /// Its arguments, as declared (da-7ramf): `key`, `key=`, `key=default`.
    pub args: Vec<String>,
    /// Its on/off flags: `--name` binds `:name` to 1, and 0 when absent.
    pub switches: Vec<String>,
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
            || c.empty.is_some()
            || !c.args.is_empty()
            || !c.switches.is_empty()
        {
            return Err(DatumError::Other(format!(
                "kit command '{}': Level 1 alias commands cannot set columns/labels/formats/sort/color_rules/empty/args/switches",
                c.name
            )));
        }
    } else {
        // Level 2: an argument is the named parameter `:key`, so its name is
        // a column name too (da-7ramf); and `key:=value` is refused, as an
        // argument the caller cannot fill is no argument.
        if let Some(bad) = c.args.iter().find(|a| {
            let (name, kind) = datum_parse::spec_keys::parse_key_decl(a);
            !crate::kit_color_rule::is_column_name(name)
                || matches!(kind, datum_parse::spec_keys::KeyKind::Assign(_))
        }) {
            return Err(DatumError::Other(format!(
                "kit command '{}': argument `{bad}` is not `key`, `key=` or `key=default` with a name of letters, digits and `_`",
                c.name
            )));
        }
        // A switch is the flag `--name` and the parameter `:name`, so it is a
        // name, and neither an argument's nor one of the flags every command
        // already reads.
        let arg_names: Vec<&str> = c
            .args
            .iter()
            .map(|a| datum_parse::spec_keys::parse_key_decl(a).0)
            .collect();
        if let Some(bad) = c.switches.iter().find(|n| {
            !crate::kit_color_rule::is_column_name(n)
                || arg_names.contains(&n.as_str())
                || ["watch", "sort", "show"].contains(&n.as_str())
        }) {
            return Err(DatumError::Other(format!(
                "kit command '{}': switch `{bad}` is not a name, or is already an argument or one of --watch, --sort, --show",
                c.name
            )));
        }
        // Every column name must be well formed; whether the view
        // has it is known only when the command runs (da-ggzv1).
        if let Some(bad) = c
            .columns
            .iter()
            .chain(c.labels.keys())
            .chain(c.formats.keys())
            .chain(&c.sort)
            .find(|n| !crate::kit_color_rule::is_column_name(n))
        {
            return Err(DatumError::Other(format!(
                "kit command '{}': `{bad}` is not a column name (letters, digits and `_`, not starting with a digit)",
                c.name
            )));
        }
        // Parse every color rule expression up front so a bad grammar
        // fails install rather than runtime.
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
        empty: c.empty,
        args: c.args,
        switches: c.switches,
        script_text: c.script,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level2(toml: &str) -> CommandEntry {
        toml::from_str(&format!("name = \"k\"\nview = \"v\"\n{toml}")).unwrap()
    }

    #[test]
    fn install_refuses_a_malformed_column_name() {
        for (toml, bad) in [
            ("columns = [\"field\", \"next due\"]", "next due"),
            ("labels = { \"count(*)\" = \"n\" }", "count(*)"),
            ("formats = { \"1st\" = \"duration\" }", "1st"),
            ("sort = \"\"", ""),
        ] {
            let err = validate_command(&level2(toml)).unwrap_err().to_string();
            assert!(
                err.contains(&format!("`{bad}` is not a column name")),
                "{err}"
            );
        }
        validate_command(&level2(
            "columns = [\"field\", \"_n2\"]\nsort = \"iti\"\nlabels = { iti = \"freq\" }",
        ))
        .unwrap();
    }

    /// da-7ramf: an argument is a named parameter, so its name is a column
    /// name — which refuses `key:=value` — and an alias-style command has none.
    #[test]
    fn install_checks_argument_names() {
        validate_command(&level2("args = [\"field=\", \"all=false\", \"n\"]")).unwrap();
        for (toml, bad) in [("args = [\"x:=1\"]", "x:=1"), ("args = [\"a b=\"]", "a b=")] {
            let err = validate_command(&level2(toml)).unwrap_err().to_string();
            assert!(
                err.contains(&format!("argument `{bad}` is not `key`")),
                "{err}"
            );
        }
        let l1: CommandEntry =
            toml::from_str("name = \"k\"\nexpands_to = \"occur x\"\nargs = [\"a\"]").unwrap();
        let err = validate_command(&l1).unwrap_err().to_string();
        assert!(err.contains("empty/args"), "{err}");
    }

    /// A switch is a name no argument or built-in flag already has.
    #[test]
    fn install_checks_switch_names() {
        validate_command(&level2("args = [\"field=\"]\nswitches = [\"all\"]")).unwrap();
        for toml in [
            "args = [\"all=\"]\nswitches = [\"all\"]",
            "switches = [\"sort\"]",
            "switches = [\"a-b\"]",
        ] {
            let err = validate_command(&level2(toml)).unwrap_err().to_string();
            assert!(
                err.contains("is not a name, or is already"),
                "{toml}: {err}"
            );
        }
    }

    /// da-x3369: `empty` is a Level 2 key — stored and read back on install,
    /// refused on an alias-style command.
    #[test]
    fn empty_installs_on_level2_only() {
        let db = crate::test_util::fresh_db();
        let c = level2("empty = \"all done\"");
        db.install_kit_commands("t", std::slice::from_ref(&c), false)
            .unwrap();
        let got = db.get_kit_command("k").unwrap().unwrap();
        assert_eq!(got.empty.as_deref(), Some("all done"));
        let back = db.load_commands_by_kit("t").unwrap();
        assert_eq!(back[0].empty.as_deref(), Some("all done"));
        let l1: CommandEntry =
            toml::from_str("name = \"k\"\nexpands_to = \"occur x\"\nempty = \"x\"").unwrap();
        let err = validate_command(&l1).unwrap_err().to_string();
        assert!(err.contains("color_rules/empty"), "{err}");
    }
}
