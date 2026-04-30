use anyhow::Result;
use datum_cli::{
    args::{Cli, CliExt, Command, DbCommand, InfoArgs},
    cmd, config,
};
use datum_commands::commands;
use datum_parse::{help, ParseError};

fn main() -> Result<()> {
    let cli = match Cli::parse_with_alias_resolution() {
        Ok(cli) => cli,
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
                        // The pre-pass already ran (alias + Level-1
                        // kit_command resolution); the only remaining way
                        // to recognise this token is a Level-2 kit command
                        // installed in the user's DB. Resolve it now and
                        // dispatch to the structured renderer; otherwise
                        // bubble the original parse error.
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

    // `config` and `sync` don't touch any DB — skip resolution entirely.
    if matches!(&cli.command, Command::Config(_) | Command::Sync(_)) {
        let placeholder = datum_core::db::Db::open_in_memory()?;
        return cmd::dispatch(cli, &placeholder);
    }

    // `info --all` and `info <target>` inspect other databases; don't
    // force-create the default DB just to satisfy dispatch plumbing.
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

    // Escape hatch for a corrupt or half-migrated DB: `db purge` opens
    // through `open_unmigrated`. Auto-migrate runs inside `Db::open` and
    // would error before the recovery code gets a chance to run on exactly
    // the DB that needs recovering. Open without migrating; the command
    // then drops and recreates the schema from scratch.
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

/// Best-effort Level-2 kit command dispatch invoked when `parse_argv`
/// rejects the verb as unknown. Opens the default DB, looks for a row
/// in `kit_commands` with `view_name` set, and runs the renderer with
/// any tokens after the command name passed through. Returns `Some(())`
/// if the command was found and dispatched, `None` if no match (so the
/// caller surfaces the original parse error).
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

    // Re-run the alias/Level-1 pre-pass against the actual DB so that a
    // Level-2 command name buried under a Level-1 alias is reachable.
    let expanded = datum_commands::args::expand_argv_with_db(raw, &db)?;
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
