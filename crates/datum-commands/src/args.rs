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
    ConflictsResolveArgs, ConflictsShowArgs, CountArgs, DayviewArgs, DeleteArgs, EditArgs,
    GetArgs, GrepArgs,
    HistoryArgs, IdsArgs, IgnoreArgs, InfoArgs, LogArgs, MigrateCommand, MigrateImportArgs,
    NowArgs, NowviewArgs, OutputFlags, RestoreArgs, RetimeArgs, SetupArgs, SpecCommand, SpecSetArgs,
    SyncArgs, SyncCommand, SyncConfigCommand, SyncInstallDaemonArgs, SyncKeyCommand,
    SyncLoginArgs, SyncStatusArgs, TailArgs, TrackArgs, UndoArgs, UpdateArgs, WhyArgs,
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

// ─── Cli — convenience parser entry points ───────────────────────────────

/// Convenience methods that mirror the pre-R2 surface. The actual
/// parsing goes through `datum_parse::parse_argv`; this trait exposes
/// the top-level entry points used by `datum-cli/src/main.rs` and the
/// integration test harness.
pub trait CliExt: Sized {
    /// Capture argv, run the full pre-pass, then parse.
    fn parse_with_alias_resolution() -> anyhow::Result<Self>;

    /// Parse a raw argv without alias resolution / pre-passes. Test-only.
    fn try_parse_argv<I, S>(argv: I) -> datum_parse::Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>;
}

impl CliExt for Cli {
    fn parse_with_alias_resolution() -> anyhow::Result<Self> {
        let raw: Vec<String> = std::env::args().collect();
        let expanded = expand_argv(raw)?;
        Ok(datum_parse::parse_argv(&expanded).map_err(anyhow::Error::from)?)
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

// ─── expand_argv — pre-pass wrapper ──────────────────────────────────────

/// Full pre-pass: terminal-verb override → alias expansion → field/verb
/// swap → stacked short-flag expansion. Also publishes the
/// [`IntentContext`] on the thread-local slot so `log_entry` can stamp it.
pub fn expand_argv(argv: Vec<String>) -> anyhow::Result<Vec<String>> {
    let raw_input = argv.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
    let mut ctx = IntentContext {
        raw_input,
        alias_chain: Vec::new(),
    };

    let maybe_db = (|| -> Option<datum_core::db::Db> {
        let db_path = crate::config::db_path().ok()?;
        if !std::path::Path::new(&db_path).exists() {
            return None;
        }
        datum_core::db::Db::open(&db_path).ok()
    })();

    let (expanded, chain) = match maybe_db {
        Some(db) => {
            let resolver = DbAliasResolver::new(&db);
            palias::expand_argv(argv, &resolver)
        }
        None => (palias::expand_argv_no_alias(argv), Vec::new()),
    };

    ctx.alias_chain = chain.into_iter().map(Into::into).collect();
    set_intent_context(ctx);
    Ok(expanded)
}

/// Like [`expand_argv`] but uses the provided `db` for alias lookup.
pub fn expand_argv_with_db(
    argv: Vec<String>,
    db: &datum_core::db::Db,
) -> anyhow::Result<Vec<String>> {
    let raw_input = argv.iter().skip(1).cloned().collect::<Vec<_>>().join(" ");
    let mut ctx = IntentContext {
        raw_input,
        alias_chain: Vec::new(),
    };

    let resolver = DbAliasResolver::new(db);
    let (expanded, chain) = palias::expand_argv(argv, &resolver);

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
        let alias = self.db.lookup_alias(name).ok().flatten()?;
        // `Alias::expand(&[])` replays the stored tokens with no extra
        // args — exactly what we want for a pre-pass splice. But we
        // need the token list WITHOUT any trailing args, so construct
        // it fresh via a shell-split.
        let tokens = palias::shell_split(&alias.expands_to);
        Some(palias::ResolvedAlias {
            expands_to: alias.expands_to,
            tokens,
        })
    }
}

// ─── Extension traits: attach convenience methods to intent structs ──────
//
// The intent structs live in `datum-parse`, so inherent impls can't go
// here. Extension traits expose the pre-R2 method shape (`args.output()`,
// `args.conflict_strategy()`, `args.resolved_path()`) to every caller
// that already imported `crate::args::*`.

/// Resolve raw [`OutputFlags`] into the render-time [`OutputArgs`].
pub fn resolve_output_flags(f: &OutputFlags) -> OutputArgs {
    let show = if f.show_all {
        Show::All
    } else {
        f.show.parse::<Show>().unwrap_or_default()
    };
    let show = if f.format_string.is_some() && f.show == "default" && !f.show_all {
        Show::Format
    } else {
        show
    };
    OutputArgs {
        show,
        format_string: f.format_string.clone(),
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
        resolve_output_flags(&self.output_flags)
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

/// Generic `.output()` bridge for every non-LogArgs intent that carries
/// an [`OutputFlags`] bag.
pub trait OutputFlagsExt {
    fn output(&self) -> OutputArgs;
}

macro_rules! impl_output_flags_ext {
    ($ty:ty) => {
        impl OutputFlagsExt for $ty {
            fn output(&self) -> OutputArgs {
                resolve_output_flags(&self.output_flags)
            }
        }
    };
}

impl_output_flags_ext!(TailArgs);
impl_output_flags_ext!(UpdateArgs);
impl_output_flags_ext!(EditArgs);
impl_output_flags_ext!(DeleteArgs);
impl_output_flags_ext!(UndoArgs);
impl_output_flags_ext!(GrepArgs);
impl_output_flags_ext!(GetArgs);

/// `.resolved_path()` on [`MigrateImportArgs`].
pub trait MigrateImportArgsExt {
    fn resolved_path(&self) -> anyhow::Result<&str>;
}

impl MigrateImportArgsExt for MigrateImportArgs {
    fn resolved_path(&self) -> anyhow::Result<&str> {
        self.backup_file
            .as_deref()
            .or(self.backup_file_positional.as_deref())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "migrate import requires a backup file (pass as positional or --from)"
                )
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
