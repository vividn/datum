//! CLI intent — the parsed shape of a `datum` invocation.
//!
//! [`Cli`] is the top-level `CommandIntent`: global flags plus a
//! [`Command`] variant carrying the subcommand's typed args. The bespoke
//! parser in [`crate::parse`] produces one of these; `datum-commands`
//! consumes it via `dispatch`.
//!
//! All types here are pure data — no derives, no I/O. Convenience
//! methods that reference `datum-commands` render types (e.g.
//! `OutputArgs`, `ConflictStrategy`) live in `datum-commands::args`
//! alongside the re-exports.

/// Parsed shape of `-A` / `--show` / `-o/--format-string`. The
/// per-command `LogArgs::output()` etc. methods live in `datum-commands`
/// and turn this into a resolved `OutputArgs`.
#[derive(Debug, Clone, Default)]
pub struct OutputFlags {
    pub show: String,
    pub show_all: bool,
    pub format_string: Option<String>,
}

impl OutputFlags {
    /// Build with defaults (show="default").
    pub fn fresh() -> Self {
        Self {
            show: "default".to_string(),
            show_all: false,
            format_string: None,
        }
    }
}

#[derive(Debug)]
pub struct Cli {
    pub db: Option<String>,
    pub command: Command,
}

#[derive(Debug)]
pub enum Command {
    Occur(LogArgs),
    Start(LogArgs),
    Stop(LogArgs),
    Switch(LogArgs),
    Add(LogArgs),

    Tail(TailArgs),
    /// Show oldest entries — powered by Tail with `oldest_first = true`.
    Head(TailArgs),
    Now(NowArgs),

    Get(GetArgs),
    Delete(DeleteArgs),
    Undo(UndoArgs),

    History(HistoryArgs),
    Why(WhyArgs),

    Edit(EditArgs),

    Update(UpdateArgs),
    Merge(UpdateArgs),
    Rekey(UpdateArgs),

    Retime(RetimeArgs),

    Spec(SpecCommand),
    Alias(AliasCommand),

    Track(TrackArgs),
    Ignore(IgnoreArgs),

    Migrate(MigrateCommand),

    Backup(BackupArgs),
    Restore(RestoreArgs),

    Grep(GrepArgs),
    Check(CheckArgs),

    Count(CountArgs),
    Ids(IdsArgs),

    Dayview(DayviewArgs),

    Info(InfoArgs),
    Setup(SetupArgs),

    Config(ConfigCommand),

    Sync(SyncArgs),

    Conflicts(ConflictsCommand),
}

// ─── Conflicts ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ConflictsCommand {
    List(ConflictsListArgs),
    Resolve(ConflictsResolveArgs),
    AutoResolve(ConflictsAutoResolveArgs),
}

#[derive(Debug, Default)]
pub struct ConflictsListArgs {
    pub kind: Option<String>,
    pub all: bool,
}

#[derive(Debug)]
pub struct ConflictsResolveArgs {
    pub hid: String,
    pub keep: Option<String>,
    pub close_older: bool,
}

#[derive(Debug, Default)]
pub struct ConflictsAutoResolveArgs {
    pub min_age: i64,
}

// ─── Sync ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct SyncArgs {
    pub command: Option<SyncCommand>,
    pub push: bool,
    pub pull: bool,
    pub dry_run: bool,
    pub server: Option<String>,
}

#[derive(Debug)]
pub enum SyncCommand {
    Login(SyncLoginArgs),
    Config(SyncConfigCommand),
    Key(SyncKeyCommand),
    Status(SyncStatusArgs),
    InstallDaemon(SyncInstallDaemonArgs),
}

#[derive(Debug, Default)]
pub struct SyncStatusArgs {
    pub server: Option<String>,
}

#[derive(Debug, Default)]
pub struct SyncInstallDaemonArgs {
    pub show: bool,
    pub uninstall: bool,
}

#[derive(Debug, Default)]
pub struct SyncLoginArgs {
    pub server: Option<String>,
}

#[derive(Debug)]
pub enum SyncConfigCommand {
    SetServer { url: String, global: bool },
    Disable,
    Show,
}

#[derive(Debug)]
pub enum SyncKeyCommand {
    Init { server: Option<String>, force: bool },
    Unlock { server: Option<String> },
    RotatePassphrase { server: Option<String> },
    ShowFingerprint { server: Option<String> },
}

// ─── Config ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ConfigCommand {
    Show(ConfigShowArgs),
}

#[derive(Debug, Default)]
pub struct ConfigShowArgs {
    pub json: bool,
}

// ─── Info / Setup ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct InfoArgs {
    pub target: Option<String>,
    pub all: bool,
}

#[derive(Debug)]
pub struct SetupArgs {
    pub force_recreate: bool,
}

