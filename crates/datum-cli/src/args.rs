//! CLI argument definitions.
//!
//! Both `datum <verb> <field>` and `datum <field> <verb>` are supported —
//! the field/verb swap happens in the pre-pass before clap sees the arguments.
//!
//! Stacked short flags (-yy, -qqq) are handled by a pre-pass before clap
//! sees the arguments (see `Cli::parse_with_alias_resolution`).

use clap::{Args, Parser, Subcommand, ValueEnum};
use datum_core::db::ConflictStrategy;

use crate::output::{OutputArgs, Show};

/// Clap-facing wrapper for the shared output flags.
#[derive(Args, Debug, Clone, Default)]
pub struct OutputFlags {
    /// How much of each result to display: default | none | minimal | standard | format | all.
    #[arg(long = "show", global = true, value_name = "MODE", default_value = "default")]
    pub show: String,

    /// Shorthand for `--show all`.
    #[arg(short = 'A', long = "show-all", global = true)]
    pub show_all: bool,

    /// Custom `%field%`-templated output string. Implies `--show format` unless another `--show` is set.
    #[arg(short = 'o', long = "format-string", global = true, value_name = "TEMPLATE")]
    pub format_string: Option<String>,
}

impl OutputFlags {
    pub fn resolve(&self) -> OutputArgs {
        let show = if self.show_all {
            Show::All
        } else {
            self.show
                .parse::<Show>()
                .unwrap_or_default()
        };
        let show = if self.format_string.is_some() && self.show == "default" && !self.show_all {
            Show::Format
        } else {
            show
        };
        OutputArgs {
            show,
            format_string: self.format_string.clone(),
        }
    }
}

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
    "tail", "head", "now", "spec", "alias", "migrate", "get", "delete", "del", "undo", "help",
    "track", "ignore", "grep", "check", "map", "reduce", "red",
    "update", "merge", "rekey", "edit", "retime", "rt",
    "backup", "restore",
];

/// Expand argv: resolve aliases, swap field/verb order, expand stacked flags.
pub fn expand_argv(argv: Vec<String>) -> anyhow::Result<Vec<String>> {
    // Step 1: Alias resolution — try to expand argv[1] as an alias from the DB.
    let argv = try_expand_alias(argv);

    // Step 2: Field/verb swap — if argv[1] is a field (not a subcommand) and
    //         argv[2] is a known verb, swap them so clap sees <verb> <field>.
    let argv = maybe_swap_field_verb(argv);

    // Step 3: Stacked flag expansion (-yy → -y -y, -qqq → -q -q -q)
    Ok(expand_stacked_flags(argv))
}

/// Like [`expand_argv`] but uses the provided `db` for alias lookup instead of
/// opening one from `config::db_path()`. Lets in-process test harnesses drive
/// the full arg-parse path against an in-memory Db.
pub fn expand_argv_with_db(
    argv: Vec<String>,
    db: &datum_core::db::Db,
) -> anyhow::Result<Vec<String>> {
    let argv = try_expand_alias_with_db(argv, db);
    let argv = maybe_swap_field_verb(argv);
    Ok(expand_stacked_flags(argv))
}

fn expand_stacked_flags(argv: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(argv.len() + 4);
    for arg in argv {
        if let Some(expanded) = expand_stacked(&arg) {
            out.extend(expanded);
        } else {
            out.push(arg);
        }
    }
    out
}

/// Try to resolve argv[1] as an alias from the database.
/// On any failure (DB missing, alias not found, parse error) returns argv unchanged.
fn try_expand_alias(argv: Vec<String>) -> Vec<String> {
    if argv.len() < 2 || KNOWN_SUBCOMMANDS.contains(&argv[1].as_str()) {
        return argv;
    }

    let maybe_db = (|| -> Option<datum_core::db::Db> {
        let db_path = crate::config::db_path().ok()?;
        if !std::path::Path::new(&db_path).exists() {
            return None;
        }
        datum_core::db::Db::open(&db_path).ok()
    })();

    match maybe_db {
        Some(db) => try_expand_alias_with_db(argv, &db),
        None => argv,
    }
}

