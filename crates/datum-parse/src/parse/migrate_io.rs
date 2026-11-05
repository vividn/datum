use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::Show;

pub(crate) fn parse_migrate_cmd(tail: &[&str]) -> Result<MigrateCommand> {
    let Some(sub) = tail.first() else {
        return Ok(MigrateCommand::Run);
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
    out.backup_file_positional = optional_pos(&pos)?;
    Ok(out)
}

/// `datum backup` (`db backup` its other name, da-tsna7; vividn, 2026-11-04:
/// «a user just trying to make a manual backup doesn't get a hanoi setup
/// going»): `<PATH>` one file, and nothing else; `--auto` or `--every` a
/// Tower of Hanoi rotation in DIR on a timer (da-d5cmj), `--no-auto` its
/// end; `--dry-run` (`--plan`) what one would set up; `--list DIR`;
/// `status`. `--to DIR` alone is the timer's step, which reads its period
/// from DIR. `daemon install | uninstall | status` are the older spellings.
pub(crate) fn parse_backup_cmd(tail: &[&str]) -> Result<DbCommand> {
    let invalid = |m: &str| Err(ParseError::InvalidArg(m.into()));
    // A word here, never a file: `./status` and `./daemon` are the files.
    match tail {
        ["status"] => return Ok(DbCommand::BackupDaemon(BackupDaemonAct::Status)),
        ["status", extra, ..] => {
            return Err(ParseError::InvalidArg(format!(
                "backup status takes nothing more: {extra}"
            )))
        }
        ["daemon", rest @ ..] => return parse_backup_daemon(rest),
        _ => {}
    }
    let mut pos: Vec<&str> = Vec::new();
    let (mut overwrite, mut dry_run, mut all) = (false, false, false);
    let mut auto: Option<bool> = None;
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
            FlagSplit::Long(
                n @ ("overwrite" | "plan" | "dry-run" | "all" | "auto" | "no-auto"),
            ) => {
                match n {
                    "overwrite" => overwrite = true,
                    "all" => all = true,
                    "plan" | "dry-run" => dry_run = true,
                    on => match auto.replace(on == "auto") {
                        Some(was) if was != (on == "auto") => {
                            return invalid("--auto or --no-auto, not both")
                        }
                        _ => {}
                    },
                }
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
    let file = !pos.is_empty() || overwrite;
    let rotation = |action, every| {
        Ok(DbCommand::BackupRotation(BackupRotationArgs {
            action,
            every,
            slots,
            all,
            show: Show::default(),
        }))
    };
    if file {
        if to.is_some() || list.is_some() || dry_run || auto.is_some() || every.is_some() {
            return invalid(
                "a backup to <PATH> is one file: a rotation takes no <PATH> and no --overwrite",
            );
        }
        if slots.is_some() || span.is_some() || all {
            return invalid("--slots, --span and --all belong to a rotation");
        }
        let path = require_pos(&pos, "<PATH>")?;
        return Ok(DbCommand::Backup(BackupArgs { path, overwrite }));
    }
    if let Some(dir) = list {
        let other = to.is_some() || dry_run || auto.is_some() || every.is_some();
        if other || slots.is_some() || span.is_some() || all {
            return invalid("--list takes only DIR");
        }
        return rotation(RotationAction::List(dir), None);
    }
    if span.is_some() && !dry_run {
        return invalid("--span belongs to --dry-run");
    }
    if dry_run {
        if auto == Some(false) {
            return invalid("--dry-run shows what a rotation would set up: not with --no-auto");
        }
        let every = every.or(Some(BACKUP_EVERY_SECS));
        return rotation(RotationAction::Plan { span, to }, every);
    }
    match (auto, every) {
        (Some(false), None) if slots.is_none() => {
            Ok(DbCommand::BackupDaemon(BackupDaemonAct::Uninstall {
                to,
                all,
            }))
        }
        (Some(false), _) => invalid("--no-auto takes [--to DIR] [--all], no --every or --slots"),
        (Some(true), _) | (None, Some(_)) => {
            Ok(DbCommand::BackupDaemon(BackupDaemonAct::Install {
                to,
                every: every.unwrap_or(BACKUP_EVERY_SECS),
                slots,
                all,
            }))
        }
        (None, None) => match to {
            // The timer's step: the period and cap are DIR's.
            Some(dir) if slots.is_none() => rotation(RotationAction::To(dir), None),
            Some(_) => invalid("--slots goes with --auto or --every"),
            None if all || slots.is_some() => {
                invalid("--slots and --all belong to a rotation: --auto or --every")
            }
            None => require_pos(&pos, "<PATH>")
                .map(|path| DbCommand::Backup(BackupArgs { path, overwrite })),
        },
    }
}

/// `backup daemon install | uninstall | status`, as they were spelled
/// before `--auto` (da-gbac0): install takes `--every`, unlike `--auto`.
fn parse_backup_daemon(rest: &[&str]) -> Result<DbCommand> {
    let act =
        match rest {
            ["install", flags @ ..] => {
                let every = (flags.iter()).any(|f| *f == "--every" || f.starts_with("--every="));
                match parse_backup_cmd(flags)? {
                    DbCommand::BackupDaemon(act @ BackupDaemonAct::Install { .. }) if every => act,
                    _ => return Err(ParseError::InvalidArg(
                        "backup daemon install takes [--to DIR] --every DUR [--slots N] [--all]"
                            .into(),
                    )),
                }
            }
            ["uninstall", flags @ ..] if flags.len() <= 3 => {
                let all = flags.contains(&"--all");
                let rest: Vec<&str> = flags.iter().copied().filter(|f| *f != "--all").collect();
                let to = match rest[..] {
                    [] => None,
                    ["--to", to] => Some(to.to_string()),
                    [flag] if flag.starts_with("--to=") => Some(flag["--to=".len()..].to_string()),
                    [extra, ..] => {
                        return Err(ParseError::InvalidArg(format!(
                            "backup daemon uninstall takes [--to DIR] [--all]: {extra}"
                        )))
                    }
                };
                BackupDaemonAct::Uninstall { to, all }
            }
            ["status"] => BackupDaemonAct::Status,
            [act @ ("uninstall" | "status"), extra, ..] => {
                return Err(ParseError::InvalidArg(format!(
                    "backup daemon {act} takes nothing more: {extra}"
                )))
            }
            _ => {
                return Err(ParseError::InvalidArg(
                    "backup daemon takes install, uninstall or status".into(),
                ))
            }
        };
    Ok(DbCommand::BackupDaemon(act))
}

pub(crate) fn parse_restore_args(tail: &[&str]) -> Result<RestoreArgs> {
    let mut pos: Vec<&str> = Vec::new();
    let mut force = false;
    let mut dry_run = false;
    let mut merge = false;
    let mut hard = false;
    let mut leave = false;
    let mut chain = false;
    let mut admin_key = None;
    let mut approve_link = None;
    let mut approval_code = None;
    let mut yes = false;
    let mut if_unchanged = None;
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Positional(s) => pos.push(s),
            FlagSplit::Long("force") => force = true,
            FlagSplit::Long("dry-run") => dry_run = true,
            FlagSplit::Long("merge") => merge = true,
            FlagSplit::Long("hard") => hard = true,
            FlagSplit::Long("leave") => leave = true,
            FlagSplit::Long("chain") => chain = true,
            FlagSplit::Long("yes") | FlagSplit::Short('y') => yes = true,
            FlagSplit::Long("admin-key") | FlagSplit::LongAttached("admin-key", _) => {
                let attached = tail[i].split_once('=').map(|(_, v)| v);
                let (v, ni) = take_value(tail, i, attached, "--admin-key")?;
                admin_key = Some(v.to_string());
                i = ni;
                continue;
            }
            FlagSplit::Long(f @ ("approve-link" | "approval-code"))
            | FlagSplit::LongAttached(f @ ("approve-link" | "approval-code"), _) => {
                let attached = tail[i].split_once('=').map(|(_, v)| v);
                let (v, ni) = take_value(tail, i, attached, &format!("--{f}"))?;
                *match f {
                    "approve-link" => &mut approve_link,
                    _ => &mut approval_code,
                } = Some(v.to_string());
                i = ni;
                continue;
            }
            FlagSplit::Long("if-unchanged") | FlagSplit::LongAttached("if-unchanged", _) => {
                let attached = tail[i].split_once('=').map(|(_, v)| v);
                let (v, ni) = take_value(tail, i, attached, "--if-unchanged")?;
                if_unchanged = Some(u64::from_str_radix(v, 16).map_err(|_| {
                    ParseError::InvalidArg(format!(
                        "--if-unchanged takes the fingerprint `--dry-run --json` printed: {v}"
                    ))
                })?);
                i = ni;
                continue;
            }
            f => return Err(unknown_flag(f)),
        }
        i += 1;
    }
    if if_unchanged.is_some() && merge {
        return Err(ParseError::InvalidArg(
            "--if-unchanged pins a replace; --merge deletes nothing".into(),
        ));
    }
    if hard && (merge || force) {
        return Err(ParseError::InvalidArg(format!(
            "--hard and --{} refuse together: --hard writes the backup verbatim over an emptied database",
            if merge { "merge" } else { "force" }
        )));
    }
    if leave && !hard {
        return Err(ParseError::InvalidArg(
            "--leave is for --hard: no other restore drops this copy's sync ops".into(),
        ));
    }
    if chain && (leave || !hard) {
        return Err(ParseError::InvalidArg(
            "--chain is for --hard, without --leave: it restores every copy in the chain".into(),
        ));
    }
    if admin_key.is_some() && !chain {
        return Err(ParseError::InvalidArg(
            "--admin-key is for --hard --chain, an admin act on the chain".into(),
        ));
    }
    let approval = match (approve_link, approval_code) {
        (Some(link), Some(code)) if chain && admin_key.is_none() => Some((link, code)),
        (None, None) => None,
        _ => {
            return Err(ParseError::InvalidArg(
                "--approve-link and --approval-code go together, with --hard --chain and \
                 without --admin-key: the order a passkey approved"
                    .into(),
            ))
        }
    };
    let path = require_pos(&pos, "<PATH>")?;
    Ok(RestoreArgs {
        path,
        force,
        dry_run,
        merge,
        hard,
        leave,
        chain,
        admin_key,
        approval,
        yes,
        if_unchanged,
        show: Show::default(),
    })
}