// ─── Logging commands (occur/start/stop/switch/add) ──────────────────────

/// The shared arg bag for all logging verbs. Must remain constructible
/// field-by-field: tests build these directly (see `datum-cli/tests/*`).
#[derive(Debug)]
pub struct LogArgs {
    pub field_or_verb: String,
    pub rest: Vec<String>,
    pub yesterday: u8,
    pub quick: u8,
    pub date: Option<String>,
    pub time: Option<String>,
    pub dur: Option<String>,
    pub timezone: Option<String>,
    pub unit: Option<String>,
    pub keys: Vec<String>,
    pub slug_keys: Vec<String>,
    pub comment: Option<String>,
    pub id: Option<String>,
    pub id_delimiter: String,
    /// `-L`/`--last-state` user assertion.
    ///
    /// * `None` — flag not supplied.
    /// * `Some("")` — bare `-L`/`--last-state` (no `=VALUE`). On Stop,
    ///   triggers the close-most-recent-open behavior; on Switch, no-op
    ///   assertion-wise.
    /// * `Some("<value>")` — `-L=<value>`/`--last-state=<value>`; recorded
    ///   as an assertion on the entry's `user_asserted_last_state`.
    pub last_state: Option<String>,
    pub moment: bool,
    pub conflict: Option<ConflictStrategyArg>,
    pub update: bool,
    pub merge: bool,
    pub no_metadata: bool,
    pub no_timestamp: bool,
    pub output_flags: OutputFlags,
}

impl LogArgs {
    pub fn fresh(field_or_verb: impl Into<String>) -> Self {
        Self {
            field_or_verb: field_or_verb.into(),
            rest: Vec::new(),
            yesterday: 0,
            quick: 0,
            date: None,
            time: None,
            dur: None,
            timezone: None,
            unit: None,
            keys: Vec::new(),
            slug_keys: Vec::new(),
            comment: None,
            id: None,
            id_delimiter: ":".to_string(),
            last_state: None,
            moment: false,
            conflict: None,
            update: false,
            merge: false,
            no_metadata: false,
            no_timestamp: false,
            output_flags: OutputFlags::fresh(),
        }
    }
}

/// Mirrors `datum_core::db::ConflictStrategy`; lives here so `datum-parse`
/// stays `datum-core`-free. The `From<ConflictStrategyArg> for
/// ConflictStrategy` conversion lives in `datum-commands::args`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictStrategyArg {
    Update,
    Merge,
    Rekey,
    Ignore,
}

// ─── Read / list commands ─────────────────────────────────────────────────

#[derive(Debug)]
pub struct TailArgs {
    pub field: Option<String>,
    pub count: Option<usize>,
    pub metric: String,
    pub reverse: bool,
    pub columns: Vec<String>,
    pub date: Option<String>,
    pub time: Option<String>,
    pub yesterday: u8,
    pub quick: u8,
    /// Take the first N (oldest) instead of the last N (newest). Powers
    /// `datum head`, which routes through tail with this flag set.
    pub oldest_first: bool,
    pub watch: Option<f64>,
    /// `--table`: force the legacy columnar view (time | field | state | dur |
    /// hid). Default is the info-line renderer matching `show_create` /
    /// `show_update`. Auto-enabled when `--column` or a non-hybrid `--metric`
    /// is set — those are tabular concerns.
    pub table: bool,
    pub output_flags: OutputFlags,
}

impl TailArgs {
    pub fn fresh() -> Self {
        Self {
            field: None,
            count: None,
            metric: "hybrid".to_string(),
            reverse: false,
            columns: Vec::new(),
            date: None,
            time: None,
            yesterday: 0,
            quick: 0,
            oldest_first: false,
            watch: None,
            table: false,
            output_flags: OutputFlags::fresh(),
        }
    }
}

#[derive(Debug)]
pub struct GetArgs {
    pub id_or_slug: String,
    pub pretty: bool,
}

// ─── Mutation (update / merge / rekey) ────────────────────────────────────

#[derive(Debug)]
pub struct UpdateArgs {
    pub id_or_slug: String,
    pub rest: Vec<String>,
    pub strategy: Option<ConflictStrategyArg>,
    pub id: Option<String>,
    pub id_delimiter: String,
    pub keys: Vec<String>,
    pub comment: Option<String>,
    pub dry_run: bool,
    pub strict: bool,
    pub output_flags: OutputFlags,
}

impl UpdateArgs {
    pub fn fresh(id_or_slug: impl Into<String>) -> Self {
        Self {
            id_or_slug: id_or_slug.into(),
            rest: Vec::new(),
            strategy: None,
            id: None,
            id_delimiter: ":".to_string(),
            keys: Vec::new(),
            comment: None,
            dry_run: false,
            strict: false,
            output_flags: OutputFlags::fresh(),
        }
    }
}