/// Alias-expansion variant that uses the given `db` for lookup.
fn try_expand_alias_with_db(argv: Vec<String>, db: &datum_core::db::Db) -> Vec<String> {
    if argv.len() < 2 || KNOWN_SUBCOMMANDS.contains(&argv[1].as_str()) {
        return argv;
    }
    match db.lookup_alias(&argv[1]) {
        Ok(Some(alias)) => {
            let mut result = vec![argv[0].clone()];
            result.extend(alias.expand(&argv[2..]));
            result
        }
        _ => argv,
    }
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
    #[command(visible_alias = "del")]
    Delete(DeleteArgs),
    /// Restore the most recently soft-deleted entry (or one specified by slug).
    Undo(UndoArgs),

    /// Open an entry in `$EDITOR` as pretty JSON and save the edited result.
    Edit(EditArgs),

    /// Mutate an existing entry's extra_data in place (replaces specified keys).
    Update(UpdateArgs),
    /// Deep-merge new values into an existing entry's extra_data.
    Merge(UpdateArgs),
    /// Copy an entry to a new id (re-interpolated from the template) — old row stays.
    Rekey(UpdateArgs),

    /// Shift `time_start` / `time_end` on one or many entries by a duration.
    #[command(visible_alias = "rt")]
    Retime(RetimeArgs),

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

    /// Dump every live entry to JSONL (or a tar.gz archive if the path ends in `.tar.gz`).
    Backup(BackupArgs),
    /// Re-import entries from a JSONL or tar.gz backup; duplicates deduped via content_hash.
    Restore(RestoreArgs),

    /// Regex search over entry JSON content (multi-pattern AND).
    Grep(GrepArgs),

    /// Run database consistency checks.
    Check(CheckArgs),

    /// Query a named view (map phase) and print rows as JSON.
    Map(MapArgs),
    /// Query a named view with reduce=true.
    #[command(visible_alias = "red")]
    Reduce(MapArgs),
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

    /// Template for the entry's slug (e.g. `"%field%:%uid%"`). Overrides the
    /// spec's `id_template` if set.
    #[arg(long = "id", value_name = "TEMPLATE")]
    pub id: Option<String>,

    /// Delimiter used to collapse consecutive empty segments produced by a
    /// template with missing keys. Defaults to `:`.
    #[arg(long = "id-delimiter", value_name = "STR", default_value = ":")]
    pub id_delimiter: String,

    /// Stop: close the most recent open block (records its prior state).
    /// Switch: transition from the currently-active state (errors if unchanged,
    /// unless also --moment).
    #[arg(long = "last-state")]
    pub last_state: bool,

    /// Switch: record a point-in-time state change (time_end == time_start).
    /// Also bypasses the --last-state dedup check.
    #[arg(short = 'm', long)]
    pub moment: bool,

    /// How to resolve content_hash collisions with existing entries.
    /// Defaults to `ignore` (silently skip). Mutually exclusive with
    /// `--update` and `--merge`, which are shorthands for the corresponding
    /// strategy.
    #[arg(long, value_enum, conflicts_with_all = ["update", "merge"])]
    pub conflict: Option<ConflictStrategyArg>,

    /// Shorthand for `--conflict update`: overwrite the existing row.
    #[arg(long, conflicts_with = "merge")]
    pub update: bool,

    /// Shorthand for `--conflict merge`: deep-merge extra_data into the existing row.
    #[arg(long)]
    pub merge: bool,

    /// Suppress bookkeeping fields — set `created_at`/`modified_at` to the UNIX
    /// epoch and clear `device_id`. Useful for deterministic fixtures and sync
    /// replay where the stamps come from elsewhere. Short form is `-M` (datum's
    /// `-FM` combo maps here via the `--no-metadata` portion).
    #[arg(short = 'M', long = "no-metadata")]
    pub no_metadata: bool,

    #[command(flatten)]
    pub output_flags: OutputFlags,
}


/// CLI-visible variant of [`ConflictStrategy`].
#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum ConflictStrategyArg {
    Update,
    Merge,
    Rekey,
    Ignore,
}

impl From<ConflictStrategyArg> for ConflictStrategy {
    fn from(v: ConflictStrategyArg) -> Self {
        match v {
            ConflictStrategyArg::Update => ConflictStrategy::Update,
            ConflictStrategyArg::Merge => ConflictStrategy::Merge,
            ConflictStrategyArg::Rekey => ConflictStrategy::Rekey,
            ConflictStrategyArg::Ignore => ConflictStrategy::Ignore,
        }
    }
}

impl LogArgs {
    pub fn conflict_strategy(&self) -> ConflictStrategy {
        if self.update {
            ConflictStrategy::Update
        } else if self.merge {
            ConflictStrategy::Merge
        } else {
            self.conflict.map(Into::into).unwrap_or_default()
        }
    }

    pub fn output(&self) -> OutputArgs { self.output_flags.resolve() }
}

#[derive(Args, Debug)]
pub struct TailArgs {
    /// Field to filter by (optional, shows all fields if omitted)
    pub field: Option<String>,
    /// Number of entries to show (default 10; omit for unlimited when -d/-y is set)
    #[arg(short = 'n')]
    pub count: Option<usize>,

    /// Time metric for ordering: hybrid | occur | create | modify
    #[arg(short = 'm', long = "metric", default_value = "hybrid")]
    pub metric: String,

    /// Invert the output ordering (newest-first for tail)
    #[arg(short = 'r', long = "reverse")]
    pub reverse: bool,

