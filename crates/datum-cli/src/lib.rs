#![doc = include_str!("../README.md")]

//! Library view of the `datum` CLI — lets integration tests dispatch through
//! the full arg-parse → command-dispatch path without spawning a subprocess.
//!
//! Most functionality now lives in the sibling `datum-commands` crate.
//! `datum-cli` keeps the binary entry point and re-exports `datum-commands`
//! modules under their pre-R1 names so existing tests keep compiling.

pub use datum_commands::args;
pub use datum_commands::cmd;
pub use datum_commands::commands::check;
pub use datum_commands::config;
pub use datum_commands::render as output;

#[cfg(feature = "native")]
pub mod repl;

use anyhow::Result;
use datum_commands::commands;
use datum_parse::{help, ParseError};

use crate::args::{Cli, Command, DbCommand, InfoArgs, Invocation, KitCommand, SyncCommand};

/// `datum 3.0.<height>-beta (<sha7>)`, from git at build time (da-5172a,
/// da-cv118, `build.rs`).
pub const VERSION_BANNER: &str = concat!("datum ", env!("DATUM_BUILD"));

/// If `e` is one of the "clean stdout print" parse errors
/// (help/version/man requested), render it to stdout and return
/// `Some(Ok(()))`. Otherwise return `None` — the caller should
/// propagate the original error (or apply its own fallbacks first).
///
/// The set itself lives in `datum_parse::help::special_output`, which is
/// where the web runner reaches it too (da-e0m6) — so `run`, `repl::run_line`
/// and the browser all honour exactly the same contract. `help NAME` for a
/// name only `db` defines, an alias or a kit command, is answered from it
/// first (da-3odno).
pub fn render_parse_special(e: &ParseError, db: Option<&datum_core::db::Db>) -> Option<Result<()>> {
    if let (Some(name), Some(db)) = (commands::help::asks_the_db(e), db) {
        match commands::help::page(name, db) {
            Ok(Some(text)) => {
                match e {
                    ParseError::GrammarRequested(_) => print!("{}", help::render_page_json(&text)),
                    _ => print!("{text}"),
                }
                return Some(Ok(()));
            }
            Ok(None) => {}
            Err(e) => return Some(Err(e)),
        }
    }
    let text = help::special_output(e, VERSION_BANNER)?;
    print!("{text}");
    Some(Ok(()))
}

/// Drive a captured `argv` through the same path the `datum` binary
/// uses for `datum <subcommand>` invocations. Carved out of `main.rs`
/// so the bin-smoke E2E gate (`tests/e2e_bin_smoke.rs`, da-1wat) can
/// exercise the actual binary entry path in-process — no subprocess
/// spawn, no parallel "tests-only" dispatch path.
///
/// Does **not** route through the REPL — callers handle the empty-argv
/// case themselves.
pub fn run(argv: Vec<String>) -> Result<()> {
    let _ = datum_core::BUILD.set(env!("DATUM_BUILD"));
    // Before any open: a drain or a snapshot signs ops ahead of `dispatch`,
    // and with no client yet `sync::endpoint` would mint this copy a new
    // key (da-9q3fr).
    datum_core::db::set_client_source(|| config::client_id().ok());
    // Shell completion's hidden reader (da-bu09u): before any parse or db
    // open, and it never fails — a TAB must not print an error. Global
    // options may stand before it (da-ejoes).
    if let Some(at) = commands::complete::position(&argv) {
        commands::complete::run(&argv[1..at], &argv[at + 1..]);
        return Ok(());
    }
    // Clear the dedup-no-op flag so it reflects only this invocation
    // (da-9sfm). `main` reads it after a clean run to choose exit code 11.
    args::reset_duplicate_noop();
    let invocation = match args::parse_invocation(argv.clone()) {
        Ok(inv) => inv,
        Err(e) => {
            // Propagate `--help` / `--version` / `--man` as a clean stdout print.
            if let Some(pe) = e.downcast_ref::<ParseError>() {
                // A second open: the parse's own died inside it. Only a help
                // request for a name no table knows pays it, and the first
                // open already migrated.
                let db =
                    commands::help::asks_the_db(pe).and_then(|_| args::open_existing_db(&argv));
                if let Some(result) = render_parse_special(pe, db.as_ref()) {
                    return result;
                }
                if let ParseError::InvalidArg(msg) = pe {
                    if msg.starts_with("unknown subcommand") {
                        if let Some(()) = try_kit_level2_dispatch()? {
                            return Ok(());
                        }
                    }
                }
            }
            return Err(e);
        }
    };

    let result = match invocation {
        Invocation::Single(cli) => dispatch_single(cli),
        Invocation::Multi { .. } => dispatch_multi(invocation),
    };
    // Parsed again: the first parse was moved into its dispatch, and `Cli`
    // is not `Clone`. Same argv, against the same file; an alias a drain
    // landed in between resolves as it would for the next command.
    if let Some(Locked(path)) = result.as_ref().err().and_then(|e| e.downcast_ref()) {
        return queue(path, args::parse_invocation(argv)?);
    }
    result
}

