//! CLI argument definitions.
//!
//! Both `datum <verb> <field>` and `datum <field> <verb>` are supported —
//! verb/field order detection happens in `LogArgs::normalize()`.
//!
//! Stacked short flags (-yy, -qqq) are handled by a pre-pass before clap
//! sees the arguments (see `Cli::parse_with_alias_resolution`).

use clap::{Args, Parser, Subcommand};

/// datum — personal data tracker
#[derive(Parser, Debug)]
#[command(name = "datum", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Pre-process argv:
    ///   1. Expand aliases (look up first arg in the DB alias table)
    ///   2. Expand stacked flags like -yy → -y -y, -qqq → -q -q -q
    ///   3. Parse with clap
    pub fn parse_with_alias_resolution() -> anyhow::Result<Self> {
        let raw: Vec<String> = std::env::args().collect();
        let expanded = expand_argv(raw)?;
        Ok(Cli::parse_from(expanded))
    }
}

/// Expand stacked short flags and resolve aliases in argv.
pub fn expand_argv(argv: Vec<String>) -> anyhow::Result<Vec<String>> {
    let mut out: Vec<String> = Vec::with_capacity(argv.len() + 4);
    for arg in argv {
        if let Some(expanded) = expand_stacked(&arg) {
            out.extend(expanded);
        } else {
            out.push(arg);
        }
    }
    Ok(out)
}

/// Expand "-yy" → ["-y", "-y"], "-qqqq" → ["-q", "-q", "-q", "-q"], etc.
/// Returns None if no expansion needed.
fn expand_stacked(s: &str) -> Option<Vec<String>> {
    if !s.starts_with('-') || s.len() < 3 {
        return None;
    }
    let inner = &s[1..];
    let ch = inner.chars().next()?;
    // Only expand if all chars in the flag are the same letter
    if inner.chars().all(|c| c == ch) && matches!(ch, 'y' | 'q') {
        let count = inner.len();
        if count > 1 {
            return Some(vec![format!("-{ch}"); count]);
        }
    }
    None
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Record a point-in-time event.
    Occur(LogArgs),
    /// Begin a duration-tracked activity.
    Start(LogArgs),
    /// End a duration-tracked activity.
    #[command(alias = "end")]
    Stop(LogArgs),
    /// Transition a field to a new state.
    Switch(LogArgs),
    /// Record non-time-bound data.
    #[command(alias = "add")]
    Note(LogArgs),

    /// Show recent entries (defaults to last 10).
    Tail(TailArgs),
    /// Show current state of tracked fields (nowview).
    Now,

    /// Manage field specs (schemas).
    #[command(subcommand)]
    Spec(SpecCommand),
    /// Manage aliases.
    #[command(subcommand)]
    Alias(AliasCommand),

    /// Migrate data from a datum legacy backup JSON file.
    Migrate(MigrateArgs),
}

/// Arguments shared by all logging commands (occur, start, stop, switch, note).
#[derive(Args, Debug)]
pub struct LogArgs {
    /// Field name or verb (order-independent: `datum occur mood` or `datum mood occur`)
    pub field_or_verb: String,

    /// Second positional: field name, value, or key=value pairs
    pub rest: Vec<String>,

    /// Go back N days (stackable: -y = yesterday, -yy = 2 days ago)
    #[arg(short = 'y', action = clap::ArgAction::Count)]
    pub yesterday: u8,

    /// Go back N×5 minutes (stackable: -q = 5min ago, -qq = 10min ago)
    #[arg(short = 'q', action = clap::ArgAction::Count)]
    pub quick: u8,

    /// Date override: offset like "-1" (yesterday), "+1" (tomorrow), or ISO "2024-03-16"
    #[arg(short = 'd', long)]
    pub date: Option<String>,

    /// Time override: offset like "-8" (8 min ago), "+5" (5 min from now), or "15:13", "9:15am"
    #[arg(short = 't', long)]
    pub time: Option<String>,

    /// Duration: "30m", "1:30", "2h". For start/stop pairs.
    #[arg(long)]
    pub dur: Option<String>,

    /// Timezone override (IANA name or UTC offset like "-7")
    #[arg(short = 'z', long)]
    pub timezone: Option<String>,

    /// Unit for the primary value (e.g. "cups", "kg")
    #[arg(short = 'u', long)]
    pub unit: Option<String>,

    /// Additional key=value pairs for extra_data
    #[arg(short = 'k', long = "key", value_name = "KEY=VALUE")]
    pub keys: Vec<String>,

    /// Comment
    #[arg(short = 'c', long)]
    pub comment: Option<String>,
}

#[derive(Args, Debug)]
pub struct TailArgs {
    /// Field to filter by (optional, shows all fields if omitted)
    pub field: Option<String>,
    /// Number of entries to show
    #[arg(short = 'n', default_value = "10")]
    pub count: usize,
}

#[derive(Subcommand, Debug)]
pub enum SpecCommand {
    /// Show a field spec
    Show { field: String },
    /// Define or update a field spec from JSON
    Set { field: String, json: String },
    /// List all field specs
    List,
}

#[derive(Subcommand, Debug)]
pub enum AliasCommand {
    /// Show an alias expansion
    Show { name: String },
    /// Define an alias: `datum alias set vbp "note word srcLang=pt pos=verb"`
    Set { name: String, expansion: String },
    /// List all aliases
    List,
    /// Remove an alias
    Remove { name: String },
}

#[derive(Args, Debug)]
pub struct MigrateArgs {
    /// Path to datum backup JSON file
    pub backup_file: String,
    /// Dry run — show what would be imported without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_stacked_yy() {
        let expanded = expand_stacked("-yy").unwrap();
        assert_eq!(expanded, vec!["-y", "-y"]);
    }

    #[test]
    fn test_expand_stacked_qqqq() {
        let expanded = expand_stacked("-qqqq").unwrap();
        assert_eq!(expanded, vec!["-q", "-q", "-q", "-q"]);
    }

    #[test]
    fn test_no_expand_single() {
        assert!(expand_stacked("-y").is_none());
        assert!(expand_stacked("-q").is_none());
    }

    #[test]
    fn test_no_expand_non_stacked() {
        assert!(expand_stacked("-d").is_none());
        assert!(expand_stacked("--date").is_none());
    }
}
