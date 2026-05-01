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

use crate::input::{DurationArgs, OutputArgs, TimeArgs};

/// Legacy flat output-flags shape, kept as a transitional shim for the
/// per-command Args structs that haven't yet adopted the composed
/// [`OutputArgs`] field. Phase A (da-e2e) migrated `LogArgs`; Phase C
/// (da-6qj) migrated the mutate commands (Update/Edit/Delete/Undo);
/// Phase B (da-ngq) migrated the read commands (`TailArgs` / `GetArgs`
/// / `GrepArgs`). Phase D retires this for the remaining spec/alias
/// commands.
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
    Kit(KitCommand),

    Track(TrackArgs),
    Ignore(IgnoreArgs),

    Grep(GrepArgs),
    Check(CheckArgs),

    Count(CountArgs),
    Ids(IdsArgs),

    Dayview(DayviewArgs),
    Nowview(NowviewArgs),
    /// `datum view <name>` — execute a named SELECT installed via a kit
    /// (or declared by the user) and render the rows. Slice 4 (da-d4t.4).
    View(ViewArgs),

    Info(InfoArgs),
    /// `datum db <subcommand>` — DB-lifecycle group. Houses every database
    /// administration verb: `setup`, `purge`, `backup`, `restore`,
    /// `migrate`, `import` (da-33g consolidated the standalone verbs).
    Db(DbCommand),

    Config(ConfigCommand),

    Sync(SyncArgs),

    Conflicts(ConflictsCommand),
}

// ─── Conflicts ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ConflictsCommand {
    List(ConflictsListArgs),
    Show(ConflictsShowArgs),
    Resolve(ConflictsResolveArgs),
    AutoResolve(ConflictsAutoResolveArgs),
}

#[derive(Debug, Default)]
pub struct ConflictsListArgs {
    pub kind: Option<String>,
    pub all: bool,
}

#[derive(Debug)]
pub struct ConflictsShowArgs {
    pub hid: String,
}