/// A command that writes, finding the database's write lock held and
/// having written nothing (da-9q3fr): [`run`] spools it instead.
#[derive(Debug)]
pub(crate) struct Locked(String);

impl std::fmt::Display for Locked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: database is locked", self.0)
    }
}

impl std::error::Error for Locked {}

/// How long a command that writes waits for the lock before it spools: past
/// a sync apply's 500 ms chunk and its pause (`APPLY_HOLD`, da-2ejbs), so a
/// write beside one lands as before, and within a second to QUEUED.
const SPOOL_AFTER: std::time::Duration = std::time::Duration::from_millis(750);

/// Does `command` change entries, and so spool when it can't write? Every
/// command that does, but sync's, `db`'s and `config`'s, which act on the
/// file, the network or a device rather than on what it holds, and `kit`'s,
/// whose indices are DDL no op carries. None in the browser, whose database
/// is one process's at a time.
fn spools(command: &Command) -> bool {
    !cfg!(target_family = "wasm")
        && matches!(
            command,
            Command::Occur(_)
                | Command::Start(_)
                | Command::Stop(_)
                | Command::Switch(_)
                | Command::Add(_)
                | Command::Note(_)
                | Command::Delete(_)
                | Command::Undo(_)
                | Command::Redo(_)
                | Command::Edit(_)
                | Command::Update(_)
                | Command::Merge(_)
                | Command::Rekey(_)
                | Command::Retime(_)
                | Command::Spec(_)
                | Command::Alias(_)
                | Command::Conflicts(_)
                | Command::Track(_)
                | Command::Ignore(_)
                | Command::Flag(_)
        )
}

/// Does `command` only read the database? Then one that is not there is an
/// error, and only a write makes it (da-oo6tr). The browser's database is
/// made by the page.
fn reads(command: &Command) -> bool {
    !cfg!(target_family = "wasm")
        && matches!(
            command,
            Command::Tail(_)
                | Command::Head(_)
                | Command::Now(_)
                | Command::Get(_)
                | Command::History(_)
                | Command::Why(_)
                | Command::Grep(_)
                | Command::Check(_)
                | Command::Count(_)
                | Command::Stats(_)
                | Command::Ids(_)
                | Command::Fields(_)
                | Command::Dayview(_)
                | Command::Nowview(_)
                | Command::View(_)
                | Command::Kit(KitCommand::List | KitCommand::Export(_))
                | Command::Db(DbCommand::Backup(_))
        )
}

/// Refuse a read of a database that does not exist, before anything is made.
fn refuse_missing(db: Option<&str>) -> Result<()> {
    match missing(db) {
        Some(path) => anyhow::bail!(
            "no database at {}: a read does not create one, the first write does (e.g. `datum occur <field>`)",
            path.display()
        ),
        None => Ok(()),
    }
}

/// Where the database `db` names would be, when no file is there.
fn missing(db: Option<&str>) -> Option<std::path::PathBuf> {
    config::locate_db(db).filter(|path| !path.exists())
}

