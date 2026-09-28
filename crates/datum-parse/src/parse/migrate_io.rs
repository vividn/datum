use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::Show;

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

/// `db backup`: a single file (`<PATH> [--overwrite]`), or — with `--to`,
/// `--list` or `--plan` — the Tower of Hanoi rotation (da-d5cmj).
pub(crate) fn parse_backup_cmd(tail: &[&str]) -> Result<DbCommand> {
    let mut pos: Vec<&str> = Vec::new();
    let (mut overwrite, mut plan) = (false, false);
    let (mut to, mut list, mut every, mut slots, mut span) = (None, None, None, None, None);
    let secs = |v: &str| -> Result<i64> {
        match crate::time::parse_duration(v)?.num_seconds() {
            n if n > 0 => Ok(n),
            _ => Err(ParseError::InvalidArg(format!(
                "duration must be positive: {v}"
            ))),
        }
    };
    const VALUED: [&str; 5] = ["to", "list", "every", "slots", "span"];
    let mut i = 0;
    while i < tail.len() {
        let (name, attached) = match split_flag(tail[i]) {
            FlagSplit::Positional(s) => {
                pos.push(s);
                i += 1;
                continue;
            }
            FlagSplit::Long("overwrite") => {
                overwrite = true;
                i += 1;
                continue;
            }
            FlagSplit::Long("plan") => {
                plan = true;
                i += 1;
                continue;
            }
            FlagSplit::Long(n) if VALUED.contains(&n) => (n, None),
            FlagSplit::LongAttached(n, v) if VALUED.contains(&n) => (n, Some(v)),
            f => return Err(unknown_flag(f)),
        };
        let (v, ni) = take_value(tail, i, attached, &format!("--{name}"))?;
        i = ni;
        match name {
            "to" => to = Some(v.to_string()),
            "list" => list = Some(v.to_string()),
            "every" => every = Some(secs(v)?),
            "span" => span = Some(secs(v)?),
            _ => {
                slots = Some(v.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| {
                    ParseError::InvalidArg(format!("--slots takes a count of 1 or more: {v}"))
                })?)
            }
        }
    }
    let action = match (to, list, plan) {
        (None, None, false) => {
            if every.is_some() || slots.is_some() || span.is_some() {
                return Err(ParseError::InvalidArg(
                    "--every, --slots and --span belong to --to or --plan".into(),
                ));
            }
            let path = require_pos(&pos, "<PATH>")?;
            return Ok(DbCommand::Backup(BackupArgs { path, overwrite }));
        }
        (Some(dir), None, false) => RotationAction::To(dir),
        (None, Some(dir), false) => RotationAction::List(dir),
        (None, None, true) => RotationAction::Plan {
            span: span.ok_or_else(|| ParseError::InvalidArg("--plan requires --span".into()))?,
        },
        _ => {
            return Err(ParseError::InvalidArg(
                "--to, --list and --plan are one each".into(),
            ))
        }
    };
    if !pos.is_empty() || overwrite {
        return Err(ParseError::InvalidArg(
            "a rotation takes no <PATH> and no --overwrite".into(),
        ));
    }
    // A flag the action never reads is refused, not ignored.
    let stray = match action {
        RotationAction::List(_) => every.is_some() || slots.is_some() || span.is_some(),
        RotationAction::To(_) => span.is_some(),
        RotationAction::Plan { .. } => false,
    };
    if stray {
        return Err(ParseError::InvalidArg(
            "--list takes only DIR, and --span belongs to --plan".into(),
        ));
    }
    let every = match (&action, every) {
        (RotationAction::List(_), _) => 0,
        (_, Some(e)) => e,
        _ => {
            return Err(ParseError::InvalidArg(
                "--to and --plan require --every".into(),
            ))
        }
    };
    Ok(DbCommand::BackupRotation(BackupRotationArgs {
        action,
        every,
        slots,
        show: Show::default(),
    }))
}

pub(crate) fn parse_restore_args(tail: &[&str]) -> Result<RestoreArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut force = false;
    let mut dry_run = false;
    let mut merge = false;
    let mut hard = false;
    let mut yes = false;
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("force") => force = true,
            FlagSplit::Long("dry-run") => dry_run = true,
            FlagSplit::Long("merge") => merge = true,
            FlagSplit::Long("hard") => hard = true,
            FlagSplit::Long("yes") | FlagSplit::Short('y') => yes = true,
            f => return Err(unknown_flag(f)),
        }
    }
    if hard && (merge || force) {
        return Err(ParseError::InvalidArg(format!(
            "--hard and --{} refuse together: --hard writes the backup verbatim over an emptied database",
            if merge { "merge" } else { "force" }
        )));
    }
    let path = require_pos(&pos, "<PATH>")?;
    Ok(RestoreArgs {
        path,
        force,
        dry_run,
        merge,
        hard,
        yes,
        show: Show::default(),
    })
}
