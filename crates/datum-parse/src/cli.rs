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

use crate::input::{DurationArgs, Matcher, OutputArgs, TimeArgs};

#[derive(Debug)]
pub struct Cli {
    pub db: Option<String>,
    pub command: Command,
}

#[derive(Debug)]
pub enum Command {
    /// No subcommand given — drop into the interactive REPL. Carries no
    /// args because every global flag (`--db`, etc.) already lives on
    /// the outer [`Cli`]. Reached when argv has only global flags (or is
    /// empty after argv[0]).
    Repl,
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

    /// `datum meta <field> <text>` — record an out-of-band protocol
    /// note about how a field is being classified or interpreted.
    /// `datum meta show <field>` lists every note for a field in
    /// `create_time` order. Notes are stored as timeless entries on
    /// the field with `extra_metadata.meta = true`.
    Meta(MetaCommand),

    /// `datum flag <field> [comment] [-t/-d/-y/-q]` — drop a manual
    /// review-later marker on a field at a time point. Stored as a
    /// regular entry carrying `extra_metadata.flag = true` so it
    /// surfaces in `datum check`'s pending-review sweep while
    /// staying out of tail / dayview / now (per da-dnnn Option A).
    Flag(FlagArgs),

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

    /// `dm reparse` — replay each entry's stored `raw_input` through the
    /// current parser, reporting (or fixing with `--fix`) drift between
    /// what the parser produces today vs. the persisted columns.
    Reparse(ReparseArgs),
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
    /// `--cluster <name>` (da-3fnd): explicitly bind this DB to a named
    /// sync cluster instead of the file-name default. Used to merge two
    /// differently-named DBs onto one cluster, or to split a DB off.
    pub cluster: Option<String>,
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

/// `datum reparse <hid>...` — replay one or more named entries' stored
/// `raw_input` through the current parser, diffing the result against
/// the stored columns.
///
/// Closes the persisted-intent loop (da-g5uk): the insert path stamps
/// `raw_input` + `alias_chain` + `parser_version` on every row; this
/// command is the consumer that lets a parser-bug fix retroactively
/// correct already-stored rows. Replay is hermetic to the alias state
/// at write time — `alias_chain` is rehydrated as the resolver, never
/// the live `aliases` table.
///
/// Per da-bkak: reparse operates ONLY on entries the user explicitly
/// names by hid. Walking all entries is out of scope — long-standing
/// data validated by the user is sacrosanct, never bulk-rewritten.
#[derive(Debug, Default)]
pub struct ReparseArgs {
    /// Hids (or slugs / quickIds) of the entries to re-parse. At least
    /// one is required; reparse never walks the entry log implicitly.
    pub hids: Vec<String>,
    /// UPDATE rows where the parser produced a different shape; bumps
    /// `parser_version` and recomputes `content_hash`. Default is dry-run.
    pub fix: bool,
}

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
    /// `--cluster` (da-ws1q sub-5 of multi-DB epic): after the local
    /// purge succeeds, also bump the sync cluster's generation on the
    /// relay so peers on the old generation get
    /// `GenerationMismatchError` on their next sync and know to re-init
    /// instead of re-pushing the wiped state. No-op when the local DB
    /// has never synced (no `sync_state.cluster_id` recorded).
    pub cluster: bool,
}

#[derive(Debug, Default)]
pub struct DbImportArgs {
    pub backup_file: Option<String>,
    pub backup_file_positional: Option<String>,
    pub dry_run: bool,
    /// `--skip-log <path>` — write the full list of skipped docs (with
    /// reason and original id) to this file for triage. The summary always
    /// reports a reason-grouped count; this flag persists the detail.
    pub skip_log: Option<String>,
}

// ─── Logging commands (occur/start/stop/switch/add) ──────────────────────

