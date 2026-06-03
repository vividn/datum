use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_migrate_cmd(tail: &[&str]) -> Result<MigrateCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing migrate subcommand (run|status|edit)".into(),
        ));
    };
    match *sub {
        "run" => Ok(MigrateCommand::Run),
        "status" => Ok(MigrateCommand::Status),
        "edit" => Ok(MigrateCommand::Edit),
        other => Err(ParseError::InvalidArg(format!(
            "unknown migrate subcommand: '{other}'"
        ))),
    }
}

pub(crate) fn parse_db_import(tail: &[&str]) -> Result<DbImportArgs> {
    let mut out = DbImportArgs::default();
    let mut pos: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
            }
            FlagSplit::Long("from") | FlagSplit::Long("backup") => {
                let (v, ni) = take_value(tail, i, None, "--from")?;
                out.backup_file = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("from", v) | FlagSplit::LongAttached("backup", v) => {
                out.backup_file = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("dry-run") => {
                out.dry_run = true;
                i += 1;
            }
            FlagSplit::Long("skip-log") => {
                let (v, ni) = take_value(tail, i, None, "--skip-log")?;
                out.skip_log = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("skip-log", v) => {
                out.skip_log = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    if let Some(p) = pos.first() {
        out.backup_file_positional = Some(p.to_string());
    }
    Ok(out)
}

pub(crate) fn parse_backup_args(tail: &[&str]) -> Result<BackupArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut overwrite = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("overwrite") => overwrite = true,
            f => return Err(unknown_flag(f)),
        }
    }
    let path = require_pos(&pos, "<PATH>")?;
    Ok(BackupArgs { path, overwrite })
}

pub(crate) fn parse_restore_args(tail: &[&str]) -> Result<RestoreArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut force = false;
    let mut dry_run = false;
    let mut json = false;
    let mut cluster = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("force") => force = true,
            FlagSplit::Long("dry-run") => dry_run = true,
            FlagSplit::Long("json") => json = true,
            FlagSplit::Long("cluster") => cluster = true,
            f => return Err(unknown_flag(f)),
        }
    }
    let path = require_pos(&pos, "<PATH>")?;
    Ok(RestoreArgs {
        path,
        force,
        dry_run,
        json,
        cluster,
    })
}