    /// Extra extra_data columns to show (repeatable, comma-splittable).
    #[arg(long = "column", value_name = "NAME")]
    pub columns: Vec<String>,

    /// Date override: offset ("-1" / "+1") or ISO ("2024-03-16"). With no -t,
    /// the window covers the entire day (no default limit).
    #[arg(short = 'd', long)]
    pub date: Option<String>,

    /// Upper-bound time: "+5" / "-8" (relative min), "15:13", "9:15am", or "now".
    #[arg(short = 't', long)]
    pub time: Option<String>,

    /// Go back N days (stackable: -y = yesterday, -yy = 2 days ago)
    #[arg(short = 'y', action = clap::ArgAction::Count)]
    pub yesterday: u8,

    /// Go back N×5 minutes (stackable: -q = 5min ago, -qq = 10min ago)
    #[arg(short = 'q', action = clap::ArgAction::Count)]
    pub quick: u8,

    /// Internal: take the first N instead of the last N (used by `head`).
    #[arg(long, hide = true)]
    pub head: bool,
    // TODO(p-tail-watch): --watch/-w live-update mode. Requires a file watcher
    // + redraw loop; deferred out of Phase-1 scope per da-u8ch.

    #[command(flatten)]
    pub output_flags: OutputFlags,
}

impl TailArgs {
    pub fn output(&self) -> OutputArgs { self.output_flags.resolve() }
}

#[derive(Args, Debug)]
pub struct HeadArgs {
    /// Field to filter by (optional, shows all fields if omitted)
    pub field: Option<String>,
    /// Number of entries to show
    #[arg(short = 'n', long = "limit", default_value = "10")]
    pub count: usize,

    #[command(flatten)]
    pub output_flags: OutputFlags,
}

impl HeadArgs {
    pub fn output(&self) -> OutputArgs { self.output_flags.resolve() }
}

#[derive(Args, Debug)]
pub struct GetArgs {
    /// Entry id or slug
    pub id_or_slug: String,
    /// Pretty-print the entry as indented JSON (two-space indent).
    #[arg(long)]
    pub pretty: bool,
}

/// Arguments shared by `update`, `merge`, and `rekey`.
#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Id or slug of the existing entry to mutate.
    pub id_or_slug: String,

    /// Positional payload: bare tokens replace the primary value; key=value
    /// pairs modify extra_data.
    pub rest: Vec<String>,

    /// Override the subcommand's default strategy
    /// (update / merge / rekey).
    #[arg(long, value_enum)]
    pub strategy: Option<ConflictStrategyArg>,

    /// New id/slug template (rekey) or override for the spec's id_template.
    /// e.g. `"%field%:%state%"`.
    #[arg(long = "id", value_name = "TEMPLATE")]
    pub id: Option<String>,

    /// Delimiter used to collapse empty segments in the interpolated id.
    #[arg(long = "id-delimiter", value_name = "STR", default_value = ":")]
    pub id_delimiter: String,

    /// Additional key=value pairs merged into extra_data.
    #[arg(short = 'k', long = "key", value_name = "KEY=VALUE")]
    pub keys: Vec<String>,

    /// Optional comment appended to extra_data.
    #[arg(short = 'c', long)]
    pub comment: Option<String>,

    #[command(flatten)]
    pub output_flags: OutputFlags,
}

impl UpdateArgs {
    pub fn output(&self) -> OutputArgs { self.output_flags.resolve() }
}

/// Arguments for `retime` / `rt` — batch time shift.
#[derive(Args, Debug)]
pub struct RetimeArgs {
    /// Target a single entry by id or slug. Mutually compatible with
    /// `--field`: when both are set, only a row matching both is shifted.
    pub slug: Option<String>,

    /// Duration offset (accepts `-5m`, `+1h`, `30m`, `1:30`, `PT30M`, ...).
    /// Positive shifts forward in time, negative shifts backward.
    #[arg(long, required = true, allow_hyphen_values = true)]
    pub offset: String,

    /// Restrict to entries with this field name.
    #[arg(short = 'f', long = "field")]
    pub field: Option<String>,

    /// Inclusive lower bound on `time_start`. Accepts `YYYY-MM-DD` (midnight
    /// local) or full RFC3339 timestamps.
    #[arg(long = "after")]
    pub after: Option<String>,

    /// Inclusive upper bound on `time_start`. Same format as `--after`.
    #[arg(long = "before")]
    pub before: Option<String>,
}

#[derive(Args, Debug)]
pub struct EditArgs {
    /// Entry id or slug
    pub id_or_slug: String,

    #[command(flatten)]
    pub output_flags: OutputFlags,
}

impl EditArgs {
    pub fn output(&self) -> OutputArgs { self.output_flags.resolve() }
}

#[derive(Args, Debug)]
pub struct DeleteArgs {
    /// Entry id or slug
    pub id_or_slug: String,
    /// Skip confirmation prompt
    #[arg(long)]
    pub force: bool,