/// The shared arg bag for all logging verbs. Composed shape (da-e2e
/// Phase A): timing/duration/output flags live in their concern bags so
/// callers can call `args.time.build()`, `args.duration.build()`,
/// `args.output.resolve()` directly. Log-specific extras (unit, keys,
/// slug_keys, conflict, …) remain as direct fields since no shared
/// concern module owns them.
#[derive(Debug, Default)]
pub struct LogArgs {
    pub field_or_verb: String,
    pub rest: Vec<String>,
    /// `-y/-q/-d/-t/-z/-T/-D` — composed timing concern.
    pub time: TimeArgs,
    /// `--dur` / `-m/--moment` — composed duration concern.
    pub duration: DurationArgs,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
    /// `-u/--undo` — delete the most recent live entry that matches this
    /// command (same field + state). Re-creates original datum's undo
    /// semantics so the user's `uu` muscle-memory alias (re-run last
    /// command with `-u` appended) reverses the prior insert.
    pub undo: bool,
    pub keys: Vec<String>,
    pub slug_keys: Vec<String>,
    /// `-c` / `--comment`. Repeatable per datum's argparse shape: a single
    /// flag yields `extra_data.comment = "x"` (string); two or more yield
    /// `extra_data.comment = [...]` (array). Joined into the entry by
    /// [`datum_parse::input::data_args::handle_data_args`].
    pub comment: Vec<String>,
    /// `-R` / `--remainder`: route un-keyed positionals to `<field>` in
    /// `extra_data` instead of the primary `value` column. Mirrors datum's
    /// remainder-bucket override.
    pub remainder: Option<String>,
    /// `-S` / `--string-remainder`: join the remainder positionals into
    /// one space-separated string instead of an array (or, when no `-R`
    /// is supplied, into the primary value).
    pub string_remainder: bool,
    /// `-C` / `--comment-remainder`: shorthand for `-SR comment` —
    /// remainder positionals join into a single string and land in
    /// `extra_data.comment`.
    pub comment_remainder: bool,
    pub id: Option<String>,
    /// `--id-delimiter` raw CLI value. `None` means "fall through to env /
    /// `datumrc.toml` / built-in default" at the slug-interpolation site;
    /// `Some(_)` is the user's per-invocation override.
    pub id_delimiter: Option<String>,
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
    /// `--dry-run` / `--dryrun` — compute the full effect of the write
    /// (build the would-be Entry, including slug derivation, field
    /// interpolation, conflict detection) and print a preview without
    /// touching the database.
    pub dry_run: bool,
}