#[derive(Debug)]
pub struct RetimeArgs {
    pub slug: Option<String>,
    pub offset: Option<String>,
    pub time: Option<String>,
    pub field: Option<String>,
    pub after: Option<String>,
    pub before: Option<String>,
}

#[derive(Debug)]
pub struct EditArgs {
    pub id_or_slug: String,
    pub output_flags: OutputFlags,
}

#[derive(Debug)]
pub struct DeleteArgs {
    pub id_or_slug: String,
    pub force: bool,
    pub dry_run: bool,
    pub strict: bool,
    pub output_flags: OutputFlags,
}

#[derive(Debug)]
pub struct UndoArgs {
    pub id_or_slug: Option<String>,
    pub to_version: Option<u32>,
    pub force: bool,
    pub window: String,
    pub output_flags: OutputFlags,
}

impl UndoArgs {
    pub fn fresh() -> Self {
        Self {
            id_or_slug: None,
            to_version: None,
            force: false,
            window: "15m".to_string(),
            output_flags: OutputFlags::fresh(),
        }
    }
}

#[derive(Debug)]
pub struct HistoryArgs {
    pub id_or_slug: String,
    pub version: Option<u32>,
    pub diff: Option<Vec<u32>>,
    pub json: bool,
}

#[derive(Debug)]
pub struct WhyArgs {
    pub id_or_slug: String,
    pub full: bool,
}

// ─── Spec / alias management ──────────────────────────────────────────────

#[derive(Debug)]
pub enum SpecCommand {
    Show { field: String },
    Set(SpecSetArgs),
    List,
    Remove { field: String },
}

#[derive(Debug)]
pub struct SpecSetArgs {
    pub field: String,
    pub json: Option<String>,
    pub default_verb: Option<String>,
    pub states: Vec<String>,
    pub color: Option<String>,
    pub state_colors: Vec<String>,
    pub id_template: Option<String>,
    pub description: Option<String>,
    pub unit: Option<String>,
    pub track: bool,
    pub ignore: bool,
    pub id_keys: Vec<String>,
    pub data_keys: Vec<String>,
    pub no_autoalias: bool,
}

impl SpecSetArgs {
    pub fn fresh(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            json: None,
            default_verb: None,
            states: Vec::new(),
            color: None,
            state_colors: Vec::new(),
            id_template: None,
            description: None,
            unit: None,
            track: false,
            ignore: false,
            id_keys: Vec::new(),
            data_keys: Vec::new(),
            no_autoalias: false,
        }
    }
}

#[derive(Debug)]
pub enum AliasCommand {
    Show { name: String },
    Set { name: String, expansion: String },
    List,
    Remove { name: String },
}

// ─── Now / Track / Ignore ─────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct NowArgs {
    pub show_all: bool,
}

#[derive(Debug)]
pub struct TrackArgs {
    pub field: String,
    pub color: Option<String>,
    pub state_colors: Vec<String>,
}

#[derive(Debug)]
pub struct IgnoreArgs {
    pub field: String,
}

// ─── Grep / Count / Ids / Dayview / Check ────────────────────────────────

#[derive(Debug)]
pub struct GrepArgs {
    pub patterns: Vec<String>,
    pub field: Option<String>,
    pub output_flags: OutputFlags,
}

#[derive(Debug, Default)]
pub struct CountArgs {
    pub field: Option<String>,
    pub group_by: Option<String>,
    pub since: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct IdsArgs {
    pub field: Option<String>,
    pub limit: Option<usize>,
    pub oldest_first: bool,
    pub since: Option<String>,
    pub slug: bool,
    pub json: bool,
}

#[derive(Debug)]
pub enum MigrateCommand {
    Run,
    Status,
    Edit,
    Import(MigrateImportArgs),
}

#[derive(Debug, Default)]
pub struct MigrateImportArgs {
    pub backup_file: Option<String>,
    pub backup_file_positional: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug)]
pub struct BackupArgs {
    pub path: String,
    pub overwrite: bool,
}

#[derive(Debug)]
pub struct RestoreArgs {
    pub path: String,
    pub force: bool,
    pub dry_run: bool,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct CheckArgs {
    pub json: bool,
}

#[derive(Debug)]
pub struct DayviewArgs {
    pub date: Option<String>,
    pub output: Option<String>,
    pub json: bool,
    pub width: f64,
    pub height: f64,
}

impl DayviewArgs {
    pub fn fresh() -> Self {
        Self {
            date: None,
            output: None,
            json: false,
            width: 2000.0,
            height: 200.0,
        }
    }
}
