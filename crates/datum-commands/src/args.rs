//! CLI argument definitions.
//!
//! Both `datum <verb> <field>` and `datum <field> <verb>` are supported —
//! the field/verb swap happens in the pre-pass before clap sees the arguments.
//!
//! Stacked short flags (-yy, -qqq) are handled by a pre-pass before clap
//! sees the arguments (see `Cli::parse_with_alias_resolution`).

use std::cell::RefCell;

use clap::{Args, Parser, Subcommand, ValueEnum};
use datum_core::db::ConflictStrategy;
use serde_json::json;

use crate::render::{OutputArgs, Show};

/// Provenance captured during the CLI pre-pass: the literal argv the user
/// typed and the chain of alias expansions (if any) that were applied. The
/// current invocation pushes one of these onto the thread-local stack; the
/// log-entry code path pops it and stamps it onto the Entry.
#[derive(Debug, Clone, Default)]
pub struct IntentContext {
    /// Literal argv joined by spaces (no shell escaping).
    pub raw_input: String,
    /// Ordered list of alias expansion steps. Each step is an object
    /// `{"name": "<alias>", "expansion": "<expanded text>"}`. Empty list →
    /// stored as None on the entry.
    pub alias_chain: Vec<AliasStep>,
}

/// A single alias expansion step, serialised as a small JSON object when
/// persisted to the entry's alias_chain column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasStep {
    pub name: String,
    pub expansion: String,
}

impl IntentContext {
    /// Convert the alias_chain into a JSON value suitable for storage in the
    /// entry's `alias_chain` column (or None if the chain is empty).
    pub fn alias_chain_json(&self) -> Option<serde_json::Value> {
        if self.alias_chain.is_empty() {
            return None;
        }
        let arr: Vec<serde_json::Value> = self
            .alias_chain
            .iter()
            .map(|step| json!({"name": step.name, "expansion": step.expansion}))
            .collect();
        Some(serde_json::Value::Array(arr))
    }
}

thread_local! {
    static INTENT_CTX: RefCell<Option<IntentContext>> = const { RefCell::new(None) };
}

/// Publish the current invocation's [`IntentContext`] to the thread-local
/// slot. Called by the CLI and test harness immediately after `expand_argv`
/// and before dispatch. Replaces any value already present.
pub fn set_intent_context(ctx: IntentContext) {
    INTENT_CTX.with(|slot| *slot.borrow_mut() = Some(ctx));
}

/// Take and clear the current [`IntentContext`]. Called by `log_entry` once
/// per entry-write so the same context isn't accidentally reused.
pub fn take_intent_context() -> Option<IntentContext> {
    INTENT_CTX.with(|slot| slot.borrow_mut().take())
}

/// Peek without consuming. Used by report-style commands (e.g. `datum why`
/// when diagnosing a problem in-flight) and by tests.
pub fn peek_intent_context() -> Option<IntentContext> {
    INTENT_CTX.with(|slot| slot.borrow().clone())
}

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
    /// Target database: name (resolves via registry) or path
    /// (contains `/`, starts with `.`, or ends in `.db`). Overrides `DATUM_DB`.
    #[arg(long = "db", global = true, value_name = "NAME_OR_PATH")]
    pub db: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    /// Pre-process argv:
    ///   1. Capture raw argv into the thread-local intent context
    ///   2. Expand aliases (look up first arg in the DB alias table) and
    ///      record each expansion step on the context
    ///   3. Apply terminal-verb override (`<field> end` → `end <field>`)
    ///   4. Swap field/verb if field comes before verb
    ///   5. Expand stacked flags like -yy → -y -y, -qqq → -q -q -q
    ///   6. Parse with clap
    pub fn parse_with_alias_resolution() -> anyhow::Result<Self> {
        let raw: Vec<String> = std::env::args().collect();
        let expanded = expand_argv(raw)?;
        Ok(Cli::parse_from(expanded))
    }

    /// Parse a raw argv vector through clap without alias resolution or the
    /// pre-passes. Test-only helper that lets integration tests verify the
    /// CLI surface without depending on `clap` directly.
    pub fn try_parse_argv<I, T>(argv: I) -> clap::error::Result<Self>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        <Self as Parser>::try_parse_from(argv)
    }
}