    #[command(flatten)]
    pub output_flags: OutputFlags,
}

impl DeleteArgs {
    pub fn output(&self) -> OutputArgs { self.output_flags.resolve() }
}

#[derive(Args, Debug)]
pub struct UndoArgs {
    /// Id or slug of a specific tombstoned entry to restore. When omitted,
    /// restores the most recently deleted entry within the safety window.
    pub id_or_slug: Option<String>,
    /// Bypass the safety window — restore the most recent tombstone
    /// regardless of age.
    #[arg(long)]
    pub force: bool,
    /// Safety window for the default (no-slug) path; anything older requires
    /// `--force`. Accepts "15m", "1h", "1:30", ISO "PT30M", etc.
    #[arg(long, default_value = "15m")]
    pub window: String,

    #[command(flatten)]
    pub output_flags: OutputFlags,
}

impl UndoArgs {
    pub fn output(&self) -> OutputArgs { self.output_flags.resolve() }
}

#[derive(Subcommand, Debug)]
pub enum SpecCommand {
    /// Show a field spec
    Show { field: String },
    /// Define or update a field spec. Use --json for a full replacement,
    /// or individual flags to merge into the existing spec.
    Set(SpecSetArgs),
    /// List all field specs
    List,
    /// Remove a field spec (cascades to its derived aliases)
    Remove { field: String },
}

#[derive(Args, Debug)]
pub struct SpecSetArgs {
    /// Field name
    pub field: String,

    /// Full spec as JSON (replaces any existing spec completely)
    #[arg(long)]
    pub json: Option<String>,

    /// Default verb: occur | start | stop | switch | note
    #[arg(long = "default-verb")]
    pub default_verb: Option<String>,

    /// Allowed state (repeatable). Each state also becomes a derived alias
    /// expanding to `switch <field> <state>`.
    #[arg(long = "state", value_name = "STATE")]
    pub states: Vec<String>,

    /// Overall field color: hex ("ff6600" or "#ff6600") or ANSI-256 ("196")
    #[arg(long)]
    pub color: Option<String>,

    /// Per-state color override in STATE=COLOR form (repeatable)
    #[arg(long = "state-color", value_name = "STATE=COLOR")]
    pub state_colors: Vec<String>,

    /// Id/slug template, e.g. "%field%:%state%"
    #[arg(long = "id-template")]
    pub id_template: Option<String>,

    /// Free-form description
    #[arg(long)]
    pub description: Option<String>,

    /// Default unit for the primary value
    #[arg(long)]
    pub unit: Option<String>,

    /// Mark the field as tracked (shown in `datum now`)
    #[arg(long)]
    pub track: bool,

    /// Mark the field as ignored in `datum now`
    #[arg(long)]
    pub ignore: bool,
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

    #[command(flatten)]
    pub output_flags: OutputFlags,
}

impl GrepArgs {
    pub fn output(&self) -> OutputArgs { self.output_flags.resolve() }
}

/// Arguments for `datum map` and `datum reduce`.
#[derive(Args, Debug)]
pub struct MapArgs {
    /// View name (e.g. `timingView`)
    pub view_name: String,

    /// Exact key match (JSON; bare strings ok, e.g. `2026-04-15`)
    #[arg(long)]
    pub key: Option<String>,

    /// Range start key (JSON; inclusive)
    #[arg(long)]
    pub startkey: Option<String>,

    /// Range end key (JSON; inclusive)
    #[arg(long)]
    pub endkey: Option<String>,

    /// Group reduced rows by full key
    #[arg(long)]
    pub group: bool,

    /// Group reduced rows by the first N elements of array keys
    #[arg(short = 'g', long = "group-level", value_name = "N")]
    pub group_level: Option<usize>,

    /// Cap the number of rows returned
    #[arg(long)]
    pub limit: Option<usize>,

    /// Run the view's reduce path (not all views reduce)
    #[arg(long)]
    pub reduce: bool,
}

#[derive(Args, Debug)]
pub struct MigrateArgs {
    /// Path to datum backup JSON file
    pub backup_file: String,
    /// Dry run — show what would be imported without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct BackupArgs {
    /// Output file path. Ends in `.tar.gz` or `.tgz` for a compressed archive; otherwise plain JSONL.
    pub path: String,
    /// Overwrite the output file if it already exists.
    #[arg(long)]
    pub overwrite: bool,
}

#[derive(Args, Debug)]
pub struct RestoreArgs {
    /// Input file path (.jsonl, .tar.gz, or .tgz).
    pub path: String,
    /// Overwrite conflicting rows (content_hash match) using `--conflict update`.
    /// Default is to skip duplicates.
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Emit a machine-readable JSON report instead of the human summary.
    #[arg(long)]
    pub json: bool,
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
