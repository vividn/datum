//! `datum sync` — Easy Sync. The core (da-k329q) moves ops as JSON lines on
//! stdin/stdout, so two copies sync through a pipe:
//! `datum --db b sync pipe ops --since "$(datum --db a sync pipe vector)" |
//! datum --db a sync pipe apply`; the rest syncs the members of a sync chain
//! over iroh (da-rugg8, da-6177s). The tree (da-aam46): bare `sync`,
//! `status`, `pause | resume`, `chain …`, `admin …`; `pipe …`, `run` and
//! datum.day's store commands are hidden.

use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::Show;

pub(crate) fn parse_sync_cmd(tail: &[&str]) -> Result<SyncCommand> {
    let Some((sub, rest)) = tail.split_first() else {
        return Ok(SyncCommand::Now);
    };
    let mut rest = rest.iter();
    let cmd = match *sub {
        // `--json` is the swept `--show json`, set by `apply_show`.
        "status" => SyncCommand::Status {
            show: Show::Default,
        },
        "pause" => SyncCommand::Pause,
        "resume" => SyncCommand::Resume,
        "help" => return Err(ParseError::HelpRequested(Some("sync".into()))),
        "chain" => parse_chain(&mut rest)?,
        "admin" => parse_admin(&mut rest)?,
        // Hidden: the pipe transport, the daemon, and datum.day's store.
        "pipe" => parse_pipe(&mut rest)?,
        "run" => SyncCommand::Run,
        // Hidden (da-rcb5m): datum.day's network worker, not a person.
        "endpoint-key" => SyncCommand::EndpointKey,
        "record-session" => {
            let mut o = opts(&mut rest, &["vector", "verify", "error"], &[], 1..=1)?;
            let verify = match o.get("verify").as_deref() {
                None => None,
                Some("match") => Some(true),
                Some("mismatch") => Some(false),
                Some(v) => {
                    return Err(ParseError::InvalidArg(format!(
                        "--verify is match or mismatch, not {v}"
                    )))
                }
            };
            SyncCommand::RecordSession {
                endpoint: o.positional.remove(0),
                vector: o.get("vector"),
                verify,
                error: o.get("error"),
            }
        }
        // Hidden (da-6kpj5): datum.day's transport, as for `record-session`.
        "handshake" => {
            let mut o = opts(
                &mut rest,
                &["label", "addr", "invite", "chain"],
                &["dialed"],
                1..=1,
            )?;
            let need = |name: &str| {
                o.get(name)
                    .ok_or_else(|| ParseError::InvalidArg(format!("handshake needs --{name}")))
            };
            SyncCommand::Handshake {
                label: need("label")?,
                addr: need("addr")?,
                invite: o.get("invite"),
                chain: o.get("chain"),
                dialed: o.on("dialed"),
                endpoint: o.positional.remove(0),
            }
        }
        "new-invite" => {
            let o = opts(&mut rest, &["expires", "admin-key"], &["reader"], 0..=0)?;
            SyncCommand::NewInvite {
                reader: o.on("reader"),
                expires_secs: o.expires()?,
                admin_key: o.get("admin-key"),
            }
        }
        other => {
            return Err(unknown(
                "sync",
                other,
                "status|chain|admin|pause|resume|help",
            ))
        }
    };
    match rest.next() {
        Some(t) => Err(unknown_flag(split_flag(t))),
        None => Ok(cmd),
    }
}

type Rest<'a> = std::slice::Iter<'a, &'a str>;