/// Known subcommands — keep in sync with Command enum.
///
/// `note` is intentionally NOT a known subcommand: it's a seeded alias that
/// rewrites to `add` in the alias pre-pass. Treating it as unknown here makes
/// the alias lookup fire.
const KNOWN_SUBCOMMANDS: &[&str] = &[
    "occur", "start", "stop", "end", "switch", "add",
    "tail", "head", "now", "spec", "alias", "migrate", "get", "delete", "del", "undo", "help",
    "history", "why",
    "track", "ignore", "grep", "check", "map", "reduce", "red",
    "update", "merge", "rekey", "edit", "retime", "rt",
    "backup", "restore", "dayview",
    "info", "setup", "config", "sync",
];

/// Terminal verb tokens the override pre-pass recognises. `end`/`stop` are
/// synonymous — `<field> end` and `<field> stop` both rewrite to `end <field>`.
const TERMINAL_VERBS: &[&str] = &["end", "stop"];

/// Expand argv: capture intent context, apply terminal-verb override, resolve
/// aliases, swap field/verb order, and expand stacked flags.
///
/// Terminal-verb override runs BEFORE alias expansion so that `datum sleep end`
/// rewrites to `datum end sleep` cleanly even when `sleep` has a bare-field
/// alias (which would otherwise expand `sleep` → `start sleep` first and leave
/// `end` dangling).
pub fn expand_argv(argv: Vec<String>) -> anyhow::Result<Vec<String>> {
    let raw_input = argv.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
    let mut ctx = IntentContext { raw_input, alias_chain: Vec::new() };

    let argv = apply_terminal_verb_override(argv);
    let argv = try_expand_alias(argv, &mut ctx);
    let argv = maybe_swap_field_verb(argv);
    let argv = expand_stacked_flags(argv);

    set_intent_context(ctx);
    Ok(argv)
}

/// Like [`expand_argv`] but uses the provided `db` for alias lookup instead of
/// opening one from `config::db_path()`. Lets in-process test harnesses drive
/// the full arg-parse path against an in-memory Db.
pub fn expand_argv_with_db(
    argv: Vec<String>,
    db: &datum_core::db::Db,
) -> anyhow::Result<Vec<String>> {
    let raw_input = argv.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
    let mut ctx = IntentContext { raw_input, alias_chain: Vec::new() };

    let argv = apply_terminal_verb_override(argv);
    let argv = try_expand_alias_with_db(argv, db, &mut ctx);
    let argv = maybe_swap_field_verb(argv);
    let argv = expand_stacked_flags(argv);

    set_intent_context(ctx);
    Ok(argv)
}

/// Rewrite `datum <field> end [args...]` → `datum end <field> [args...]`.
/// Only triggers when the second token is a recognised terminal verb and
/// the first is not itself a known subcommand or verb — so `datum end mood`
/// (already verb-first) and `datum stop field=value` (third token is a kv
/// pair, not relevant here) are untouched.
fn apply_terminal_verb_override(mut argv: Vec<String>) -> Vec<String> {
    if argv.len() < 3 {
        return argv;
    }
    let terminal_is_second = TERMINAL_VERBS.contains(&argv[2].as_str());
    let first_is_known_subcommand = KNOWN_SUBCOMMANDS.contains(&argv[1].as_str());
    if terminal_is_second && !first_is_known_subcommand {
        // datum <field> end → datum end <field>
        argv.swap(1, 2);
    }
    argv
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

/// Maximum number of alias expansion hops before we give up (to guard
/// against user-defined cycles like `a → b`, `b → a`).
const MAX_ALIAS_HOPS: usize = 16;

/// Try to resolve argv[1] as an alias from the database, recording each
/// expansion step onto `ctx`. On any failure (DB missing, alias not found,
/// parse error) returns argv unchanged.
fn try_expand_alias(argv: Vec<String>, ctx: &mut IntentContext) -> Vec<String> {
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
        Some(db) => try_expand_alias_with_db(argv, &db, ctx),
        None => argv,
    }
}

