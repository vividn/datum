use super::core::*;
use super::migrate_io::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_info_args(tail: &[&str]) -> Result<InfoArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut all = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("all") => all = true,
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(InfoArgs {
        target: optional_pos(&pos)?,
        all,
    })
}

pub(crate) fn parse_reparse_args(tail: &[&str]) -> Result<ReparseArgs> {
    let mut out = ReparseArgs::default();
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("fix") => {
                out.fix = true;
                i += 1;
            }
            FlagSplit::Positional(tok) => {
                out.hids.push(tok.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

pub(crate) fn parse_db_cmd(tail: &[&str]) -> Result<DbCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing db subcommand (purge|backup|restore|migrate|v2import)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "purge" => Ok(DbCommand::Purge(parse_db_purge_args(rest)?)),
        "backup" => parse_backup_cmd(rest),
        "restore" => Ok(DbCommand::Restore(parse_restore_args(rest)?)),
        "migrate" => Ok(DbCommand::Migrate(parse_migrate_cmd(rest)?)),
        "v2import" => Ok(DbCommand::V2import(parse_db_import(rest)?)),
        "import" => Err(ParseError::InvalidArg(
            "unknown db subcommand: 'import' (the legacy import is `db v2import`)".into(),
        )),
        other => Err(ParseError::InvalidArg(format!(
            "unknown db subcommand: '{other}'"
        ))),
    }
}

pub(crate) fn parse_db_purge_args(tail: &[&str]) -> Result<DbPurgeArgs> {
    let mut args = DbPurgeArgs::default();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("yes") | FlagSplit::Short('y') => args.yes = true,
            FlagSplit::Long("leave") => args.leave = true,
            FlagSplit::Long("rejoin") => args.rejoin = true,
            FlagSplit::Long("force") => args.force = true,
            f => return Err(unknown_flag(f)),
        }
    }
    if args.leave && args.rejoin {
        return Err(ParseError::InvalidArg(
            "db purge: --leave or --rejoin, not both".into(),
        ));
    }
    if args.force && !args.leave {
        return Err(ParseError::InvalidArg(
            "db purge: --force only goes with --leave".into(),
        ));
    }
    Ok(args)
}

pub(crate) fn parse_completions_cmd(tail: &[&str]) -> Result<CompletionsCommand> {
    let shells = "zsh|bash|fish";
    let shell = |name: &str| {
        Shell::from_name(name)
            .ok_or_else(|| ParseError::InvalidArg(format!("unknown shell '{name}' ({shells})")))
    };
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(format!(
            "missing completions subcommand ({shells}|install|uninstall)"
        )));
    };
    let chosen = match &tail[1..] {
        [] => None,
        ["--shell", name] => Some(shell(name)?),
        [flag] if flag.starts_with("--shell=") => Some(shell(&flag["--shell=".len()..])?),
        [tok, ..] => return Err(unknown_flag(split_flag(tok))),
    };
    match *sub {
        "install" => Ok(CompletionsCommand::Install(chosen)),
        "uninstall" => Ok(CompletionsCommand::Uninstall(chosen)),
        name if chosen.is_none() && tail.len() == 1 => {
            Ok(CompletionsCommand::Script(shell(name).map_err(|_| {
                ParseError::InvalidArg(format!(
                    "unknown completions subcommand: '{name}' ({shells}|install|uninstall)"
                ))
            })?))
        }
        _ => Err(unknown_flag(split_flag(tail[1]))),
    }
}

pub(crate) fn parse_config_cmd(tail: &[&str]) -> Result<ConfigCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing config subcommand (show)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "show" => {
            let args = ConfigShowArgs::default();
            if let Some(tok) = rest.first() {
                return Err(unknown_flag(split_flag(tok)));
            }
            Ok(ConfigCommand::Show(args))
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown config subcommand: '{other}'"
        ))),
    }
}