/// `datum sync chain …`: the chain's membership.
fn parse_chain(rest: &mut Rest) -> Result<SyncCommand> {
    const SET: &str = "invite|connect|list|rename|revoke|role|ack|rekey";
    Ok(match rest.next().copied() {
        Some("invite") => {
            let o = opts(rest, &["expires", "admin-key"], &["qr", "reader"], 0..=0)?;
            SyncCommand::Invite {
                reader: o.on("reader"),
                qr: o.on("qr"),
                expires_secs: o.expires()?,
                admin_key: o.get("admin-key"),
            }
        }
        Some("connect") => SyncCommand::Connect {
            ticket: rest.next().map(|t| t.to_string()),
        },
        Some("list") => SyncCommand::Members {
            show: Show::Default,
        },
        Some("rename") => {
            let mut o = opts(rest, &["admin-key"], &[], 1..=2)?;
            let label = o.positional.pop().unwrap_or_default();
            SyncCommand::Rename {
                who: o.positional.pop(),
                label,
                admin_key: o.get("admin-key"),
            }
        }
        Some("revoke") => act(rest, SyncAdminAct::Revoke, &[], &[])?,
        Some("role") => {
            let mut o = opts(rest, &["admin-key"], &[], 2..=2)?;
            let reader = match o.positional.pop().as_deref() {
                Some("reader") => true,
                Some("member") => false,
                r => {
                    return Err(ParseError::InvalidArg(format!(
                        "a role is member or reader, not {}",
                        r.unwrap_or_default()
                    )))
                }
            };
            SyncCommand::Admin {
                act: SyncAdminAct::Role { reader },
                who: o.positional.remove(0),
                label: None,
                admin_key: o.get("admin-key"),
                allow_unlock: false,
            }
        }
        Some("ack") => SyncCommand::Ack {
            who: rest
                .next()
                .ok_or_else(|| ParseError::InvalidArg("ack needs an endpoint or admin".into()))?
                .to_string(),
        },
        Some("rekey") => SyncCommand::Rekey,
        Some(other) => return Err(unknown("sync chain", other, SET)),
        None => return Err(missing("sync chain", SET)),
    })
}

/// `datum sync admin …`: the keys that sign a locked chain's acts.
fn parse_admin(rest: &mut Rest) -> Result<SyncCommand> {
    const SET: &str = "list|add|remove";
    Ok(match rest.next().copied() {
        Some("list") => SyncCommand::AdminList {
            show: Show::Default,
        },
        Some("add") => act(rest, SyncAdminAct::AddAdmin, &["label"], &[])?,
        Some("remove") => act(rest, SyncAdminAct::RemoveAdmin, &[], &["allow-unlock"])?,
        Some(other) => return Err(unknown("sync admin", other, SET)),
        None => return Err(missing("sync admin", SET)),
    })
}

/// A signed act on one member or admin: `<WHO> [--admin-key <FILE>]`.
fn act(
    rest: &mut Rest,
    act: SyncAdminAct,
    values: &[&str],
    switches: &[&str],
) -> Result<SyncCommand> {
    let values: Vec<&str> = values.iter().copied().chain(["admin-key"]).collect();
    let mut o = opts(rest, &values, switches, 1..=1)?;
    Ok(SyncCommand::Admin {
        act,
        who: o.positional.remove(0),
        label: o.get("label"),
        admin_key: o.get("admin-key"),
        allow_unlock: o.on("allow-unlock"),
    })
}

/// Hidden: `datum sync pipe …`, ops as JSON lines on stdin and stdout.
fn parse_pipe(rest: &mut Rest) -> Result<SyncCommand> {
    const SET: &str = "vector|ops|apply|verify";
    Ok(match rest.next().copied() {
        Some("vector") => SyncCommand::Vector,
        // `--json` is the swept `--show json`, set by `apply_show`.
        Some("apply") => SyncCommand::Apply {
            show: Show::Default,
        },
        Some("verify") => {
            let verbose = match rest.next().map(|t| split_flag(t)) {
                Some(FlagSplit::Long("verbose") | FlagSplit::Short('v')) => true,
                Some(f) => return Err(unknown_flag(f)),
                None => false,
            };
            SyncCommand::Verify { verbose }
        }
        Some("ops") => {
            let o = opts(rest, &["since"], &["no-overlap"], 0..=0)?;
            SyncCommand::Ops {
                since: o.get("since"),
                overlap: !o.on("no-overlap"),
            }
        }
        Some(other) => return Err(unknown("sync pipe", other, SET)),
        None => return Err(missing("sync pipe", SET)),
    })
}

fn unknown(group: &str, word: &str, set: &str) -> ParseError {
    ParseError::InvalidArg(format!(
        "unknown {group} subcommand: '{word}' (expected {set})"
    ))
}

fn missing(group: &str, set: &str) -> ParseError {
    ParseError::InvalidArg(format!("missing {group} subcommand ({set})"))
}

