//! CLI argument definitions.
//!
//! Both `datum <verb> <field>` and `datum <field> <verb>` are supported —
//! the field/verb swap happens in the pre-pass before clap sees the arguments.
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
    ///   2. Swap field/verb if field comes before verb
    ///   3. Expand stacked flags like -yy → -y -y, -qqq → -q -q -q
    ///   4. Parse with clap
    pub fn parse_with_alias_resolution() -> anyhow::Result<Self> {
        let raw: Vec<String> = std::env::args().collect();
        let expanded = expand_argv(raw)?;
        Ok(Cli::parse_from(expanded))
    }
}

/// Known subcommands — keep in sync with Command enum.
const KNOWN_SUBCOMMANDS: &[&str] = &[
    "occur", "start", "stop", "end", "switch", "note", "add",
    "tail", "head", "now", "spec", "alias", "migrate", "get", "delete", "help",
    "track", "ignore", "grep",
];

/// Expand argv: resolve aliases, swap field/verb order, expand stacked flags.
pub fn expand_argv(argv: Vec<String>) -> anyhow::Result<Vec<String>> {
    // Step 1: Alias resolution — try to expand argv[1] as an alias from the DB.
    let argv = try_expand_alias(argv);

    // Step 2: Field/verb swap — if argv[1] is a field (not a subcommand) and
    //         argv[2] is a known verb, swap them so clap sees <verb> <field>.
    let argv = maybe_swap_field_verb(argv);

    // Step 3: Stacked flag expansion (-yy → -y -y, -qqq → -q -q -q)
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

/// Try to resolve argv[1] as an alias from the database.
/// On any failure (DB missing, alias not found, parse error) returns argv unchanged.
fn try_expand_alias(argv: Vec<String>) -> Vec<String> {
    if argv.len() < 2 {
        return argv;
    }
    // Only attempt lookup if argv[1] is not already a known subcommand
    if KNOWN_SUBCOMMANDS.contains(&argv[1].as_str()) {
        return argv;
    }

    let expanded = (|| -> Option<Vec<String>> {
        let db_path = crate::config::db_path().ok()?;
        if !std::path::Path::new(&db_path).exists() {
            return None;
        }
        let db = datum_core::db::Db::open(&db_path).ok()?;
        let alias = db.lookup_alias(&argv[1]).ok()??;
        let mut result = vec![argv[0].clone()];
        result.extend(alias.expand(&argv[2..]));
        Some(result)
    })();

    expanded.unwrap_or(argv)
}

/// Swap argv[1] and argv[2] when argv[1] is a field name and argv[2] is a verb.
/// E.g. `datum mood switch happy` → `datum switch mood happy`.
fn maybe_swap_field_verb(mut argv: Vec<String>) -> Vec<String> {
    if argv.len() >= 3
        && !KNOWN_SUBCOMMANDS.contains(&argv[1].as_str())
        && KNOWN_SUBCOMMANDS.contains(&argv[2].as_str())
    {
        argv.swap(1, 2);
    }
    argv
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
    /// Show oldest entries (defaults to first 10).
    Head(HeadArgs),
    /// Show current state of tracked fields (nowview).
    Now(NowArgs),

    /// Print a single entry by id or slug.
    Get(GetArgs),
    /// Delete an entry by id or slug.
    Delete(DeleteArgs),

    /// Manage field specs (schemas).
    #[command(subcommand)]
    Spec(SpecCommand),
    /// Manage aliases.
    #[command(subcommand)]
    Alias(AliasCommand),

    /// Track a field in the now view.
    Track(TrackArgs),
    /// Ignore a field in the now view.
    Ignore(IgnoreArgs),

    /// Migrate data from a datum legacy backup JSON file.
    Migrate(MigrateArgs),

    /// Regex search over entry JSON content (multi-pattern AND).
    Grep(GrepArgs),
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

#[derive(Args, Debug)]
pub struct HeadArgs {
    /// Field to filter by (optional, shows all fields if omitted)
    pub field: Option<String>,
    /// Number of entries to show
    #[arg(short = 'n', long = "limit", default_value = "10")]
    pub count: usize,
}

#[derive(Args, Debug)]
pub struct GetArgs {
    /// Entry id or slug
    pub id_or_slug: String,
}

#[derive(Args, Debug)]
pub struct DeleteArgs {
    /// Entry id or slug
    pub id_or_slug: String,
    /// Skip confirmation prompt
    #[arg(long)]
    pub force: bool,
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
pub struct NowArgs {
    /// Show all fields including ignored (appended in dim text after a separator)
    #[arg(short = 'A')]
    pub show_all: bool,
}

#[derive(Args, Debug)]
pub struct TrackArgs {
    /// Field name to track
    pub field: String,
    /// Color override: hex ("ff6600" or "#ff6600") or ANSI-256 number ("196")
    #[arg(long)]
    pub color: Option<String>,
    /// Per-state color override in STATE=COLOR format (repeatable)
    #[arg(long = "state", value_name = "STATE=COLOR")]
    pub state_colors: Vec<String>,
}

#[derive(Args, Debug)]
pub struct IgnoreArgs {
    /// Field name to ignore in the now view
    pub field: String,
}

#[derive(Args, Debug)]
pub struct GrepArgs {
    /// One or more regex patterns (all must match — AND)
    #[arg(required = true)]
    pub patterns: Vec<String>,
    /// Restrict the search to a single field
    #[arg(short = 'f', long)]
    pub field: Option<String>,
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

    #[test]
    fn test_maybe_swap_field_verb() {
        let argv = vec!["datum".to_string(), "mood".to_string(), "switch".to_string(), "happy".to_string()];
        let result = maybe_swap_field_verb(argv);
        assert_eq!(result, vec!["datum", "switch", "mood", "happy"]);
    }

    #[test]
    fn test_no_swap_when_verb_first() {
        let argv = vec!["datum".to_string(), "switch".to_string(), "mood".to_string(), "happy".to_string()];
        let result = maybe_swap_field_verb(argv.clone());
        assert_eq!(result, argv);
    }

    #[test]
    fn test_no_swap_when_no_verb_at_pos2() {
        let argv = vec!["datum".to_string(), "coffee".to_string(), "2".to_string()];
        let result = maybe_swap_field_verb(argv.clone());
        assert_eq!(result, argv);
    }
}
