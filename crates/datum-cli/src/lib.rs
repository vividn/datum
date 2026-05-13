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
pub use datum_commands::commands::sync;
pub use datum_commands::config;
pub use datum_commands::render as output;

pub mod repl;

use anyhow::Result;
use datum_commands::commands;
use datum_parse::{help, ParseError};

use crate::args::{Cli, Command, DbCommand, InfoArgs, Invocation};

/// Drive a captured `argv` through the same path the `datum` binary
/// uses for `datum <subcommand>` invocations. Carved out of `main.rs`
/// so the bin-smoke E2E gate (`tests/e2e_bin_smoke.rs`, da-1wat) can
/// exercise the actual binary entry path in-process — no subprocess
/// spawn, no parallel "tests-only" dispatch path.
///
/// Does **not** route through the REPL — callers handle the empty-argv
/// case themselves.
pub fn run(argv: Vec<String>) -> Result<()> {
    let invocation = match args::parse_invocation(argv) {
        Ok(inv) => inv,
        Err(e) => {
            // Propagate `--help` / `--version` as a clean stdout print.
            if let Some(pe) = e.downcast_ref::<ParseError>() {
                match pe {
                    ParseError::HelpRequested(cmd) => {
                        let text = match cmd.as_deref() {
                            Some(c) => help::command_help(c),
                            None => help::top_level_help(),
                        };
                        print!("{text}");
                        return Ok(());
                    }
                    ParseError::VersionRequested => {
                        println!("{}", help::version_banner());
                        return Ok(());
                    }
                    ParseError::ManRequested(cmd) => {
                        print!("{}", help::render_roff(cmd.as_deref()));
                        return Ok(());
                    }
                    ParseError::InvalidArg(msg) if msg.starts_with("unknown subcommand") => {
                        if let Some(()) = try_kit_level2_dispatch()? {
                            return Ok(());
                        }
                    }
                    _ => {}
                }
            }
            return Err(e);
        }
    };

    match invocation {
        Invocation::Single(cli) => dispatch_single(cli),
        Invocation::Multi { .. } => dispatch_multi(invocation),
    }
}

fn dispatch_single(cli: Cli) -> Result<()> {
    if matches!(&cli.command, Command::Repl) {
        return repl::run(cli.db.as_deref());
    }

    if matches!(&cli.command, Command::Config(_) | Command::Sync(_)) {
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
    {
        let placeholder = datum_core::db::Db::open_in_memory()?;
        return cmd::dispatch(cli, &placeholder);
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

    let db = if matches!(&cli.command, Command::Db(DbCommand::Purge(_))) {
        datum_core::db::Db::open_unmigrated(&resolved.path.to_string_lossy())?
    } else {
        datum_core::db::Db::open(&resolved.path.to_string_lossy())?
    };

    if resolved.name.is_some() {
        let _ = config::ensure_registered(&resolved);
    }

    cmd::dispatch(cli, &db)
}

fn dispatch_multi(invocation: Invocation) -> Result<()> {
    let db_hint = match &invocation {
        Invocation::Multi { sides, .. } => sides.first().and_then(|c| c.db.clone()),
        _ => None,
    };
    let resolved = config::resolve_db(db_hint.as_deref())?;
    if resolved.created_new {
        if let Some(name) = resolved.name.as_deref() {
            eprintln!(
                "note: created new database '{name}' at {}",
                resolved.path.display()
            );
        }
    }
    let db = datum_core::db::Db::open(&resolved.path.to_string_lossy())?;
    if resolved.name.is_some() {
        let _ = config::ensure_registered(&resolved);
    }
    cmd::dispatch_invocation(invocation, &db)
}

fn try_kit_level2_dispatch() -> Result<Option<()>> {
    let raw: Vec<String> = std::env::args().collect();
    if raw.len() < 2 {
        return Ok(None);
    }

    let resolved = config::resolve_db(None)?;
    if !resolved.path.exists() {
        return Ok(None);
    }
    let db = datum_core::db::Db::open(&resolved.path.to_string_lossy())?;

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
