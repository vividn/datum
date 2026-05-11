//! CLI argument re-exports + DB-backed alias resolution.
//!
//! The intent-data types and the bespoke parser live in `datum-parse`.
//! This module stays as a thin shim so every `crate::args::Foo` import
//! across `datum-commands` keeps resolving, and hosts the bits of
//! integration that need `datum-core` / `datum-commands` types:
//!
//! * `IntentContext` — thread-local invocation provenance used by
//!   `log_entry` to stamp `raw_input` / `alias_chain` onto entries.
//! * `DbAliasResolver` — the concrete `datum_parse::alias::Resolver`
//!   implementation that looks up aliases against a `datum_core::db::Db`.
//! * `expand_argv` / `expand_argv_with_db` — the one-shot wrappers
//!   every caller used pre-R2; they now drive the datum-parse pre-pass
//!   and write the resulting chain onto [`IntentContext`].
//! * `Cli::parse_with_alias_resolution` / `Cli::try_parse_argv` — same
//!   API surface; internally dispatches to `datum_parse::parse_argv`.
//! * `impl`-blocks that attach `datum-commands::render::OutputArgs` and
//!   `datum-core::db::ConflictStrategy` conversions to the shared
//!   intent structs.

use std::cell::RefCell;

use datum_core::db::ConflictStrategy;
use datum_parse::alias::{self as palias, AliasStep as ParseAliasStep, Resolver};
use datum_parse::cli as pcli;
use datum_parse::error::ParseError;
use serde_json::json;

use crate::render::{OutputArgs, Show};

// ─── Re-exports of the intent shape ──────────────────────────────────────

pub use pcli::{
    AliasCommand, BackupArgs, CheckArgs, Cli, Command, ConfigCommand, ConfigShowArgs,
    ConflictStrategyArg, ConflictsAutoResolveArgs, ConflictsCommand, ConflictsListArgs,
    ConflictsResolveArgs, ConflictsShowArgs, CountArgs, DayviewArgs, DbCommand, DbImportArgs,
    DbPurgeArgs, DeleteArgs, EditArgs, GetArgs, GrepArgs, HistoryArgs, IdsArgs, IgnoreArgs,
    InfoArgs, KitCommand, KitExportArgs, KitInstallArgs, KitUninstallArgs, LogArgs, MigrateCommand,
    NowArgs, NowviewArgs, ReparseArgs, RestoreArgs, RetimeArgs, SetupArgs, SpecCommand,
    SpecSetArgs, SyncArgs, SyncCommand, SyncConfigCommand, SyncInstallDaemonArgs, SyncKeyCommand,
    SyncLoginArgs, SyncStatusArgs, TailArgs, TrackArgs, UndoArgs, UpdateArgs, ViewArgs, ViewFormat,
    WhyArgs,
};

// ─── Intent context (provenance for the current invocation) ──────────────

/// Provenance captured during the CLI pre-pass: the literal argv the
/// user typed and the chain of alias expansions (if any) that were
/// applied. The current invocation pushes one of these onto the
/// thread-local stack; the log-entry code path pops it and stamps it
/// onto the Entry.
#[derive(Debug, Clone, Default)]
pub struct IntentContext {
    pub raw_input: String,
    /// Pre-expansion argv tail (i.e. argv[1..]). Distinct from
    /// `raw_input`'s space-joined form so callers that need to
    /// reconstruct a canonical command (e.g. the `--alias` flag's
    /// post-success persist hook) don't have to re-shell-split.
    pub raw_argv: Vec<String>,
    pub alias_chain: Vec<AliasStep>,
}

/// One step in an alias expansion chain. Keeps the same shape as the
/// pre-R2 struct so persisted JSON round-trips unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasStep {
    pub name: String,
    pub expansion: String,
}

impl From<ParseAliasStep> for AliasStep {
    fn from(s: ParseAliasStep) -> Self {
        Self {
            name: s.name,
            expansion: s.expansion,
        }
    }
}

impl IntentContext {
    /// JSON form suitable for the entry's `alias_chain` column.
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
    /// da-gh0.C: when `Some`, the log path pushes its prepared
    /// [`datum_core::entry::Entry`] (paired with the side's
    /// [`OutputArgs`]) onto this buffer *instead* of inserting it.
    /// The multi-dispatch driver enables the buffer around each side,
    /// collects all N entries, then runs them through
    /// [`datum_core::db::Db::insert_entries_atomic`] so the whole batch
    /// lands in a single transaction with same-field merge applied at
    /// write time. The retained `OutputArgs` lets the post-insert
    /// render path (da-dayk) honour per-side flags (`--show none`,
    /// `--format`, etc.) for the corresponding merged row.
    static MULTI_COLLECTOR: RefCell<Option<Vec<(datum_core::entry::Entry, OutputArgs)>>> =
        const { RefCell::new(None) };
}