/// A single resolve invocation may carry exactly one of: `--keep both`
/// (legacy), `--close-older` (legacy), `--take local|remote`, `--edit`,
/// `--skip`. The parser enforces mutual exclusion; the dispatcher maps
/// them to [`datum_core::conflicts::Resolution`].
#[derive(Debug)]
pub struct ConflictsResolveArgs {
    pub hid: String,
    pub keep: Option<String>,
    pub close_older: bool,
    pub take: Option<String>,
    pub edit: bool,
    pub skip: bool,
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

#[derive(Debug, Default)]
pub struct SetupArgs {}

// ─── Db (subcommand group) ────────────────────────────────────────────────

#[derive(Debug)]
pub enum DbCommand {
    /// `datum db setup` — initialize or migrate the active DB schema.
    Setup(SetupArgs),
    /// `datum db purge` — drop every datum table and rebuild from scratch.
    Purge(DbPurgeArgs),
    /// `datum db backup` — dump live entries to JSONL or tar.gz.
    Backup(BackupArgs),
    /// `datum db restore` — replay entries from a backup file.
    Restore(RestoreArgs),
    /// `datum db migrate` — schema migration runner (run/status/edit).
    Migrate(MigrateCommand),
    /// `datum db import` — load a legacy datum CouchDB backup JSON.
    Import(DbImportArgs),
}

#[derive(Debug, Default)]
pub struct DbPurgeArgs {
    /// `--yes` / `-y` — skip the type-the-database-path confirmation prompt
    /// and purge non-interactively. Required when stdin is not a TTY.
    pub yes: bool,
}

#[derive(Debug, Default)]
pub struct DbImportArgs {
    pub backup_file: Option<String>,
    pub backup_file_positional: Option<String>,
    pub dry_run: bool,
}

// ─── Logging commands (occur/start/stop/switch/add) ──────────────────────

/// The shared arg bag for all logging verbs. Composed shape (da-e2e
/// Phase A): timing/duration/output flags live in their concern bags so
/// callers can call `args.time.build()`, `args.duration.build()`,
/// `args.output.resolve()` directly. Log-specific extras (unit, keys,
/// slug_keys, conflict, …) remain as direct fields since no shared
/// concern module owns them.
#[derive(Debug)]
pub struct LogArgs {
    pub field_or_verb: String,
    pub rest: Vec<String>,
    /// `-y/-q/-d/-t/-z/-T` — composed timing concern. `-D/--full-day` is
    /// not exposed for logging verbs; the parser strips that spec.
    pub time: TimeArgs,
    /// `--dur` / `-m/--moment` — composed duration concern.
    pub duration: DurationArgs,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
    pub unit: Option<String>,
    pub keys: Vec<String>,
    pub slug_keys: Vec<String>,
    /// `-c` / `--comment`. Repeatable per datum's argparse shape: a single
    /// flag yields `extra_data.comment = "x"` (string); two or more yield
    /// `extra_data.comment = [...]` (array). Joined into the entry by
    /// [`datum_parse::input::data_args::handle_data_args`].
    pub comment: Vec<String>,
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
    pub conflict: Option<ConflictStrategyArg>,
    pub update: bool,
    pub merge: bool,
    pub no_metadata: bool,
    /// `--alias <NAME>` — when set, after the command succeeds, persist
    /// `<NAME> -> <canonical-form>` to the alias table. The canonical
    /// form is the original argv with the alias-naming flag (and any
    /// `--alias-overwrite`) stripped.
    pub alias_name: Option<String>,
    /// `--alias-overwrite` — companion to `--alias`. Allows replacing an
    /// existing alias of the same name; without it, a name collision is
    /// an error.
    pub alias_overwrite: bool,
}

impl LogArgs {
    pub fn fresh(field_or_verb: impl Into<String>) -> Self {
        Self {
            field_or_verb: field_or_verb.into(),
            rest: Vec::new(),
            time: TimeArgs::default(),
            duration: DurationArgs::default(),
            output: OutputArgs::default(),
            unit: None,
            keys: Vec::new(),
            slug_keys: Vec::new(),
            comment: Vec::new(),
            id: None,
            id_delimiter: ":".to_string(),
            last_state: None,
            conflict: None,
            update: false,
            merge: false,
            no_metadata: false,
            alias_name: None,
            alias_overwrite: false,
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
    /// `-y/-q/-d/-t` — composed timing concern. Tail does not expose
    /// `-z/-T/-D`; the parser strips those specs when building the
    /// registry.
    pub time: TimeArgs,
    /// Take the first N (oldest) instead of the last N (newest). Powers
    /// `datum head`, which routes through tail with this flag set.
    pub oldest_first: bool,
    pub watch: Option<f64>,
    /// `--table`: force the legacy columnar view (time | field | state | dur |
    /// hid). Default is the info-line renderer matching `show_create` /
    /// `show_update`. Auto-enabled when `--column` or a non-hybrid `--metric`
    /// is set — those are tabular concerns.
    pub table: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

impl TailArgs {
    pub fn fresh() -> Self {
        Self {
            field: None,
            count: None,
            metric: "hybrid".to_string(),
            reverse: false,
            columns: Vec::new(),
            time: TimeArgs::default(),
            oldest_first: false,
            watch: None,
            table: false,
            output: OutputArgs::default(),
        }
    }
}

#[derive(Debug)]
pub struct GetArgs {
    /// Optional. When omitted, callers default to the `_LAST` quickId
    /// token (resolves against the last-accessed pointer).
    pub id_or_slug: Option<String>,
    pub pretty: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

impl GetArgs {
    pub fn fresh(id_or_slug: impl Into<String>) -> Self {
        Self {
            id_or_slug: Some(id_or_slug.into()),
            pretty: false,
            output: OutputArgs::default(),
        }
    }

    /// Constructor for the bare `datum get` form (no positional).
    pub fn fresh_default() -> Self {
        Self {
            id_or_slug: None,
            pretty: false,
            output: OutputArgs::default(),
        }
    }
}

// ─── Mutation (update / merge / rekey) ────────────────────────────────────

#[derive(Debug)]
pub struct UpdateArgs {
    /// Optional. When omitted, callers default to the
    /// `_LAST_WITH_PROTECTION` quickId token (resolves against the
    /// last-accessed pointer, refusing if it's > 15 min stale).
    pub id_or_slug: Option<String>,
    pub rest: Vec<String>,
    pub strategy: Option<ConflictStrategyArg>,
    pub id: Option<String>,
    pub id_delimiter: String,
    pub keys: Vec<String>,
    /// `-c` / `--comment`. Repeatable: a single flag yields a string, two
    /// or more yield an array — see [`LogArgs::comment`].
    pub comment: Vec<String>,
    pub dry_run: bool,
    pub strict: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

impl UpdateArgs {
    pub fn fresh(id_or_slug: impl Into<String>) -> Self {
        Self {
            id_or_slug: Some(id_or_slug.into()),
            rest: Vec::new(),
            strategy: None,
            id: None,
            id_delimiter: ":".to_string(),
            keys: Vec::new(),
            comment: Vec::new(),
            dry_run: false,
            strict: false,
            output: OutputArgs::default(),
        }
    }

    /// Constructor for the bare `datum update` form (no positional).
    pub fn fresh_default() -> Self {
        Self {
            id_or_slug: None,
            rest: Vec::new(),
            strategy: None,
            id: None,
            id_delimiter: ":".to_string(),
            keys: Vec::new(),
            comment: Vec::new(),
            dry_run: false,
            strict: false,
            output: OutputArgs::default(),
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
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

#[derive(Debug)]
pub struct DeleteArgs {
    /// Optional. When omitted, callers default to the
    /// `_LAST_WITH_PROTECTION` quickId token (resolves against the
    /// last-accessed pointer, refusing if it's > 15 min stale).
    pub id_or_slug: Option<String>,
    pub force: bool,
    pub dry_run: bool,
    pub strict: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

impl DeleteArgs {
    pub fn fresh(id_or_slug: impl Into<String>) -> Self {
        Self {
            id_or_slug: Some(id_or_slug.into()),
            force: false,
            dry_run: false,
            strict: false,
            output: OutputArgs::default(),
        }
    }

    pub fn fresh_default() -> Self {
        Self {
            id_or_slug: None,
            force: false,
            dry_run: false,
            strict: false,
            output: OutputArgs::default(),
        }
    }
}

#[derive(Debug)]
pub struct UndoArgs {
    pub id_or_slug: Option<String>,
    pub to_version: Option<u32>,
    pub force: bool,
    pub window: String,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

impl UndoArgs {
    pub fn fresh() -> Self {
        Self {
            id_or_slug: None,
            to_version: None,
            force: false,
            window: "15m".to_string(),
            output: OutputArgs::default(),
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
    List { tag: Option<String> },
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
    /// Vertical position of the field on the dayview/nowview, fraction
    /// `[0.0, 1.0]`. `None` here means "leave as-is in the existing spec";
    /// to clear an explicit value, the user must currently re-set the spec
    /// from JSON. Default fallback is the `md5(field)` hash.
    pub y: Option<f64>,
    /// Vertical row height as a fraction of the dayview's data area.
    /// `None` here means "leave as-is"; default fallback is `0.12`.
    pub height: Option<f64>,
    /// Stacking order. Higher values render on top. `None` here means
    /// "leave as-is"; default fallback is the resolved `y`.
    pub z_index: Option<f64>,
    /// Replace the spec's `tags` list with this. Empty (`Vec::is_empty`) is
    /// treated as "leave as-is" — to clear tags, pass `--json '...tags=[]'`.
    pub tags: Vec<String>,
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
            y: None,
            height: None,
            z_index: None,
            tags: Vec::new(),
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

// ─── Kit (da-d4t.1) ────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum KitCommand {
    /// `datum kit install <path>` — parse a kit TOML and upsert its field
    /// specs. Existing rows for the same field win unless `--force`.
    Install(KitInstallArgs),
    /// `datum kit export <name> [path]` — serialize the kit's installed
    /// rows back to TOML. None path → stdout.
    Export(KitExportArgs),
    /// `datum kit list` — installed kits + per-kit field counts.
    List,
    /// `datum kit uninstall <name>` — remove every row tagged with this
    /// kit's id; user-declared rows untouched.
    Uninstall(KitUninstallArgs),
}

#[derive(Debug)]
pub struct KitInstallArgs {
    pub path: String,
    /// When true, conflicting field rows are overwritten and re-tagged
    /// with this kit's id.
    pub force: bool,
    /// When true, auto-approve the index trust prompt (used for
    /// non-interactive shells / CI). Without `--yes`, the dispatcher
    /// prints the proposed indices and reads y/N from stdin.
    pub assume_yes: bool,
}

#[derive(Debug)]
pub struct KitExportArgs {
    pub name: String,
    /// Output file. None → write to stdout.
    pub path: Option<String>,
}

#[derive(Debug)]
pub struct KitUninstallArgs {
    pub name: String,
}

// ─── Now / Track / Ignore ─────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct NowArgs {
    pub show_all: bool,
    /// `--bare` — suppress the trailing terminal nowview band; only show
    /// the active-state list. Useful for piping or scripting.
    pub bare: bool,
    /// Force-disable color in the terminal nowview band. Mirrors
    /// `NO_COLOR` env var behavior; useful for redirected output.
    pub no_color: bool,
    /// Override terminal width in cells for the nowview band. None →
    /// auto-detect from `$COLUMNS`, fall back to 80.
    pub cols: Option<u16>,
}

#[derive(Debug)]
pub struct TrackArgs {
    pub field: String,
    pub color: Option<String>,
    pub state_colors: Vec<String>,
    /// Each `--tag <name>` flag adds the tag to the field's spec
    /// idempotently. Repeatable; order preserved on first-add.
    pub tags: Vec<String>,
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
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
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

/// Render orientation for `datum dayview`. Mirrors
/// [`datum_layout::dayview::Orientation`] so callers don't need a layout
/// dep just to construct args.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayviewOrientation {
    /// Time runs horizontally, days stack vertically (datum's original).
    Vertical,
    /// Time runs vertically, days stack horizontally (per bead da-a7f).
    Horizontal,
}

impl Default for DayviewOrientation {
    fn default() -> Self {
        DayviewOrientation::Horizontal
    }
}

/// `datum dayview` arguments — port of datum's `dayviewCmdArgs` (TS) plus
/// the orientation knob from bead da-a7f.
///
/// Defaults match the original. `width`, `height`, and `day_height` are
/// `Option` so the command can apply datum's auto-sizing rules at dispatch
/// time (see `dayview.ts` lines 38–78).
#[derive(Debug)]
pub struct DayviewArgs {
    /// `--start-date` — inclusive start (ISO `YYYY-MM-DD` or relative
    /// offset like `-3` or `0`). Resolved via `parse_date_arg`.
    pub start_date: Option<String>,
    /// `--end-date`/`-d` — inclusive end. Default: today.
    pub end_date: Option<String>,
    /// `--n-days`/`-n` — number of days. Default: 1 (when no other dates
    /// are given).
    pub n_days: Option<u32>,
    /// `--width` — total SVG width in pixels. Default: 2000 (vertical) or
    /// derived from day-stack (horizontal).
    pub width: Option<f64>,
    /// `--height` — total SVG height in pixels. None → auto from
    /// `day_height + n_days` (vertical) or default time extent (horizontal).
    pub height: Option<f64>,
    /// `--day-height` — per-day cross-axis extent in pixels. Default: 100.
    pub day_height: Option<f64>,
    /// `--time-axis-height` — time-axis strip thickness. Default: 15.
    pub time_axis_height: f64,
    /// `--margin` — outer margin. Default: 2.
    pub margin: f64,
    /// `-o`/`--output-file` — output path; extension picks format
    /// (.svg / .png / .html). None → SVG to stdout.
    pub output_file: Option<String>,
    /// `--json` — emit the layout JSON instead of SVG (test-only / debug
    /// hook; not in datum-TS).
    pub json: bool,
    /// `--watch`/`-w` — redraw on a 5-minute interval (poll loop;
    /// follow-up will swap to DB-change events).
    pub watch: bool,
    /// `--orientation horizontal|vertical` (or shorthand `--horizontal` /
    /// `--vertical`). Defaults to horizontal per bead da-a7f.
    pub orientation: DayviewOrientation,
}

impl DayviewArgs {
    pub fn fresh() -> Self {
        Self {
            start_date: None,
            end_date: None,
            n_days: None,
            width: None,
            height: None,
            day_height: None,
            time_axis_height: 15.0,
            margin: 2.0,
            output_file: None,
            json: false,
            watch: false,
            orientation: DayviewOrientation::default(),
        }
    }
}

/// `datum nowview` arguments — port of datum's `nowviewCmdArgs` (TS).
///
/// Defaults match the original. `width` is `None` so the command can apply
/// the history-aware default (100/200/300/400 px) at dispatch time.
#[derive(Debug, Default)]
pub struct NowviewArgs {
    /// History duration string ("15m", "1h", "2h"). Empty / None → no
    /// history panel.
    pub history: Option<String>,
    /// SVG width in pixels. None → scale from `history` (datum behavior).
    pub width: Option<f64>,
    /// SVG height in pixels. Default 300.
    pub height: f64,
    /// Width of the right "now" panel in time units (e.g. "5m").
    pub now_width: Option<String>,
    /// Margin around the plot. Default 2.
    pub margin: f64,
    /// Pixel height of the time-axis strip. Default 15.
    pub time_axis_height: f64,
    /// Output file. Extension picks format: .svg / .png / .html. None →
    /// SVG to stdout.
    pub output_file: Option<String>,
    /// Watch mode: redraw every minute (or on DB change in a future
    /// follow-up).
    pub watch: bool,
}

impl NowviewArgs {
    pub fn fresh() -> Self {
        Self {
            history: None,
            width: None,
            height: 300.0,
            now_width: None,
            margin: 2.0,
            time_axis_height: 15.0,
            output_file: None,
            watch: false,
        }
    }
}

/// Output format for `datum view <name>`. `Auto` resolves at dispatch
/// time: pretty when stdout is a TTY, tsv when piped (matching the
/// slice 4 bead's "pretty when tty, tsv when piped" rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewFormat {
    Auto,
    Pretty,
    Tsv,
    Csv,
    Json,
}

impl Default for ViewFormat {
    fn default() -> Self {
        ViewFormat::Auto
    }
}

/// `datum view <name>` arguments.
#[derive(Debug)]
pub struct ViewArgs {
    /// View name, looked up in the `views` table.
    pub name: String,
    /// `--format pretty|tsv|csv|json`. Defaults to [`ViewFormat::Auto`].
    pub format: ViewFormat,
    /// `--watch` — re-run the query on a poll interval until interrupted.
    /// Slice 4 polls; the daemon-hook integration is a follow-up.
    pub watch: bool,
}

impl ViewArgs {
    pub fn fresh(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            format: ViewFormat::default(),
            watch: false,
        }
    }
}
