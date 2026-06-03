use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_sync_args(tail: &[&str]) -> Result<SyncArgs> {
    // sync is unusual: it has its OWN flags AND an optional subcommand.
    // Walk flags first; the first positional must be one of the known
    // subcommand names (else it's an unexpected positional error).
    let mut args = SyncArgs::default();
    const SUBS: &[&str] = &["login", "config", "key", "status", "install-daemon"];

    let mut i = 0;
    while i < tail.len() {
        let tok = tail[i];
        match split_flag(tok) {
            FlagSplit::Positional(s) => {
                if SUBS.contains(&s) {
                    let rest: &[&str] = &tail[i + 1..];
                    args.command = Some(match s {
                        "login" => SyncCommand::Login(parse_sync_login(rest)?),
                        "config" => SyncCommand::Config(parse_sync_config(rest)?),
                        "key" => SyncCommand::Key(parse_sync_key(rest)?),
                        "status" => SyncCommand::Status(parse_sync_status(rest)?),
                        "install-daemon" => SyncCommand::InstallDaemon(parse_sync_install(rest)?),
                        _ => unreachable!(),
                    });
                    return Ok(args);
                }
                return Err(ParseError::InvalidArg(format!(
                    "unexpected positional: {s}"
                )));
            }
            FlagSplit::Long("push") => {
                args.push = true;
                i += 1;
            }
            FlagSplit::Long("pull") => {
                args.pull = true;
                i += 1;
            }
            FlagSplit::Long("dry-run") => {
                args.dry_run = true;
                i += 1;
            }
            FlagSplit::Long("server") => {
                let (v, ni) = take_value(tail, i, None, "--server")?;
                args.server = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("server", v) => {
                args.server = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    if args.push && args.pull {
        return Err(ParseError::InvalidArg(
            "--push and --pull are mutually exclusive".into(),
        ));
    }
    Ok(args)
}

pub(crate) fn parse_sync_login(tail: &[&str]) -> Result<SyncLoginArgs> {
    let mut out = SyncLoginArgs::default();
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("server") => {
                let (v, ni) = take_value(tail, i, None, "--server")?;
                out.server = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("server", v) => {
                out.server = Some(v.to_string());
                i += 1;
            }
            FlagSplit::Long("cluster") => {
                let (v, ni) = take_value(tail, i, None, "--cluster")?;
                out.cluster = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("cluster", v) => {
                out.cluster = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

pub(crate) fn parse_sync_status(tail: &[&str]) -> Result<SyncStatusArgs> {
    let mut out = SyncStatusArgs::default();
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("server") => {
                let (v, ni) = take_value(tail, i, None, "--server")?;
                out.server = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("server", v) => {
                out.server = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

pub(crate) fn parse_sync_install(tail: &[&str]) -> Result<SyncInstallDaemonArgs> {
    let mut out = SyncInstallDaemonArgs::default();
    for tok in tail {
        match split_flag(tok) {
            FlagSplit::Long("show") => out.show = true,
            FlagSplit::Long("uninstall") => out.uninstall = true,
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(out)
}

pub(crate) fn parse_sync_config(tail: &[&str]) -> Result<SyncConfigCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing sync config subcommand (set-server|disable|show)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "set-server" => {
            let mut url: Option<String> = None;
            let mut global = false;
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Positional(s) => {
                        if url.is_none() {
                            url = Some(s.to_string());
                        } else {
                            return Err(ParseError::InvalidArg(format!(
                                "unexpected extra positional: {s}"
                            )));
                        }
                    }
                    FlagSplit::Long("global") => global = true,
                    f => return Err(unknown_flag(f)),
                }
            }
            let url = url.ok_or_else(|| {
                ParseError::InvalidArg("sync config set-server requires <URL>".into())
            })?;
            Ok(SyncConfigCommand::SetServer { url, global })
        }
        "disable" => Ok(SyncConfigCommand::Disable),
        "show" => Ok(SyncConfigCommand::Show),
        other => Err(ParseError::InvalidArg(format!(
            "unknown sync config subcommand: '{other}'"
        ))),
    }
}

pub(crate) fn parse_sync_key(tail: &[&str]) -> Result<SyncKeyCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing sync key subcommand (init|unlock|rotate-passphrase|show-fingerprint)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "init" => {
            let mut server: Option<String> = None;
            let mut force = false;
            let mut i = 0;
            while i < rest.len() {
                match split_flag(rest[i]) {
                    FlagSplit::Long("server") => {
                        let (v, ni) = take_value(rest, i, None, "--server")?;
                        server = Some(v.to_string());
                        i = ni;
                    }
                    FlagSplit::LongAttached("server", v) => {
                        server = Some(v.to_string());
                        i += 1;
                    }
                    FlagSplit::Long("force") => {
                        force = true;
                        i += 1;
                    }
                    f => return Err(unknown_flag(f)),
                }
            }
            Ok(SyncKeyCommand::Init { server, force })
        }
        "unlock" => Ok(SyncKeyCommand::Unlock {
            server: take_server_only(rest)?,
        }),
        "rotate-passphrase" => Ok(SyncKeyCommand::RotatePassphrase {
            server: take_server_only(rest)?,
        }),
        "show-fingerprint" => Ok(SyncKeyCommand::ShowFingerprint {
            server: take_server_only(rest)?,
        }),
        other => Err(ParseError::InvalidArg(format!(
            "unknown sync key subcommand: '{other}'"
        ))),
    }
}

pub(crate) fn take_server_only(tail: &[&str]) -> Result<Option<String>> {
    let mut server: Option<String> = None;
    let mut i = 0;
    while i < tail.len() {
        match split_flag(tail[i]) {
            FlagSplit::Long("server") => {
                let (v, ni) = take_value(tail, i, None, "--server")?;
                server = Some(v.to_string());
                i = ni;
            }
            FlagSplit::LongAttached("server", v) => {
                server = Some(v.to_string());
                i += 1;
            }
            f => return Err(unknown_flag(f)),
        }
    }
    Ok(server)
}