/// Can `command` ask its person something before it writes? `edit` always
/// (its editor), and a change to `_LAST` named by default (the stale-`_LAST`
/// question).
fn asks(command: &Command) -> bool {
    match command {
        Command::Edit(_) => true,
        // `update_core::parse_update_payload`'s rule: no bare token, `_LAST`.
        Command::Update(a) | Command::Merge(a) | Command::Rekey(a) => {
            a.id_or_slug.iter().chain(&a.rest).all(|t| t.contains('='))
        }
        Command::Retime(a) => a.slug.is_none(),
        _ => false,
    }
}

/// Run a command that may ask its person on a snapshot from the start
/// (vividn, 2026-10-15: «it shouldn't rerun anything. It should just write the
/// database at the right spot»): what it asks is about the entry it finds,
/// and what it signs is that answer, computed once. Its ops land as a
/// drain lands a line, now when the lock is free, and else at the next
/// drain, so a change made meanwhile to the same entry by another copy is
/// a sync conflict (da-9q3fr). A snapshot costs ~1 ms per MB copied, which a
/// command waiting on a person can spend and a log cannot.
fn ask_first(path: &str, cli: Cli) -> Result<()> {
    let (snapshot, spool) = datum_core::db::Db::spool_open(path)?;
    snapshot_notes(path, &spool);
    cmd::dispatch(cli, &snapshot)?;
    if !spool.append(&snapshot)? {
        return Ok(());
    }
    let db = datum_core::db::Db::open(path);
    if db
        .as_ref()
        .map_or(true, |db| db.drain_spool().is_err() || db.spool_waiting())
    {
        eprintln!("note: the database is locked: this change is queued in {path}.spool and lands at the next command");
    }
    Ok(())
}

/// Open `path` for `run` to write, landing what is spooled beside it, or
/// say [`Locked`] when `spools` and another holds the write lock.
fn open_to_write(path: &str, spools: bool) -> Result<datum_core::db::Db> {
    let db = match datum_core::db::Db::open(path) {
        Err(e) if spools && datum_core::db::busy(&e) => return Err(Locked(path.into()).into()),
        r => r?,
    };
    datum_commands::announce_migration(&db);
    ready_to_write(&db, path, spools)?;
    Ok(db)
}

/// Land what is spooled beside `db`, at `path`, and say [`Locked`] when
/// `spools` and a write now would not land.
fn ready_to_write(db: &datum_core::db::Db, path: &str, spools: bool) -> Result<()> {
    #[cfg(not(target_family = "wasm"))]
    {
        let stuck = match db.drain_spool() {
            Ok(d) => {
                if d.landed > 0 {
                    eprintln!("note: {} queued change(s) written", d.landed);
                }
                if d.wiped > 0 {
                    eprintln!(
                        "note: dropped {} queued change(s) made before the sync chain's purge",
                        d.wiped
                    );
                }
                set_aside(path, &d.set_aside);
                d.stuck
            }
            Err(e) => Some(e),
        };
        if let Some(e) = stuck {
            eprintln!(
                "⚠ the changes queued in {path}.spool cannot land: {e} — `datum check spool` shows them"
            );
        }
    }
    // Behind changes still queued (a drain stepped aside, or a line waits),
    // a write queues too: this copy's changes land in the order they were
    // made, and its ops in that order (da-9q3fr).
    if spools && (db.spool_waiting() || !db.can_write(SPOOL_AFTER)?) {
        return Err(Locked(path.into()).into());
    }
    Ok(())
}

/// Run `invocation` on `db`, open at `path`, as a command of its own runs
/// (da-3j6g7): a REPL line. What is spooled lands first, one that may ask
/// its person runs on a snapshot from the start, and a write the lock
/// refuses is [`Locked`], for the caller to [`queue`].
#[cfg(feature = "native")]
pub(crate) fn run_on(db: &datum_core::db::Db, path: &str, invocation: Invocation) -> Result<()> {
    let spools = match &invocation {
        Invocation::Multi { sides, .. } => sides.iter().any(|c| spools(&c.command)),
        Invocation::Single(c) => spools(&c.command),
    };
    let invocation = match invocation {
        Invocation::Single(cli) if spools && asks(&cli.command) => return ask_first(path, cli),
        i => i,
    };
    ready_to_write(db, path, spools)?;
    let before = db.conn().total_changes();
    locked_out(
        cmd::dispatch_invocation(invocation, db),
        db,
        before,
        spools,
        path,
    )
}