/// Alias-expansion variant that uses the given `db` for lookup. Follows
/// alias chains up to [`MAX_ALIAS_HOPS`] steps, appending each expansion
/// onto `ctx.alias_chain`.
fn try_expand_alias_with_db(
    mut argv: Vec<String>,
    db: &datum_core::db::Db,
    ctx: &mut IntentContext,
) -> Vec<String> {
    for _ in 0..MAX_ALIAS_HOPS {
        if argv.len() < 2 || KNOWN_SUBCOMMANDS.contains(&argv[1].as_str()) {
            return argv;
        }
        let name = argv[1].clone();
        match db.lookup_alias(&name) {
            Ok(Some(alias)) => {
                ctx.alias_chain.push(AliasStep {
                    name: name.clone(),
                    expansion: alias.expands_to.clone(),
                });
                let mut result = vec![argv[0].clone()];
                result.extend(alias.expand(&argv[2..]));
                argv = result;
            }
            _ => return argv,
        }
    }
    argv
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
    /// End a duration-tracked activity. `end` is accepted as a synonym.
    #[command(alias = "end")]
    Stop(LogArgs),
    /// Transition a field to a new state.
    Switch(LogArgs),
    /// Canonical add. Shape (point|block|ongoing) is inferred from the
    /// `start_time=` / `end_time=` / `dur=` keys present on the invocation.
    /// Legacy `note` invocations resolve here via a seeded alias.
    Add(LogArgs),

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

    /// Show the full modification trail for an entry.
    History(HistoryArgs),

    /// Explain the intent behind an entry (raw input, alias chain, history).
    Why(WhyArgs),

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

    /// Migration framework: schema migrations + legacy datum import.
    #[command(subcommand)]
    Migrate(MigrateCommand),

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

    /// Render a dayview SVG (or dump the layout struct as JSON).
    Dayview(DayviewArgs),

    /// Show health/metadata for one or all databases.
    Info(InfoArgs),

    /// Initialize or migrate the database schema.
    Setup(SetupArgs),

    /// Show or manage user configuration (datumrc.yml).
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Sync — push and/or pull changesets, manage auth and keys.
    Sync(SyncArgs),

    /// Inspect and resolve overlap / double-open / orphan-end conflicts.
    #[command(subcommand)]
    Conflicts(ConflictsCommand),
}

// ─── Conflicts subcommands (D5) ──────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum ConflictsCommand {
    /// List pending (or resolved) conflicts.
    List(ConflictsListArgs),
    /// Resolve a single conflict by its short hid (or full id).
    Resolve(ConflictsResolveArgs),
    /// Run the auto-resolver over pending conflicts older than --min-age.
    AutoResolve(ConflictsAutoResolveArgs),
}

#[derive(Args, Debug, Default)]
pub struct ConflictsListArgs {
    /// Restrict to one kind: double_open | orphan_end | overlap.
    #[arg(long)]
    pub kind: Option<String>,
    /// Include resolved rows too.
    #[arg(long)]
    pub all: bool,
}

#[derive(Args, Debug)]
pub struct ConflictsResolveArgs {
    /// Short hid or full id of the conflict to resolve.
    pub hid: String,
    /// Keep both entries — record the user's explicit decision.
    #[arg(long, group = "decision")]
    pub keep: Option<String>,
    /// Close the older ongoing entry at the newer one's start_time.
    #[arg(long, group = "decision")]
    pub close_older: bool,
}

#[derive(Args, Debug, Default)]
pub struct ConflictsAutoResolveArgs {
    /// Minimum age (in seconds) before a pending conflict becomes eligible.
    #[arg(long, default_value_t = datum_core::conflicts::AUTO_RESOLVE_MIN_AGE_SECS)]
    pub min_age: i64,
}

// ─── Sync subcommands ────────────────────────────────────────────────────────

/// `datum sync` — the top-level sync verb. Without a subcommand it performs a
/// bidirectional sync (pull then push); `--push` / `--pull` narrow to one
/// direction, and `--dry-run` reports without mutating anything.
#[derive(Args, Debug, Default)]
pub struct SyncArgs {
    #[command(subcommand)]
    pub command: Option<SyncCommand>,

    /// Push only — skip the pull phase.
    #[arg(long, conflicts_with = "pull")]
    pub push: bool,

    /// Pull only — skip the push phase.
    #[arg(long)]
    pub pull: bool,

    /// Report what would happen without writing anything (no HTTP,
    /// no DB mutations, no watermark advances).
    #[arg(long)]
    pub dry_run: bool,