pub fn set_intent_context(ctx: IntentContext) {
    INTENT_CTX.with(|slot| *slot.borrow_mut() = Some(ctx));
}

pub fn take_intent_context() -> Option<IntentContext> {
    INTENT_CTX.with(|slot| slot.borrow_mut().take())
}

pub fn peek_intent_context() -> Option<IntentContext> {
    INTENT_CTX.with(|slot| slot.borrow().clone())
}

/// Begin a multi-entry write window: subsequent calls to `log_entry`
/// push their prepared entries into the returned collector rather than
/// inserting them. Pair with [`end_multi_collector`] to drain the
/// buffer and atomically write at the end of the multi dispatch.
pub fn begin_multi_collector() {
    MULTI_COLLECTOR.with(|slot| *slot.borrow_mut() = Some(Vec::new()));
}

/// Drain the multi-collector buffer, leaving the thread-local empty.
/// Returns `None` if no window is currently open. Each element pairs
/// the prepared entry with the originating side's [`OutputArgs`] so
/// downstream render code can honour per-side display flags.
pub fn end_multi_collector() -> Option<Vec<(datum_core::entry::Entry, OutputArgs)>> {
    MULTI_COLLECTOR.with(|slot| slot.borrow_mut().take())
}

/// Push an entry (with its side's [`OutputArgs`]) into the active
/// multi-collector. Returns `true` when a collector is in scope (the
/// caller should skip the normal insert path); returns `false` when
/// there is no active window.
pub fn push_to_multi_collector(entry: datum_core::entry::Entry, output: OutputArgs) -> bool {
    MULTI_COLLECTOR.with(|slot| {
        if let Some(buf) = slot.borrow_mut().as_mut() {
            buf.push((entry, output));
            true
        } else {
            false
        }
    })
}

// ─── Cli — convenience parser entry points ───────────────────────────────

/// Top-level outcome of the unified pre-pass: either a single parsed
/// [`Cli`] (the common case) or an N-sided multi-entry plan ready for
/// per-side dispatch through the multi-collector.
///
/// Both binary (`main.rs`) and the test/REPL entry points (`dispatch_argv`,
/// `repl::run_line`) route through [`parse_invocation`] /
/// [`parse_invocation_with_db`] and branch on this result. There is no
/// other comma-aware seam in the pipeline — the only way to produce a
/// multi-row write is via this enum's [`Self::Multi`] variant.
pub enum Invocation {
    /// Single-row invocation — `cli` is the fully parsed intent.
    Single(Cli),
    /// Multi-row invocation — one parsed [`Cli`] per side, in side
    /// order. `argv0` is the binary name for diagnostic / argv-rebuild
    /// purposes. The shared multi-collector + atomic insert happens
    /// after dispatch of every side.
    Multi {
        sides: Vec<Cli>,
        side_argvs: Vec<Vec<String>>,
        argv0: String,
    },
}

/// Convenience methods that mirror the pre-R2 surface. The actual
/// parsing goes through `datum_parse::parse_argv`; this trait exposes
/// the top-level entry points used by `datum-cli/src/main.rs` and the
/// integration test harness.
pub trait CliExt: Sized {
    /// Capture argv, run the full pre-pass (including comma-split), then
    /// parse. Returns [`Invocation::Single`] or [`Invocation::Multi`].
    fn parse_invocation() -> anyhow::Result<Invocation>;

    /// Parse a raw argv without alias resolution / pre-passes. Test-only.
    fn try_parse_argv<I, S>(argv: I) -> datum_parse::Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;
}

impl CliExt for Cli {
    fn parse_invocation() -> anyhow::Result<Invocation> {
        let raw: Vec<String> = std::env::args().collect();
        parse_invocation(raw)
    }

    fn try_parse_argv<I, S>(argv: I) -> datum_parse::Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let v: Vec<String> = argv.into_iter().map(Into::into).collect();
        datum_parse::parse_argv(&v)
    }
}

// ─── parse_invocation — unified pre-pass + parse pipeline ────────────────