/// A subcommand's flags: `values` take one (`--x V` or `--x=V`), `switches`
/// none, and as many bare words as `positionals` allows.
struct Opts {
    values: std::collections::BTreeMap<String, String>,
    switches: Vec<String>,
    positional: Vec<String>,
}

impl Opts {
    fn get(&self, name: &str) -> Option<String> {
        self.values.get(name).cloned()
    }

    fn on(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }

    /// `--expires`, 10 minutes by default.
    fn expires(&self) -> Result<u64> {
        let Some(value) = self.values.get("expires") else {
            return Ok(600);
        };
        u64::try_from(crate::time::parse_duration(value)?.num_seconds())
            .ok()
            .filter(|s| *s > 0)
            .ok_or_else(|| {
                ParseError::InvalidArg(format!("--expires must be a positive duration: {value}"))
            })
    }
}

fn opts<'a>(
    rest: &mut std::slice::Iter<'a, &'a str>,
    values: &[&str],
    switches: &[&str],
    positionals: std::ops::RangeInclusive<usize>,
) -> Result<Opts> {
    let mut o = Opts {
        values: Default::default(),
        switches: Vec::new(),
        positional: Vec::new(),
    };
    while let Some(t) = rest.next() {
        let (name, value) = match split_flag(t) {
            FlagSplit::Long(n) if switches.contains(&n) => {
                o.switches.push(n.to_string());
                continue;
            }
            FlagSplit::LongAttached(n, v) if values.contains(&n) => (n, v),
            FlagSplit::Long(n) if values.contains(&n) => (
                n,
                *rest
                    .next()
                    .ok_or_else(|| ParseError::InvalidArg(format!("--{n} needs a value")))?,
            ),
            _ if !t.starts_with('-') && o.positional.len() < *positionals.end() => {
                o.positional.push(t.to_string());
                continue;
            }
            f => return Err(unknown_flag(f)),
        };
        o.values.insert(name.to_string(), value.to_string());
    }
    if o.positional.len() < *positionals.start() {
        return Err(ParseError::InvalidArg(
            "this sync subcommand needs an argument".into(),
        ));
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(argv: &[&str]) -> SyncCommand {
        parse_sync_cmd(argv).unwrap()
    }

    #[test]
    fn parses_each_subcommand() {
        assert_eq!(p(&[]), SyncCommand::Now);
        assert_eq!(
            p(&["status"]),
            SyncCommand::Status {
                show: Show::Default
            }
        );
        assert_eq!(p(&["pause"]), SyncCommand::Pause);
        assert_eq!(p(&["resume"]), SyncCommand::Resume);
        assert!(matches!(
            parse_sync_cmd(&["help"]),
            Err(ParseError::HelpRequested(Some(s))) if s == "sync"
        ));
        assert_eq!(p(&["pipe", "vector"]), SyncCommand::Vector);
        assert_eq!(
            p(&["pipe", "apply"]),
            SyncCommand::Apply {
                show: Show::Default
            }
        );
        assert_eq!(
            p(&["pipe", "verify", "--verbose"]),
            SyncCommand::Verify { verbose: true }
        );
        assert_eq!(
            p(&["pipe", "ops", "--since={\"a\":1}"]),
            SyncCommand::Ops {
                since: Some("{\"a\":1}".into()),
                overlap: true
            }
        );
        assert_eq!(
            p(&["pipe", "ops", "--since", "{}", "--no-overlap"]),
            SyncCommand::Ops {
                since: Some("{}".into()),
                overlap: false
            }
        );
        assert_eq!(
            p(&["handshake", "ab", "--label", "l", "--addr={}", "--dialed"]),
            SyncCommand::Handshake {
                endpoint: "ab".into(),
                label: "l".into(),
                addr: "{}".into(),
                invite: None,
                chain: None,
                dialed: true
            }
        );
        assert!(parse_sync_cmd(&["handshake", "ab", "--label", "l"]).is_err());
        assert_eq!(
            p(&["new-invite", "--expires", "1m"]),
            SyncCommand::NewInvite {
                reader: false,
                expires_secs: 60,
                admin_key: None
            }
        );
        assert_eq!(p(&["run"]), SyncCommand::Run);
        assert_eq!(
            p(&["chain", "invite", "--admin-key=k", "--qr", "--reader"]),
            SyncCommand::Invite {
                reader: true,
                qr: true,
                expires_secs: 600,
                admin_key: Some("k".into())
            }
        );
        assert_eq!(
            p(&["chain", "connect"]),
            SyncCommand::Connect { ticket: None }
        );
        assert_eq!(
            p(&["chain", "list"]),
            SyncCommand::Members {
                show: Show::Default
            }
        );
        assert_eq!(
            p(&["chain", "rename", "abcd", "phone", "--admin-key", "k"]),
            SyncCommand::Rename {
                who: Some("abcd".into()),
                label: "phone".into(),
                admin_key: Some("k".into())
            }
        );
        assert_eq!(
            p(&["chain", "revoke", "b"]),
            SyncCommand::Admin {
                act: SyncAdminAct::Revoke,
                who: "b".into(),
                label: None,
                admin_key: None,
                allow_unlock: false
            }
        );
        assert_eq!(
            p(&["chain", "role", "b", "reader", "--admin-key", "k"]),
            SyncCommand::Admin {
                act: SyncAdminAct::Role { reader: true },
                who: "b".into(),
                label: None,
                admin_key: Some("k".into()),
                allow_unlock: false
            }
        );
        assert!(parse_sync_cmd(&["chain", "role", "b", "admin"]).is_err());
        assert!(parse_sync_cmd(&["chain", "role", "b"]).is_err());
        assert_eq!(
            p(&["chain", "ack", "abc"]),
            SyncCommand::Ack { who: "abc".into() }
        );
        assert_eq!(p(&["chain", "rekey"]), SyncCommand::Rekey);
        assert_eq!(
            p(&["admin", "list"]),
            SyncCommand::AdminList {
                show: Show::Default
            }
        );
        assert_eq!(
            p(&["admin", "add", "k", "--label", "desk"]),
            SyncCommand::Admin {
                act: SyncAdminAct::AddAdmin,
                who: "k".into(),
                label: Some("desk".into()),
                admin_key: None,
                allow_unlock: false
            }
        );
        assert_eq!(
            p(&["admin", "remove", "k", "--allow-unlock"]),
            SyncCommand::Admin {
                act: SyncAdminAct::RemoveAdmin,
                who: "k".into(),
                label: None,
                admin_key: None,
                allow_unlock: true
            }
        );
        // The flags are each act's own.
        assert!(parse_sync_cmd(&["admin", "add", "k", "--allow-unlock"]).is_err());
        assert!(parse_sync_cmd(&["chain", "revoke", "k", "--label", "x"]).is_err());
        for group in ["chain", "admin", "pipe"] {
            let err = parse_sync_cmd(&[group]).unwrap_err().to_string();
            assert!(
                err.contains(&format!("missing sync {group} subcommand")),
                "{err}"
            );
            assert!(parse_sync_cmd(&[group, "nope"]).is_err());
        }
        assert!(parse_sync_cmd(&["chain", "revoke"]).is_err());
        assert!(parse_sync_cmd(&["chain", "revoke", "a", "b"]).is_err());
        assert!(parse_sync_cmd(&["chain", "rename"]).is_err());
        assert!(parse_sync_cmd(&["chain", "rekey", "extra"]).is_err());
        assert!(parse_sync_cmd(&["chain", "invite", "--expires", "0s"]).is_err());
        assert!(parse_sync_cmd(&["pipe", "vector", "extra"]).is_err());
        assert!(parse_sync_cmd(&["status", "extra"]).is_err());
    }

    /// Every spelling the regrouping retired is gone (da-aam46), as are
    /// the ones da-6177s retired before it.
    #[test]
    fn the_old_spellings_are_gone() {
        for gone in [
            "now",
            "vector",
            "ops",
            "apply",
            "verify",
            "connect",
            "invite",
            "members",
            "rename",
            "ack",
            "revoke",
            "add-admin",
            "remove-admin",
            "rekey",
            "init",
            "add-endpoint",
            "add-client",
            "peers",
            "forget",
        ] {
            assert!(parse_sync_cmd(&[gone]).is_err(), "{gone}");
        }
    }
}