/// Say where each line queued beside `path` that could not be read was set
/// aside (da-29om3).
fn set_aside(path: &str, to: &[std::path::PathBuf]) {
    for p in to {
        eprintln!(
            "warning: a change queued in {path}.spool could not be read: set aside in {}",
            p.display()
        );
    }
}

/// Say what the snapshot of `path` a command runs on could not take from
/// the spool: lines set aside, and the queued changes it stops short of.
fn snapshot_notes(path: &str, spool: &datum_core::db::Spool) {
    set_aside(path, spool.set_aside());
    if let Some(e) = spool.short() {
        eprintln!("warning: this command ran without some changes queued in {path}.spool: {e}");
    }
}

/// Whether `result`, of a command that `spools` and changed nothing on
/// `db` since `before`, failed for the lock: then it is [`Locked`] too.
fn locked_out(
    result: Result<()>,
    db: &datum_core::db::Db,
    before: u64,
    spools: bool,
    path: &str,
) -> Result<()> {
    match result {
        Err(e)
            if spools
                && db.conn().total_changes() == before
                && e.chain().any(datum_core::db::busy) =>
        {
            Err(Locked(path.into()).into())
        }
        r => r,
    }
}

/// Run `invocation` on a snapshot of the database at `path`, printing
/// QUEUED for its action word, and spool what it signed (da-9q3fr).
pub(crate) fn queue(path: &str, invocation: Invocation) -> Result<()> {
    let (snapshot, spool) = datum_core::db::Db::spool_open(path)?;
    snapshot_notes(path, &spool);
    {
        let _queued = output::Queued::hold();
        cmd::dispatch_invocation(invocation, &snapshot)?;
    }
    spool.append(&snapshot)?;
    Ok(())
}

fn dispatch_single(cli: Cli) -> Result<()> {
    // `--color always|never` outranks the environment for the whole run,
    // a REPL started under it included.
    let _color = cli.color.map(output::colors::ColorOverride::set);
    if matches!(&cli.command, Command::Repl) {
        #[cfg(feature = "native")]
        return repl::run(cli.db.as_deref());
        // No line editor under WASI; the browser has its own REPL.
        #[cfg(not(feature = "native"))]
        anyhow::bail!("repl is unavailable in this build");
    }

    // `datum config show`, `datum completions` and `datum sync daemon` run
    // against an in-memory placeholder: they do not read the entry db (the
    // daemon opens each chained db itself, and must not create the default).
    if matches!(
        &cli.command,
        Command::Config(_) | Command::Completions(_) | Command::Sync(SyncCommand::Daemon(_))
    ) {
        let placeholder = datum_core::db::Db::open_in_memory()?;
        return cmd::dispatch(cli, &placeholder);
    }

    if matches!(
        &cli.command,
        Command::Info(InfoArgs { all: true, .. })
            | Command::Info(InfoArgs {
                target: Some(_),
                ..
            })
    ) && cli.db.is_none()
        // A missing active database is reported `✗ (missing)`, as a TARGET is.
        || matches!(&cli.command, Command::Info(_)) && missing(cli.db.as_deref()).is_some()
    {
        let placeholder = datum_core::db::Db::open_in_memory()?;
        return cmd::dispatch(cli, &placeholder);
    }

    if reads(&cli.command) {
        refuse_missing(cli.db.as_deref())?;
    }
    let resolved = config::resolve_db(cli.db.as_deref())?;

    if resolved.created_new {
        if let Some(name) = resolved.name.as_deref() {
            eprintln!(
                "note: created new database '{name}' at {}",
                resolved.path.display()
            );
        }
    }

    let path = resolved.path.to_string_lossy().into_owned();
    let spools = spools(&cli.command);
    if spools && asks(&cli.command) {
        if resolved.name.is_some() {
            let _ = config::ensure_registered(&resolved);
        }
        return ask_first(&path, cli);
    }
    let db = name_holder(
        if matches!(&cli.command, Command::Db(DbCommand::Purge(_))) {
            datum_core::db::Db::open_unmigrated(&path)
                .inspect(datum_commands::announce_migration)
                .map_err(Into::into)
        } else {
            open_to_write(&path, spools)
        },
        &resolved.path,
    )?;

    if resolved.name.is_some() {
        let _ = config::ensure_registered(&resolved);
    }

    let before = db.conn().total_changes();
    name_holder(
        locked_out(cmd::dispatch(cli, &db), &db, before, spools, &path),
        &resolved.path,
    )
}