    /// Override the configured sync server for this invocation.
    #[arg(long, value_name = "URL")]
    pub server: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum SyncCommand {
    /// OAuth device-code login. Stores a bearer token per server.
    Login(SyncLoginArgs),
    /// Manage per-DB and global sync server config.
    #[command(subcommand)]
    Config(SyncConfigCommand),
    /// Manage the end-to-end encryption master key.
    #[command(subcommand)]
    Key(SyncKeyCommand),
    /// Show sync status: configured server, last push/pull, pending conflicts.
    Status(SyncStatusArgs),
    /// Install the background sync daemon as a user service (systemd on
    /// Linux, launchd on macOS). Without flags: write unit + enable.
    InstallDaemon(SyncInstallDaemonArgs),
}

#[derive(Args, Debug, Default)]
pub struct SyncStatusArgs {
    /// Override the configured sync server for the status lookup.
    #[arg(long, value_name = "URL")]
    pub server: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct SyncInstallDaemonArgs {
    /// Print the unit file that would be installed and exit.
    #[arg(long)]
    pub show: bool,
    /// Remove the installed unit instead of installing one.
    #[arg(long)]
    pub uninstall: bool,
}

#[derive(Args, Debug, Default)]
pub struct SyncLoginArgs {
    /// Override the configured sync server for this invocation.
    #[arg(long, value_name = "URL")]
    pub server: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum SyncConfigCommand {
    /// Set the sync server. Per-DB by default; `--global` writes the
    /// fallback under `sync.default_server`.
    SetServer {
        /// Relay URL (e.g. `https://sync.example.com`).
        url: String,
        /// Write the global default instead of the per-DB override.
        #[arg(long)]
        global: bool,
    },
    /// Disable sync for the active DB (or globally if no named DB is active).
    Disable,
    /// Print the effective sync config.
    Show,
}

#[derive(Subcommand, Debug)]
pub enum SyncKeyCommand {
    /// Generate a fresh 256-bit master key, wrap it with a passphrase, upload
    /// the wrapped blob to the relay, and persist the plaintext key locally.
    Init {
        /// Override the configured sync server.
        #[arg(long, value_name = "URL")]
        server: Option<String>,
        /// Replace an existing local key file if one exists.
        #[arg(long)]
        force: bool,
    },
    /// Download the wrapped key from the relay, prompt for the passphrase,
    /// and persist the unwrapped master key locally.
    Unlock {
        #[arg(long, value_name = "URL")]
        server: Option<String>,
    },
    /// Re-wrap the existing local master key with a new passphrase and
    /// re-upload it to the relay.
    RotatePassphrase {
        #[arg(long, value_name = "URL")]
        server: Option<String>,
    },
    /// Print the SHA-256 fingerprint of the local master key.
    ShowFingerprint {
        #[arg(long, value_name = "URL")]
        server: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Print the effective config with the source of each key
    /// (flag / env / yaml / default).
    Show(ConfigShowArgs),
}

#[derive(Args, Debug, Default)]
pub struct ConfigShowArgs {
    /// Emit JSON instead of the human-readable table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct InfoArgs {
    /// Name or path of a specific DB. Omit to use the active DB.
    pub target: Option<String>,
    /// Show info for every registered DB + any `*.db` in the default dir.
    #[arg(long)]
    pub all: bool,
}

#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Drop and recreate the schema. Destroys existing data.
    #[arg(long = "force-recreate")]
    pub force_recreate: bool,
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

    /// Data-only id-key slot (repeatable). Accepts `KEY`, `KEY=DEFAULT`, or
    /// `KEY=` (optional, omit when no positional provided). Bare positionals
    /// fill `-K` slots first, then `-k` slots, in declaration order. Legacy
    /// `-k KEY=VALUE` with no matching positional still behaves as an
    /// immediate assignment via the default.
    #[arg(short = 'k', long = "key", value_name = "KEY_SPEC")]
    pub keys: Vec<String>,

    /// Slug-contributing id-key slot (repeatable). Same spec syntax as `-k`.
    /// Populated slots are joined by `__` to form the slug
    /// `<field>:<v1>__<v2>[__…]` — overrides the default `<field>:<timestamp>`.
    #[arg(short = 'K', long = "slug-key", value_name = "KEY_SPEC")]
    pub slug_keys: Vec<String>,

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

    /// Route any log verb through the timeless `add` codepath — produces an
    /// entry with no `start_time` / `end_time` (datum-compatible `-T`).
    /// Incompatible with time-specifying flags (`-t`, `-d`, `--dur`, `-y`,
    /// `-q`, `-z`) — providing any of those alongside `-T` errors out.
    #[arg(short = 'T', long = "no-timestamp")]
    pub no_timestamp: bool,

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

    /// After the initial tail, stream newly-inserted entries live. Optional
    /// interval in seconds (default 1). `-w` alone polls once per second;
    /// `-w 5` polls every 5 s. Ctrl-C to exit.
    #[arg(
        short = 'w',
        long = "watch",
        value_name = "INTERVAL",
        num_args = 0..=1,
        default_missing_value = "1",
    )]
    pub watch: Option<f64>,

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
    /// Mutually exclusive with `--time`.
    #[arg(long, allow_hyphen_values = true, conflicts_with = "time")]
    pub offset: Option<String>,