impl LogArgs {
    /// Start from `Default` with `field_or_verb` pre-set — the one field
    /// every logging caller supplies up front.
    pub fn with_field(field_or_verb: impl Into<String>) -> Self {
        Self {
            field_or_verb: field_or_verb.into(),
            ..Self::default()
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
    /// Zero, one, or many field names. Comma-separated on the CLI:
    /// `datum tail mood,project` parses to `["mood", "project"]`. Empty = no
    /// filter. Multi-field rows interleave by occur_time in render.
    pub fields: Vec<String>,
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
    /// `field:state` form (da-plbb): when set, `tail` renders state-blocks
    /// (start/end/dur) for periods where `fields[0]` was in the matching
    /// state. Parsed from the positional by splitting on the first `:`
    /// and then on `,` to form the parallel-state array. Mutually
    /// exclusive with the comma-separated multi-field form — when the
    /// positional contains `:`, the whole token is treated as a
    /// state-filter, not as a field list. Stored as raw strings;
    /// normalization happens in the command layer at compare time.
    pub state_filter: Option<Vec<String>>,
    /// `--show-meta`: include `extra_metadata.flag = true` (da-dnnn) and
    /// `extra_metadata.meta = true` (da-xn8j) rows. Default is to hide
    /// them — they're review markers / out-of-band notes, not tracked
    /// data. See da-ghlx.
    pub show_meta: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

impl Default for TailArgs {
    fn default() -> Self {
        Self {
            fields: Vec::new(),
            state_filter: None,
            count: None,
            metric: "hybrid".to_string(),
            reverse: false,
            columns: Vec::new(),
            time: TimeArgs::default(),
            oldest_first: false,
            watch: None,
            table: false,
            show_meta: false,
            output: OutputArgs::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct GetArgs {
    /// Optional. When omitted, callers default to the `_LAST` quickId
    /// token (resolves against the last-accessed pointer). Repurposed
    /// as the `<field>` positional in search mode (`matchers` non-empty).
    pub id_or_slug: Option<String>,
    pub pretty: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
    /// da-yhh1: matcher tokens (`k=v`, `k>v`, `k=gte=v`, …) supplied
    /// after the positional. When non-empty the dispatcher routes to
    /// the search path; when empty the existing quickId resolve runs.
    pub matchers: Vec<Matcher>,
}

impl GetArgs {
    /// Start from `Default` with the positional id/slug pre-set. The bare
    /// `datum get` form uses `GetArgs::default()` directly.
    pub fn with_id(id_or_slug: impl Into<String>) -> Self {
        Self {
            id_or_slug: Some(id_or_slug.into()),
            ..Self::default()
        }
    }
}

// ─── Mutation (update / merge / rekey) ────────────────────────────────────

#[derive(Debug, Default)]
pub struct UpdateArgs {
    /// Optional. When omitted, callers default to the
    /// `_LAST_WITH_PROTECTION` quickId token (resolves against the
    /// last-accessed pointer, refusing if it's > 15 min stale).
    pub id_or_slug: Option<String>,
    pub rest: Vec<String>,
    pub strategy: Option<ConflictStrategyArg>,
    pub id: Option<String>,
    /// `--id-delimiter` raw CLI value. `None` falls through to env /
    /// `datumrc.toml` / built-in default at the slug-interpolation site.
    pub id_delimiter: Option<String>,
    pub keys: Vec<String>,
    /// `-c` / `--comment`. Repeatable: a single flag yields a string, two
    /// or more yield an array — see [`LogArgs::comment`].
    pub comment: Vec<String>,
    pub dry_run: bool,
    pub strict: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

#[derive(Debug, Default)]
pub struct RetimeArgs {
    /// Optional. When omitted, the command resolves against `_LAST`
    /// (matching the update/merge/delete/undo default).
    pub slug: Option<String>,
    /// ISO-8601 duration shift relative to the current occur_time.
    /// Mutually exclusive with [`bare_offset_min`] and with absolute
    /// targeting via `time` / `relative` (the parser enforces).
    pub offset: Option<String>,
    /// Time-arg group (`-t`/`-y`/`-q`/`-d`/`-z`/`-D`/`-T`). When no
    /// flag in the group is set, the command treats the request as
    /// "set to now". `relative=true` flips the reference frame to the
    /// entry's current occur_time (computed as a delta from "now").
    pub time: TimeArgs,
    pub field: Option<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    /// `-r`/`--relative`: interpret `time` as a shift relative to the
    /// entry's current occur_time rather than absolute from now.
    pub relative: bool,
    /// Bare numeric positional after the slug (e.g. `retime <id> -5`)
    /// → minutes-offset shorthand for `-r -t <N>`. Mutually exclusive
    /// with `offset`.
    pub bare_offset_min: Option<i64>,
}

#[derive(Debug)]
pub struct EditArgs {
    pub id_or_slug: String,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

#[derive(Debug, Default)]
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

#[derive(Debug)]
pub struct UndoArgs {
    pub id_or_slug: Option<String>,
    pub to_version: Option<u32>,
    pub force: bool,
    pub window: String,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

impl Default for UndoArgs {
    fn default() -> Self {
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
    /// `datum spec show <FIELD> [--text]` — JSON by default; `--text`
    /// switches to a human-readable summary including per-state colors
    /// and descriptions (da-nhzo).
    Show {
        field: String,
        text: bool,
    },
    Set(Box<SpecSetArgs>),
    List {
        tag: Option<String>,
    },
    Remove {
        field: String,
    },
}

#[derive(Debug, Default)]
pub struct SpecSetArgs {
    pub field: String,
    pub json: Option<String>,
    pub default_verb: Option<String>,
    pub states: Vec<String>,
    pub color: Option<String>,
    pub state_colors: Vec<String>,
    /// Per-state descriptions in `STATE=TEXT` form (da-zwqk). Mirrors
    /// `state_colors`. Empty by default.
    pub state_descriptions: Vec<String>,
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
    /// `--strict` / `--no-strict`. `Some(true)` locks the spec down (the
    /// `orphaned_extra_data_key` check fires for undeclared keys);
    /// `Some(false)` clears strictness; `None` leaves the existing value.
    pub strict: Option<bool>,
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
    /// `-A` / `--all` — include ignored fields. Layer 3 in the
    /// disclosure ladder (da-84cg): everything that exists. Implies
    /// `--tracked` (the ladder is concentric).
    pub show_all: bool,
    /// `-a` / `--tracked` — also surface tracked fields whose current
    /// state is falsy. Layer 2 in the disclosure ladder; default is
    /// layer 1 (active-only). Rebound from `-t` to `-a` in da-tx8j so
    /// the standard `-t TIME` time_args flag can fly here.
    pub show_tracked: bool,
    /// `--json` — emit the [`NowSnapshot`](datum_core::db::NowSnapshot)
    /// wire schema instead of the terminal view. Other surfaces (web,
    /// wear, voice) consume this; the schema is versioned via
    /// `datum_core::db::NOW_SCHEMA_ID`.
    pub json: bool,
    /// `--bare` — suppress the trailing terminal nowview band; only show
    /// the active-state list. Useful for piping or scripting.
    pub bare: bool,
    /// Force-disable color in the terminal nowview band. Mirrors
    /// `NO_COLOR` env var behavior; useful for redirected output.
    pub no_color: bool,
    /// Override terminal width in cells for the nowview band. None →
    /// auto-detect from `$COLUMNS`, fall back to 80.
    pub cols: Option<u16>,
    /// `-t/-d/-y/-q/-z` — composed timing concern (da-tx8j). When any
    /// of these are set, `datum now` shows the state-at-that-point
    /// instead of the live wall-clock state, and suppresses the
    /// terminal nowview band (which is anchored to the *real* now).
    pub time: TimeArgs,
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

// ─── Meta (da-xn8j) ───────────────────────────────────────────────────────

/// `datum meta` subcommand surface — either add a new note or list
/// the notes already attached to a field.
#[derive(Debug)]
pub enum MetaCommand {
    /// `datum meta <field> <text...>` — append a protocol-change note
    /// to `<field>`. Positional tail joins on spaces.
    Add { field: String, text: String },
    /// `datum meta show <field>` — list every note attached to
    /// `<field>` in `create_time` order.
    Show { field: String },
}

// ─── Flag (da-dnnn) ───────────────────────────────────────────────────────

/// `datum flag <field> [comment]` — drop a review-later marker. The
/// standard time-flag group (`-t`/`-d`/`-y`/`-q`/`-z`) places the
/// flag at a specific moment, including back-dated. The optional
/// comment becomes the flag's payload.
#[derive(Debug, Default)]
pub struct FlagArgs {
    pub field: String,
    /// Optional payload — what the user wants to review. Empty
    /// string when the bare `datum flag <field>` form is used (the
    /// "just look here later" marker).
    pub comment: String,
    /// `-y/-q/-d/-t/-z` — the timing concern. `-T` and `-D` are
    /// intentionally not exposed (a flag without a moment loses its
    /// point).
    pub time: TimeArgs,
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
    /// `--cluster` (da-ws1q sub-5): after the restore succeeds, bump
    /// the sync cluster's generation on the relay so peers see a
    /// `GenerationMismatchError` instead of re-pushing their stale
    /// state on top of the restored snapshot. No-op when the local DB
    /// has no recorded `sync_state.cluster_id`, when `--dry-run`, or
    /// when the restore wrote nothing (no point bumping for nothing).
    pub cluster: bool,
}

#[derive(Debug, Default)]
pub struct CheckArgs {
    pub json: bool,
    /// Positional list of checker names to run. When empty, the default
    /// battery runs (every known checker). When non-empty, only the named
    /// checkers run. Known names: see [`KNOWN_CHECKERS`].
    pub checkers: Vec<String>,
    /// `--fix` — rewrite drifted `verb` columns to match `derive_verb`.
    /// For the `aliases` checker, annotates state-name collisions with the
    /// explicit first-saved-wins resolution (no DB mutation — the
    /// precedence is already deterministic in [`SpecAliasIndex`]).
    pub fix: bool,
    /// `--list` — print the set of known checker names and exit without
    /// running any checks.
    pub list: bool,
}

/// Checker names accepted as `dm check <NAME>...` positionals. Kept in the
/// parse crate so both the parser (typo validation) and the command
/// dispatcher (which-categories-to-run) read from the same source.
pub const KNOWN_CHECKERS: &[&str] = &["verbs", "aliases", "slugs", "integrity", "flags"];

/// Render orientation for `datum dayview`, named by time-axis direction.
/// Mirrors [`datum_layout::dayview::Orientation`] so callers don't need a
/// layout dep just to construct args.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DayviewOrientation {
    /// Time runs vertically (top→bottom); days stack horizontally — each
    /// day is a column.
    Vertical,
    /// Time runs horizontally (left→right); days stack vertically. Datum's
    /// original layout and the default per bead da-tbe.
    #[default]
    Horizontal,
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
    /// `--width` — total SVG width in pixels. Default: 2000 (horizontal) or
    /// derived from day-stack (vertical).
    pub width: Option<f64>,
    /// `--height` — total SVG height in pixels. None → auto from
    /// `day_height + n_days` (horizontal) or default time extent (vertical).
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
    /// `--vertical`), named by time-axis direction. Defaults to horizontal
    /// per bead da-tbe.
    pub orientation: DayviewOrientation,
}

impl Default for DayviewArgs {
    fn default() -> Self {
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

/// `datum nowview` arguments. Per bead da-6jy nowview shares dayview's
/// render path scoped to the last `history` minutes (default 30m); there
/// is no separate "now panel" anymore so `--now-width` was dropped.
#[derive(Debug)]
pub struct NowviewArgs {
    /// History duration string ("15m", "1h", "2h"). None → 30m default.
    pub history: Option<String>,
    /// SVG width in pixels. None → scale from `history` (datum behavior).
    pub width: Option<f64>,
    /// SVG height in pixels. Default 300.
    pub height: f64,
    /// Margin around the plot. Default 2.
    pub margin: f64,
    /// Pixel height of the time-axis strip. Default 15.
    pub time_axis_height: f64,
    /// Output file. Extension picks format: .svg / .png / .html. None →
    /// terminal output (per da-57dj), routed through the
    /// `nowview_terminal` visual band-stack renderer.
    pub output_file: Option<String>,
    /// Watch mode: redraw on DB change (≈500ms) plus a 60s periodic
    /// refresh so the clock keeps advancing on an idle DB.
    pub watch: bool,
    // Terminal-mode controls (only consulted when `output_file` is None).
    // Per da-57dj `dm nowview` defaults to rendering a pure visual band
    // stack in the terminal — no field/state labels, only colored bands
    // plus an `hh:mm` footer.
    /// Time window for terminal Flow mode. None → vertical Snapshot mode
    /// (current state only). `Some("30m")` → horizontal flow across the
    /// last 30 minutes with a dashed yellow now-marker at the right edge.
    pub window: Option<String>,
    /// Override terminal width in cells. None → auto-detect via `$COLUMNS`.
    pub cols: Option<u16>,
    /// Override terminal height in rows. None → auto-detect via `$LINES`.
    pub rows: Option<u16>,
    /// Force-disable ANSI color in the terminal render. Mirrors `NO_COLOR`.
    pub no_color: bool,
}

impl Default for NowviewArgs {
    fn default() -> Self {
        Self {
            history: None,
            width: None,
            height: 300.0,
            margin: 2.0,
            // da-34w: bumped from 15 → 22 so the time-axis label font
            // (`time_axis_height - 6`, capped) lands at ~16pt instead of
            // ~9pt. The previous default rendered too small to read.
            time_axis_height: 22.0,
            output_file: None,
            watch: false,
            window: None,
            cols: None,
            rows: None,
            no_color: false,
        }
    }
}

/// Output format for `datum view <name>`. `Auto` resolves at dispatch
/// time: pretty when stdout is a TTY, tsv when piped (matching the
/// slice 4 bead's "pretty when tty, tsv when piped" rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewFormat {
    #[default]
    Auto,
    Pretty,
    Tsv,
    Csv,
    Json,
}

/// `datum view <name>` arguments.
#[derive(Debug, Default)]
pub struct ViewArgs {
    /// View name, looked up in the `views` table.
    pub name: String,
    /// `--format pretty|tsv|csv|json`. Defaults to [`ViewFormat::Auto`].
    pub format: ViewFormat,
    /// `--watch` — re-run the query on a poll interval until interrupted.
    /// Slice 4 polls; the daemon-hook integration is a follow-up.
    pub watch: bool,
    /// `--list` — print the set of installed view names and exit. Mirrors
    /// `datum check --list` (da-79vv: discoverability gap — view names
    /// otherwise live only inside kit TOML).
    pub list: bool,
}