/// Capture argv from `std::env::args`, open the configured DB
/// best-effort for alias lookup, then run the unified
/// comma-split-first pre-pass and parse each resulting side. The
/// returned [`Invocation`] is `Single` when no top-level standalone
/// comma is present and no multi-alias body fires, or `Multi` when N
/// sides were produced.
///
/// This is the canonical binary entry point: both `main.rs` and the
/// test/REPL harnesses route through it (via the env-args wrapper or
/// the explicit-DB variant below). There is no parallel path that
/// skips the comma-split — the multi-row feature can't be bypassed
/// at the binary level.
pub fn parse_invocation(argv: Vec<String>) -> anyhow::Result<Invocation> {
    let maybe_db = (|| -> Option<datum_core::db::Db> {
        let db_path = crate::config::db_path().ok()?;
        if !std::path::Path::new(&db_path).exists() {
            return None;
        }
        datum_core::db::Db::open(&db_path).ok()
    })();

    match maybe_db {
        Some(db) => parse_invocation_inner(argv, &DbAliasResolver::new(&db)),
        None => parse_invocation_inner(argv, &palias::NoResolver),
    }
}

/// Like [`parse_invocation`] but uses the provided `db` for alias
/// lookup. Used by integration tests, REPL, and the WASM frontend.
pub fn parse_invocation_with_db(
    argv: Vec<String>,
    db: &datum_core::db::Db,
) -> anyhow::Result<Invocation> {
    parse_invocation_inner(argv, &DbAliasResolver::new(db))
}

fn parse_invocation_inner(
    argv: Vec<String>,
    resolver: &dyn Resolver,
) -> anyhow::Result<Invocation> {
    let raw_argv: Vec<String> = argv.iter().skip(1).cloned().collect();
    let raw_input = raw_argv.join(" ");
    let argv0 = argv.first().cloned().unwrap_or_default();

    let expanded = palias::expand_into_sides(argv, resolver)?;

    set_intent_context(IntentContext {
        raw_input,
        raw_argv,
        alias_chain: expanded.chain.into_iter().map(Into::into).collect(),
    });

    if expanded.sides.is_empty() {
        // Shouldn't happen — split_argv_at_commas always returns >=1
        // side or errors. Treat as a malformed invocation.
        return Err(anyhow::anyhow!("empty argv after expansion"));
    }

    if expanded.sides.len() == 1 {
        let mut side_argv = Vec::with_capacity(expanded.sides[0].len() + 1);
        side_argv.push(argv0);
        side_argv.extend(expanded.sides.into_iter().next().unwrap());
        let cli = datum_parse::parse_argv(&side_argv).map_err(anyhow::Error::from)?;
        return Ok(Invocation::Single(cli));
    }

    let mut sides: Vec<Cli> = Vec::with_capacity(expanded.sides.len());
    let mut side_argvs: Vec<Vec<String>> = Vec::with_capacity(expanded.sides.len());
    for side in expanded.sides {
        let mut side_argv = Vec::with_capacity(side.len() + 1);
        side_argv.push(argv0.clone());
        side_argv.extend(side);
        let cli = datum_parse::parse_argv(&side_argv).map_err(anyhow::Error::from)?;
        sides.push(cli);
        side_argvs.push(side_argv);
    }
    Ok(Invocation::Multi {
        sides,
        side_argvs,
        argv0,
    })
}

/// Legacy flat-argv expansion: terminal-verb override + alias expansion
/// + field/verb swap + stacked-flag expansion, returning a single
/// argv. Retained only for the Level-2 kit-command post-parse-error
/// recovery path in `datum-cli/src/main.rs`, which needs the
/// re-expanded argv to look up the verb after `parse_argv` rejects it.
/// Do not use for new dispatch paths — they MUST go through
/// [`parse_invocation`] so the comma-split seam stays canonical.
pub fn expand_argv_with_db(
    argv: Vec<String>,
    db: &datum_core::db::Db,
) -> anyhow::Result<Vec<String>> {
    let raw_argv: Vec<String> = argv.iter().skip(1).cloned().collect();
    let raw_input = raw_argv.join(" ");
    let mut ctx = IntentContext {
        raw_input,
        raw_argv,
        alias_chain: Vec::new(),
    };

    let resolver = DbAliasResolver::new(db);
    let (expanded, chain) = palias::expand_argv(argv, &resolver)?;

    ctx.alias_chain = chain.into_iter().map(Into::into).collect();
    set_intent_context(ctx);
    Ok(expanded)
}

/// DB-backed alias resolver. Looks each name up in `aliases` and
/// tokenises the stored `expands_to` via the shared shell-splitter.
pub struct DbAliasResolver<'a> {
    db: &'a datum_core::db::Db,
}