    /// Replace `start_time` (and `end_time`, shifted by the original
    /// duration) with a freshly parsed time. Accepts the same syntax as
    /// `datum occur -t`, including `±N[unit]` uncertainty; the entry's
    /// precision fields are overwritten from the new parse.
    /// Mutually exclusive with `--offset`.
    #[arg(short = 't', long = "time", allow_hyphen_values = true, conflicts_with = "offset")]
    pub time: Option<String>,

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
    /// Id or slug of a specific entry to undo. Reverts the entry to its
    /// immediately-prior recorded version. When omitted, restores the most
    /// recently deleted entry within the safety window.
    pub id_or_slug: Option<String>,
    /// Revert to a specific version number (requires `id_or_slug`). When
    /// omitted, undo targets version N-1 (the state just before the latest
    /// recorded change).
    #[arg(long = "to-version", value_name = "N")]
    pub to_version: Option<u32>,
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

#[derive(Args, Debug)]
pub struct HistoryArgs {
    /// Entry id or slug. The full modification trail for that entry is shown.
    pub id_or_slug: String,
    /// Print the full entry snapshot at a specific version.
    #[arg(long, value_name = "N")]
    pub version: Option<u32>,
    /// Diff two versions (version-A, version-B). Shows a plain-text JSON diff.
    #[arg(long, num_args = 2, value_names = ["A", "B"])]
    pub diff: Option<Vec<u32>>,
    /// Emit the version list (or snapshot / diff payload) as JSON instead of
    /// the human-readable table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct WhyArgs {
    /// Entry id or slug.
    pub id_or_slug: String,
    /// Trace back through the full history: every raw_input and alias_chain
    /// that shaped this entry, newest first.
    #[arg(long)]
    pub full: bool,
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

    /// Default verb: one of `occur | start | switch | add`. `end`/`stop`/`note`
    /// are not valid defaults — they are terminal or compat-only.
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

    /// Spec-level `-K` identity key (repeatable). Each item may be `key` or
    /// `key=default`.
    #[arg(long = "id-key", value_name = "KEY[=DEFAULT]")]
    pub id_keys: Vec<String>,

    /// Spec-level `-k` data key (repeatable). Each item may be `key` or
    /// `key=default`.
    #[arg(long = "data-key", value_name = "KEY[=DEFAULT]")]
    pub data_keys: Vec<String>,

    /// Disable automatic alias generation for this field.
    #[arg(long = "no-autoalias")]
    pub no_autoalias: bool,
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

#[derive(Subcommand, Debug)]
pub enum MigrateCommand {
    /// Run any pending schema migrations to advance the DB to the current version.
    Run,
    /// Show each registered schema migration and whether it has been applied.
    Status,
    /// (stub) Apply a user-defined data migration (datum's `migrate edit`).
    Edit,
    /// Import entries from a legacy datum backup JSON file.
    Import(MigrateImportArgs),
}

#[derive(Args, Debug)]
pub struct MigrateImportArgs {
    /// Path to datum backup JSON file
    #[arg(long = "from", value_name = "BACKUP_FILE", alias = "backup")]
    pub backup_file: Option<String>,
    /// Positional fallback: path to datum backup JSON file
    #[arg(value_name = "BACKUP_FILE")]
    pub backup_file_positional: Option<String>,
    /// Dry run — show what would be imported without writing
    #[arg(long)]
    pub dry_run: bool,
}

impl MigrateImportArgs {
    /// Resolve the backup file from either `--from` or the positional arg.
    pub fn resolved_path(&self) -> anyhow::Result<&str> {
        self.backup_file
            .as_deref()
            .or(self.backup_file_positional.as_deref())
            .ok_or_else(|| {
                anyhow::anyhow!("migrate import requires a backup file (pass as positional or --from)")
            })
    }
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
    /// Parse the backup and report what would be inserted/skipped/updated
    /// without writing to the DB. Exit code is 0 if the restore would succeed.
    #[arg(long)]
    pub dry_run: bool,
    /// Emit a machine-readable JSON report instead of the human summary.
    /// Useful with `--dry-run`.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Emit a machine-readable JSON report instead of the human summary.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct DayviewArgs {
    /// Day to render, `YYYY-MM-DD`. Defaults to today (local).
    #[arg(long)]
    pub date: Option<String>,

    /// Output file. If omitted, the SVG (or JSON) is printed to stdout.
    #[arg(long, value_name = "PATH")]
    pub output: Option<String>,

    /// Dump the computed layout struct as JSON instead of rendering SVG.
    /// Used by the PWA until the WASM binding is hooked up.
    #[arg(long)]
    pub json: bool,

    /// Output SVG width in pixels.
    #[arg(long, default_value_t = 2000.0)]
    pub width: f64,

    /// Output SVG height in pixels.
    #[arg(long, default_value_t = 200.0)]
    pub height: f64,
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
