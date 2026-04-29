use anyhow::Result;
use datum_cli::{
    args::{Cli, CliExt, Command, InfoArgs},
    cmd, config,
};
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

    let db = datum_core::db::Db::open(&resolved.path.to_string_lossy())?;

    if resolved.name.is_some() {
        let _ = config::ensure_registered(&resolved);
    }

    cmd::dispatch(cli, &db)
}