impl<'a> DbAliasResolver<'a> {
    pub fn new(db: &'a datum_core::db::Db) -> Self {
        Self { db }
    }
}

impl<'a> Resolver for DbAliasResolver<'a> {
    fn lookup(&self, name: &str) -> Option<palias::ResolvedAlias> {
        // Precedence: user alias (and derived state aliases) first;
        // Level 1 kit_commands (`expands_to`) fall through second so a
        // user can shadow a kit-defined alias by registering their own.
        // Level 2 kit_commands (no `expands_to`) are NOT splice-style
        // and are handled separately by the post-parse dispatcher.
        if let Some(alias) = self.db.lookup_alias(name).ok().flatten() {
            let tokens = palias::shell_split(&alias.expands_to);
            return Some(palias::ResolvedAlias {
                expands_to: alias.expands_to,
                tokens,
            });
        }
        let cmd = self.db.get_kit_command(name).ok().flatten()?;
        let expands_to = cmd.expands_to?;
        let tokens = palias::shell_split(&expands_to);
        Some(palias::ResolvedAlias { expands_to, tokens })
    }
}

// ─── Extension traits: attach convenience methods to intent structs ──────
//
// The intent structs live in `datum-parse`, so inherent impls can't go
// here. Extension traits expose the pre-R2 method shape (`args.output()`,
// `args.conflict_strategy()`, `args.resolved_path()`) to every caller
// that already imported `crate::args::*`.

/// Render-time resolution starting from the composed
/// [`datum_parse::input::OutputArgs`].
pub fn resolve_parse_output_args(o: &datum_parse::input::OutputArgs) -> OutputArgs {
    use datum_parse::input::output_args as parse_output;

    let resolved = o.resolve();
    OutputArgs {
        show: match resolved.show {
            parse_output::Show::Default => Show::Default,
            parse_output::Show::None => Show::None,
            parse_output::Show::Minimal => Show::Minimal,
            parse_output::Show::Standard => Show::Standard,
            parse_output::Show::Format => Show::Format,
            parse_output::Show::All => Show::All,
        },
        format_string: resolved.format_string,
    }
}

/// Adds `.output()` / `.conflict_strategy()` to the shared intent
/// structs. Imported wherever callers used the pre-R2 inherent methods.
pub trait LogArgsExt {
    fn output(&self) -> OutputArgs;
    fn conflict_strategy(&self) -> ConflictStrategy;
}

impl LogArgsExt for LogArgs {
    fn output(&self) -> OutputArgs {
        resolve_parse_output_args(&self.output)
    }
    fn conflict_strategy(&self) -> ConflictStrategy {
        if self.update {
            ConflictStrategy::Update
        } else if self.merge {
            ConflictStrategy::Merge
        } else {
            self.conflict.map(Into::into).unwrap_or_default()
        }
    }
}

/// Generic `.output()` bridge for intents whose composed [`OutputArgs`]
/// field needs render-time resolution.
pub trait OutputFlagsExt {
    fn output(&self) -> OutputArgs;
}

macro_rules! impl_output_args_ext {
    ($ty:ty) => {
        impl OutputFlagsExt for $ty {
            fn output(&self) -> OutputArgs {
                resolve_parse_output_args(&self.output)
            }
        }
    };
}

impl_output_args_ext!(TailArgs);
impl_output_args_ext!(GrepArgs);
impl_output_args_ext!(GetArgs);
impl_output_args_ext!(UpdateArgs);
impl_output_args_ext!(EditArgs);
impl_output_args_ext!(DeleteArgs);
impl_output_args_ext!(UndoArgs);

/// `.resolved_path()` on [`DbImportArgs`].
pub trait DbImportArgsExt {
    fn resolved_path(&self) -> anyhow::Result<&str>;
}

impl DbImportArgsExt for DbImportArgs {
    fn resolved_path(&self) -> anyhow::Result<&str> {
        self.backup_file
            .as_deref()
            .or(self.backup_file_positional.as_deref())
            .ok_or_else(|| {
                anyhow::anyhow!("db import requires a backup file (pass as positional or --from)")
            })
    }
}

// `datum-parse` returns `ParseError`, but most callers use `anyhow`.
// The conversion is automatic through anyhow's blanket
// `From<E: std::error::Error + Send + Sync + 'static>` impl.
#[allow(dead_code)]
fn _parse_error_goes_into_anyhow(e: ParseError) -> anyhow::Error {
    anyhow::Error::from(e)
}
