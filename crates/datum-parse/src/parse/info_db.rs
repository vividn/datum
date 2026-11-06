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
            "missing db subcommand (purge|backup|restore|migrate|v2import|config|rename)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "purge" => Ok(DbCommand::Purge(parse_db_purge_args(rest)?)),
        "backup" => parse_backup_cmd(rest),
        "restore" => Ok(DbCommand::Restore(parse_restore_args(rest)?)),
        "migrate" => Ok(DbCommand::Migrate(parse_migrate_cmd(rest)?)),
        "v2import" => Ok(DbCommand::V2import(parse_db_import(rest)?)),
        "config" => Ok(DbCommand::Config(parse_db_config(rest)?)),
        "rename" => match rest {
            [new] if !new.starts_with('-') && !new.is_empty() => {
                Ok(DbCommand::Rename(new.to_string()))
            }
            _ => Err(ParseError::InvalidArg(
                "db rename takes the database's new name: `datum --db OLD db rename NEW`".into(),
            )),
        },
        "import" => Err(ParseError::InvalidArg(
            "unknown db subcommand: 'import' (the legacy import is `db v2import`)".into(),
        )),
        other => Err(ParseError::InvalidArg(format!(
            "unknown db subcommand: '{other}'"
        ))),
    }
}

/// `datum db config [KEY [VALUE]]`: `timezone_follow` is `ask` or `auto`, any
/// other setting `true` or `false`.
fn parse_db_config(tail: &[&str]) -> Result<DbConfigArgs> {
    let mut pos: Vec<&str> = Vec::new();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            f => return Err(unknown_flag(f)),
        }
    }
    let value = match (pos.first(), pos.get(1)) {
        (_, None) => None,
        (Some(&"timezone_follow"), Some(&v)) if v == "ask" || v == "auto" => Some(v.into()),
        (Some(&"timezone_follow"), Some(v)) => {
            return Err(ParseError::InvalidArg(format!(
                "db config: timezone_follow is ask or auto, got '{v}' (not tracking is `timezone IGNORED`)"
            )))
        }
        (_, Some(&"true")) => Some(true.into()),
        (_, Some(&"false")) => Some(false.into()),
        (_, Some(v)) => {
            return Err(ParseError::InvalidArg(format!(
                "db config: a setting is true or false, got '{v}'"
            )))
        }
    };
    if pos.len() > 2 {
        return Err(ParseError::InvalidArg(
            "db config takes a setting and its value, no more".into(),
        ));
    }
    Ok(DbConfigArgs {
        key: pos.first().map(|s| s.to_string()),
        value,
    })
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

pub(crate) fn parse_install_args(tail: &[&str]) -> Result<InstallArgs> {
    let mut args = InstallArgs::default();
    let mut words = tail.iter();
    while let Some(&tok) = words.next() {
        let name = match split_flag(tok) {
            FlagSplit::Long("daemon") => {
                args.daemon = true;
                continue;
            }
            FlagSplit::Long("no-completions") => {
                args.no_completions = true;
                continue;
            }
            FlagSplit::Long(yes @ ("yes" | "no")) => {
                args.auto_upgrade = Some(yes == "yes");
                continue;
            }
            FlagSplit::Long("shell") => words.next().copied().unwrap_or(""),
            FlagSplit::LongAttached("shell", name) => name,
            other => return Err(unknown_flag(other)),
        };
        args.shell = Some(Shell::from_name(name).ok_or_else(|| {
            ParseError::InvalidArg(format!("unknown shell '{name}' (zsh|bash|fish)"))
        })?);
    }
    Ok(args)
}

pub(crate) fn parse_upgrade_args(tail: &[&str]) -> Result<Command> {
    let (mut check, mut auto, mut to, mut past_pin) = (false, None, None, None);
    let mut words = tail.iter();
    while let Some(tok) = words.next() {
        let version = match split_flag(tok) {
            // `status`, as `backup status` and `sync status` say it (da-tsna7).
            FlagSplit::Long("check") | FlagSplit::Positional("status") => {
                check = true;
                continue;
            }
            // `--auto` and `--no-auto` stand alone (da-f7ckm).
            FlagSplit::Long(on @ ("auto" | "no-auto")) => {
                if let Some(extra) = tail.iter().find(|t| *t != tok) {
                    return Err(ParseError::InvalidArg(format!(
                        "unexpected argument: '{extra}'"
                    )));
                }
                auto = Some(on == "auto");
                continue;
            }
            FlagSplit::Long(how @ ("force-upgrade" | "leave-behind" | "daily")) => {
                let how = match how {
                    "force-upgrade" => PastPin::Force,
                    "leave-behind" => PastPin::LeaveBehind,
                    _ => PastPin::Daily,
                };
                if past_pin.is_some_and(|was| was != how) {
                    return Err(ParseError::InvalidArg(
                        "--force-upgrade or --leave-behind, not both".into(),
                    ));
                }
                past_pin = Some(how);
                continue;
            }
            FlagSplit::Long("to") => words.next().copied().ok_or_else(|| {
                ParseError::InvalidArg(
                    "--to takes a version: 3.<minor> or 3.<minor>.<height>".into(),
                )
            })?,
            FlagSplit::LongAttached("to", v) => v,
            FlagSplit::Positional(extra) => {
                return Err(ParseError::InvalidArg(format!(
                    "unexpected argument: '{extra}'"
                )))
            }
            other => return Err(unknown_flag(other)),
        };
        to = Some(release_version("--to takes", version)?);
    }
    // The chain's ask, which `--force-upgrade` writes, names the newest.
    let with = match (check, to.is_some()) {
        (true, true) => "--check or --to",
        (true, false) => "--check",
        (false, true) => "--to",
        (false, false) => "",
    };
    if past_pin == Some(PastPin::Force) && !with.is_empty() {
        return Err(ParseError::InvalidArg(format!(
            "--force-upgrade upgrades the whole chain to the newest release: not with {with}"
        )));
    }
    Ok(Command::Upgrade {
        check,
        auto,
        to,
        past_pin,
    })
}

/// A kept release's version as `datum --version` prints it, `3.<minor>.<height>`,
/// or `3.<minor>` alone for that minor's newest build, with or without its
/// tag's `v` and its `-beta`: two numbers or three. `what` begins the refusal.
pub(crate) fn release_version(what: &str, v: &str) -> Result<String> {
    let core = v.strip_prefix('v').unwrap_or(v);
    let core = core.split_once('-').map_or(core, |(c, _)| c);
    let parts: Vec<&str> = core.split('.').collect();
    if (parts.len() == 2 || parts.len() == 3)
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        Ok(core.to_string())
    } else {
        Err(ParseError::InvalidArg(format!(
            "{what} a version, 3.<minor> or 3.<minor>.<height> as `datum --version` prints it: not '{v}'"
        )))
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