/// A write that waited out the lock says who held it (da-hyem1).
fn name_holder<T>(result: Result<T>, db: &std::path::Path) -> Result<T> {
    result.map_err(|e| match datum_core::db::lock_holder(db) {
        Some(holder) if e.chain().any(datum_core::db::busy) => {
            e.context(format!("the database is locked by {holder}"))
        }
        _ => e,
    })
}

fn dispatch_multi(invocation: Invocation) -> Result<()> {
    // The global flags are prepended to every side, so the first has them.
    let first = match &invocation {
        Invocation::Multi { sides, .. } => sides.first(),
        _ => None,
    };
    let db_hint = first.and_then(|c| c.db.clone());
    let _color = first
        .and_then(|c| c.color)
        .map(output::colors::ColorOverride::set);
    if matches!(&invocation, Invocation::Multi { sides, .. } if sides.iter().all(|c| reads(&c.command)))
    {
        refuse_missing(db_hint.as_deref())?;
    }
    let resolved = config::resolve_db(db_hint.as_deref())?;
    if resolved.created_new {
        if let Some(name) = resolved.name.as_deref() {
            eprintln!(
                "note: created new database '{name}' at {}",
                resolved.path.display()
            );
        }
    }
    let path = resolved.path.to_string_lossy().into_owned();
    let spools = match &invocation {
        Invocation::Multi { sides, .. } => sides.iter().any(|c| spools(&c.command)),
        Invocation::Single(c) => spools(&c.command),
    };
    let db = name_holder(open_to_write(&path, spools), &resolved.path)?;
    if resolved.name.is_some() {
        let _ = config::ensure_registered(&resolved);
    }
    let before = db.conn().total_changes();
    name_holder(
        locked_out(
            cmd::dispatch_invocation(invocation, &db),
            &db,
            before,
            spools,
            &path,
        ),
        &resolved.path,
    )
}

fn try_kit_level2_dispatch() -> Result<Option<()>> {
    let mut raw: Vec<String> = std::env::args()
        .map(datum_parse::alias::spell_color)
        .collect();
    // A leading `--db NAME` names the database the command runs against —
    // the web passes one on every named database (da-1mkej) — and a
    // `--color WHEN` beside it was already checked by the parse that failed.
    let (mut named, mut color) = (None, None);
    while let Some(first) = raw.get(1).cloned() {
        let (flag, value) = match first.split_once('=') {
            Some((f @ ("--db" | "--color"), v)) => {
                raw.remove(1);
                (f.to_string(), v.to_string())
            }
            None if (first == "--db" || first == "--color") && raw.len() > 2 => {
                let v = raw.remove(2);
                raw.remove(1);
                (first.clone(), v)
            }
            _ => break,
        };
        if flag == "--db" {
            named = Some(value);
        } else {
            color = match value.as_str() {
                "always" => Some(true),
                "never" => Some(false),
                _ => None,
            };
        }
    }
    let _color = color.map(output::colors::ColorOverride::set);
    if raw.len() < 2 {
        return Ok(None);
    }

    let resolved = config::resolve_db(named.as_deref())?;
    if !resolved.path.exists() {
        return Ok(None);
    }
    let db = datum_core::db::Db::open(&resolved.path.to_string_lossy())?;
    datum_commands::announce_migration(&db);

    let expanded = args::expand_argv_with_db(raw, &db)?;
    if expanded.len() < 2 {
        return Ok(None);
    }
    let name = &expanded[1];

    let Some(cmd) = db.get_kit_command(name)? else {
        return Ok(None);
    };
    if cmd.view_name.is_none() {
        return Ok(None);
    }

    let extra: Vec<String> = expanded.iter().skip(2).cloned().collect();
    commands::kit_command::run(name, &extra, &db)?;
    Ok(Some(()))
}
