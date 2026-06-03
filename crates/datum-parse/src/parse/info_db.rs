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
        target: pos.first().map(|s| s.to_string()),
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

pub(crate) fn parse_setup_args(tail: &[&str]) -> Result<SetupArgs> {
    if let Some(tok) = tail.first() {
        return Err(unknown_flag(split_flag(tok)));
    }
    Ok(SetupArgs {})
}

pub(crate) fn parse_db_cmd(tail: &[&str]) -> Result<DbCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing db subcommand (setup|purge|backup|restore|migrate|import)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "setup" => Ok(DbCommand::Setup(parse_setup_args(rest)?)),
        "purge" => Ok(DbCommand::Purge(parse_db_purge_args(rest)?)),
        "backup" => Ok(DbCommand::Backup(parse_backup_args(rest)?)),
        "restore" => Ok(DbCommand::Restore(parse_restore_args(rest)?)),
        "migrate" => Ok(DbCommand::Migrate(parse_migrate_cmd(rest)?)),
        "import" => Ok(DbCommand::Import(parse_db_import(rest)?)),
        other => Err(ParseError::InvalidArg(format!(
            "unknown db subcommand: '{other}'"
        ))),
    }
}

pub(crate) fn parse_db_purge_args(tail: &[&str]) -> Result<DbPurgeArgs> {
    let mut yes = false;
    let mut cluster = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("yes") | FlagSplit::Short('y') => yes = true,
            FlagSplit::Long("cluster") => cluster = true,
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(DbPurgeArgs { yes, cluster })
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
            let mut args = ConfigShowArgs::default();
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Long("json") => args.json = true,
                    f => return Err(unknown_flag(f)),
                }
            }
            Ok(ConfigCommand::Show(args))
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown config subcommand: '{other}'"
        ))),
    }
}
