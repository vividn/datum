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
            show_revoked: opts(&mut rest, &[], &["show-revoked"], 0..=0)?.on("show-revoked"),
        },
        "pause" => SyncCommand::Pause,
        "resume" => SyncCommand::Resume,
        "diff" => SyncCommand::Diff {
            who: opts(&mut rest, &[], &[], 1..=1)?.positional.remove(0),
            show: Show::Default,
        },
        "refold" => SyncCommand::Refold {
            dry_run: opts(&mut rest, &[], &["dry-run"], 0..=0)?.on("dry-run"),
            show: Show::Default,
        },
        "help" => return Err(ParseError::HelpRequested(Some("sync".into()))),
        "chain" => parse_chain(&mut rest)?,
        "admin" => parse_admin(&mut rest)?,
        "daemon" => SyncCommand::Daemon(match rest.next().copied() {
            Some("install") => SyncDaemonAct::Install,
            Some("uninstall") => SyncDaemonAct::Uninstall,
            Some("status") => SyncDaemonAct::Status,
            // Hidden: what the service runs.
            Some("run") => SyncDaemonAct::Run,
            Some(other) => return Err(unknown("sync daemon", other, DAEMON)),
            None => return Err(missing("sync daemon", DAEMON)),
        }),
        // Hidden (da-otz0d): datum.day's store, a session's command.
        "--as" => {
            let id = rest
                .next()
                .ok_or_else(|| missing("sync --as", "ENDPOINT"))?;
            let cmd = parse_sync_cmd(rest.as_slice())?;
            return Ok(SyncCommand::As(id.to_string(), Box::new(cmd)));
        }
        // Hidden: the pipe transport, the daemon, and datum.day's store.
        "pipe" => parse_pipe(&mut rest)?,
        "run" => SyncCommand::Run,
        // Hidden (da-rcb5m): datum.day's network worker, not a person.
        "endpoint-key" => SyncCommand::EndpointKey,
        "holds" => SyncCommand::Holds,
        "record-session" => {
            let mut o = opts(
                &mut rest,
                &["vector", "verify", "error", "met"],
                &["offline"],
                1..=1,
            )?;
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
                offline: o.on("offline"),
                met: o.get("met"),
            }
        }
        // Hidden (da-6kpj5): datum.day's transport, as for `record-session`.
        "handshake" => {
            let mut o = opts(
                &mut rest,
                &[
                    "label", "addr", "invite", "chain", "holds", "datum", "schema",
                ],
                &["dialed", "records"],
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
                holds: match o.get("holds") {
                    Some(n) => n.parse().map_err(|_| {
                        ParseError::InvalidArg(format!("--holds is a count, not {n}"))
                    })?,
                    None => 0,
                },
                version: match o.get("schema") {
                    Some(n) => Some((
                        o.get("datum").unwrap_or_default(),
                        n.parse().map_err(|_| {
                            ParseError::InvalidArg(format!("--schema is a version, not {n}"))
                        })?,
                    )),
                    None => None,
                },
                records: o.on("records"),
                dialed: o.on("dialed"),
                endpoint: o.positional.remove(0),
            }
        }
        "new-invite" => {
            let o = opts(
                &mut rest,
                &["expires", "admin-key", "fields", "kit"],
                &["reader"],
                0..=0,
            )?;
            let filter = o.filter()?;
            SyncCommand::NewInvite {
                reader: o.on("reader") || filter.is_some(),
                filter,
                expires_secs: o.expires()?,
                admin_key: o.get("admin-key"),
            }
        }
        "approve" => SyncCommand::Approve {
            link: opts(&mut rest, &[], &[], 1..=1)?.positional.remove(0),
        },
        "qr" => SyncCommand::Qr {
            text: opts(&mut rest, &[], &[], 1..=1)?.positional.remove(0),
        },
        other => {
            return Err(unknown(
                "sync",
                other,
                "status|diff|refold|chain|admin|daemon|pause|resume|help",
            ))
        }
    };
    match rest.next() {
        Some(t) => Err(unknown_flag(split_flag(t))),
        None => Ok(cmd),
    }
}

const DAEMON: &str = "install|uninstall|status";

type Rest<'a> = std::slice::Iter<'a, &'a str>;

/// `datum sync chain …`: the chain's membership.
fn parse_chain(rest: &mut Rest) -> Result<SyncCommand> {
    const SET: &str = "invite|connect|leave|list|rename|revoke|revert|purge|role|ack|rekey";
    Ok(match rest.next().copied() {
        Some("invite") => {
            let values = ["expires", "admin-key", "fields", "kit", "label"];
            let o = opts(rest, &values, &["qr", "reader"], 0..=0)?;
            let filter = o.filter()?;
            SyncCommand::Invite {
                reader: o.on("reader") || filter.is_some(),
                filter,
                qr: o.on("qr"),
                expires_secs: o.expires()?,
                label: o.get("label"),
                admin_key: o.get("admin-key"),
            }
        }
        Some("connect") => {
            let mut o = opts(rest, &["label"], &[], 0..=1)?;
            SyncCommand::Connect {
                ticket: o.positional.pop(),
                label: o.get("label"),
            }
        }
        Some("leave") => {
            let o = opts(rest, &[], &["alone", "dry-run", "force"], 0..=0)?;
            SyncCommand::Leave {
                alone: o.on("alone"),
                dry_run: o.on("dry-run"),
                force: o.on("force"),
            }
        }
        Some("list") => SyncCommand::Members {
            show: Show::Default,
            show_revoked: opts(rest, &[], &["show-revoked"], 0..=0)?.on("show-revoked"),
        },
        Some("rename") => {
            let mut o = opts(rest, &["admin-key"], &["unset", "check"], 0..=2)?;
            let unset = o.on("unset");
            let given = match unset {
                true => 0..=1,
                false => 1..=2,
            };
            if !given.contains(&o.positional.len()) {
                return Err(ParseError::InvalidArg(
                    "rename takes [<WHO>] <LABEL>, or [<WHO>] --unset".into(),
                ));
            }
            SyncCommand::Rename {
                label: (!unset).then(|| o.positional.pop()).flatten(),
                who: o.positional.pop(),
                admin_key: o.get("admin-key"),
                check: o.on("check"),
            }
        }
        Some("revoke") => act(rest, SyncAdminAct::Revoke, &[], &[])?,
        // Renamed (da-c9fiy): one device's purge is `revert <WHO>`.
        Some("purge") => {
            let o = opts(rest, &["admin-key"], &["data", "all", "yes"], 0..=1)?;
            if !o.positional.is_empty() {
                return Err(ParseError::InvalidArg(
                    "`sync chain purge <WHO>` is now `sync chain revert <WHO>`".into(),
                ));
            }
            if o.on("data") == o.on("all") {
                return Err(ParseError::InvalidArg(
                    "`sync chain purge` takes --data (every copy drops its data) or --all \
                     (and the chain dissolves)"
                        .into(),
                ));
            }
            SyncCommand::Purge {
                all: o.on("all"),
                yes: o.on("yes"),
                admin_key: o.get("admin-key"),
            }
        }
        Some("revert") => {
            let mut o = opts(rest, &["admin-key"], &["yes"], 1..=1)?;
            SyncCommand::Admin {
                act: SyncAdminAct::Revert {
                    yes: o.on("yes"),
                    show: Show::Default,
                },
                who: o.positional.remove(0),
                label: None,
                admin_key: o.get("admin-key"),
                allow_unlock: false,
            }
        }
        Some("role") => {
            let mut o = opts(rest, &["admin-key", "fields", "kit"], &[], 2..=2)?;
            let filter = o.filter()?;
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
            if filter.is_some() && !reader {
                return Err(ParseError::InvalidArg(
                    "only a reader is filtered: `role <WHO> reader --fields …`".into(),
                ));
            }
            SyncCommand::Admin {
                act: SyncAdminAct::Role { reader, filter },
                who: o.positional.remove(0),
                label: None,
                admin_key: o.get("admin-key"),
                allow_unlock: false,
            }
        }
        Some("ack") => {
            let mut o = opts(rest, &[], &["all"], 0..=1)?;
            let (who, all) = (o.positional.pop(), o.on("all"));
            if who.is_some() && all {
                return Err(ParseError::InvalidArg(
                    "ack <WHO> or --all, not both".into(),
                ));
            }
            SyncCommand::Ack { who, all }
        }
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
    const SET: &str = "vector|ops|apply|records|verify|digest|held";
    Ok(match rest.next().copied() {
        Some("vector") => SyncCommand::Vector,
        Some("records") => SyncCommand::Records,
        // `--json` is the swept `--show json`, set by `apply_show`.
        Some("apply") => SyncCommand::Apply {
            show: Show::Default,
        },
        Some("verify") => {
            let o = opts(rest, &["peer"], &["verbose"], 0..=0)?;
            SyncCommand::Verify {
                verbose: o.on("verbose"),
                peer: o.get("peer"),
            }
        }
        Some("digest") => SyncCommand::Digest {
            peer: opts(rest, &["peer"], &[], 0..=0)?.get("peer"),
        },
        Some("held") => SyncCommand::Held {
            peer: opts(rest, &["peer"], &[], 0..=0)?.get("peer"),
        },
        Some("ops") => {
            let o = opts(rest, &["since", "peer", "limit"], &["no-overlap"], 0..=0)?;
            SyncCommand::Ops {
                since: o.get("since"),
                overlap: !o.on("no-overlap"),
                peer: o.get("peer"),
                limit: match o.get("limit") {
                    Some(n) => Some(n.parse().map_err(|_| {
                        ParseError::InvalidArg(format!("--limit is a count, not {n}"))
                    })?),
                    None => None,
                },
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

    /// A reader's filter (da-u81u1): `--fields a,b` or `--kit <KIT>`.
    fn filter(&self) -> Result<Option<SyncFilter>> {
        match (self.get("fields"), self.get("kit")) {
            (Some(_), Some(_)) => Err(ParseError::InvalidArg("--fields or --kit, not both".into())),
            (Some(f), None) => {
                let fields: Vec<String> = f
                    .split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(Into::into)
                    .collect();
                match fields.is_empty() {
                    true => Err(ParseError::InvalidArg("--fields names no field".into())),
                    false => Ok(Some(SyncFilter::Fields(fields))),
                }
            }
            (None, Some(k)) => Ok(Some(SyncFilter::Kit(k))),
            (None, None) => Ok(None),
        }
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
                show: Show::Default,
                show_revoked: false
            }
        );
        assert_eq!(
            p(&["status", "--show-revoked"]),
            SyncCommand::Status {
                show: Show::Default,
                show_revoked: true
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
            p(&["--as", "ab", "pipe", "vector"]),
            SyncCommand::As("ab".into(), Box::new(SyncCommand::Vector))
        );
        assert!(parse_sync_cmd(&["--as"]).is_err());
        assert_eq!(p(&["pipe", "records"]), SyncCommand::Records);
        assert_eq!(
            p(&["pipe", "apply"]),
            SyncCommand::Apply {
                show: Show::Default
            }
        );
        assert_eq!(
            p(&["pipe", "verify", "--verbose"]),
            SyncCommand::Verify {
                verbose: true,
                peer: None
            }
        );
        assert_eq!(
            p(&["pipe", "verify", "--peer", "ab"]),
            SyncCommand::Verify {
                verbose: false,
                peer: Some("ab".into())
            }
        );
        assert_eq!(
            p(&["pipe", "ops", "--since={\"a\":1}"]),
            SyncCommand::Ops {
                since: Some("{\"a\":1}".into()),
                overlap: true,
                peer: None,
                limit: None
            }
        );
        assert_eq!(
            p(&[
                "pipe",
                "ops",
                "--since",
                "{}",
                "--no-overlap",
                "--peer=ab",
                "--limit",
                "9"
            ]),
            SyncCommand::Ops {
                since: Some("{}".into()),
                overlap: false,
                peer: Some("ab".into()),
                limit: Some(9)
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
                holds: 0,
                version: None,
                records: false,
                dialed: true
            }
        );
        assert_eq!(
            p(&[
                "handshake",
                "ab",
                "--label=l",
                "--addr={}",
                "--invite=s",
                "--holds",
                "42",
                "--datum",
                "3.0.9-beta (abc1234)",
                "--schema",
                "57",
                "--records"
            ]),
            SyncCommand::Handshake {
                endpoint: "ab".into(),
                label: "l".into(),
                addr: "{}".into(),
                invite: Some("s".into()),
                chain: None,
                holds: 42,
                version: Some(("3.0.9-beta (abc1234)".into(), 57)),
                records: true,
                dialed: false
            }
        );
        assert!(parse_sync_cmd(&["handshake", "ab", "--label", "l"]).is_err());
        assert!(
            parse_sync_cmd(&["handshake", "ab", "--label=l", "--addr={}", "--holds=x"]).is_err()
        );
        assert_eq!(
            p(&["new-invite", "--expires", "1m"]),
            SyncCommand::NewInvite {
                reader: false,
                filter: None,
                expires_secs: 60,
                admin_key: None
            }
        );
        assert_eq!(p(&["run"]), SyncCommand::Run);
        for (word, act) in [
            ("install", SyncDaemonAct::Install),
            ("uninstall", SyncDaemonAct::Uninstall),
            ("status", SyncDaemonAct::Status),
            ("run", SyncDaemonAct::Run),
        ] {
            assert_eq!(p(&["daemon", word]), SyncCommand::Daemon(act));
        }
        assert!(parse_sync_cmd(&["daemon", "install", "extra"]).is_err());
        assert_eq!(
            p(&[
                "chain",
                "invite",
                "--admin-key=k",
                "--qr",
                "--reader",
                "--label",
                "laptop"
            ]),
            SyncCommand::Invite {
                reader: true,
                filter: None,
                qr: true,
                expires_secs: 600,
                label: Some("laptop".into()),
                admin_key: Some("k".into())
            }
        );
        assert_eq!(
            p(&["chain", "connect"]),
            SyncCommand::Connect {
                ticket: None,
                label: None
            }
        );
        assert_eq!(
            p(&["chain", "connect", "--label", "phone", "t"]),
            SyncCommand::Connect {
                ticket: Some("t".into()),
                label: Some("phone".into())
            }
        );
        assert!(parse_sync_cmd(&["chain", "connect", "t", "u"]).is_err());
        assert_eq!(
            p(&["chain", "list"]),
            SyncCommand::Members {
                show: Show::Default,
                show_revoked: false
            }
        );
        assert_eq!(
            p(&["chain", "list", "--show-revoked"]),
            SyncCommand::Members {
                show: Show::Default,
                show_revoked: true
            }
        );
        assert_eq!(
            p(&["chain", "rename", "abcd", "phone", "--admin-key", "k"]),
            SyncCommand::Rename {
                who: Some("abcd".into()),
                label: Some("phone".into()),
                admin_key: Some("k".into()),
                check: false
            }
        );
        assert_eq!(
            p(&["refold", "--dry-run"]),
            SyncCommand::Refold {
                dry_run: true,
                show: Show::Default
            }
        );
        // A flag it does not take is refused, never a refold that writes.
        assert!(parse_sync_cmd(&["refold", "--dryrun"]).is_err());
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
            p(&["chain", "revert", "b", "--yes", "--admin-key", "k"]),
            SyncCommand::Admin {
                act: SyncAdminAct::Revert {
                    yes: true,
                    show: Show::Default
                },
                who: "b".into(),
                label: None,
                admin_key: Some("k".into()),
                allow_unlock: false
            }
        );
        assert!(parse_sync_cmd(&["chain", "revert"]).is_err());
        let old = parse_sync_cmd(&["chain", "purge", "b", "--yes"]).unwrap_err();
        assert!(
            old.to_string().contains("`sync chain revert <WHO>`"),
            "{old}"
        );
        assert_eq!(
            p(&["chain", "purge", "--all", "--yes", "--admin-key", "k"]),
            SyncCommand::Purge {
                all: true,
                yes: true,
                admin_key: Some("k".into()),
            }
        );
        for wrong in [
            &["chain", "purge"][..],
            &["chain", "purge", "--data", "--all"],
        ] {
            let e = parse_sync_cmd(wrong).unwrap_err().to_string();
            assert!(e.contains("takes --data") && e.contains("or --all"), "{e}");
        }
        // Gone (da-q4rau): a fresh invite brings a purged device back.
        assert!(parse_sync_cmd(&["chain", "unpurge", "b"]).is_err());
        assert_eq!(
            p(&["chain", "role", "b", "reader", "--admin-key", "k"]),
            SyncCommand::Admin {
                act: SyncAdminAct::Role {
                    reader: true,
                    filter: None
                },
                who: "b".into(),
                label: None,
                admin_key: Some("k".into()),
                allow_unlock: false
            }
        );
        assert!(parse_sync_cmd(&["chain", "role", "b", "admin"]).is_err());
        // A filter (da-u81u1): a reader's alone, fields or a kit, and a
        // filtered invite is a reader's.
        assert_eq!(
            p(&["chain", "role", "b", "reader", "--fields", "coffee, tea"]),
            SyncCommand::Admin {
                act: SyncAdminAct::Role {
                    reader: true,
                    filter: Some(SyncFilter::Fields(vec!["coffee".into(), "tea".into()]))
                },
                who: "b".into(),
                label: None,
                admin_key: None,
                allow_unlock: false
            }
        );
        assert!(parse_sync_cmd(&["chain", "role", "b", "member", "--kit", "k"]).is_err());
        assert!(parse_sync_cmd(&["chain", "invite", "--fields=a", "--kit=k"]).is_err());
        assert!(parse_sync_cmd(&["chain", "invite", "--fields=,"]).is_err());
        assert_eq!(
            p(&["chain", "invite", "--kit", "health"]),
            SyncCommand::Invite {
                reader: true,
                filter: Some(SyncFilter::Kit("health".into())),
                qr: false,
                expires_secs: 600,
                label: None,
                admin_key: None
            }
        );
        assert!(parse_sync_cmd(&["chain", "role", "b"]).is_err());
        assert_eq!(
            p(&["chain", "ack", "abc"]),
            SyncCommand::Ack {
                who: Some("abc".into()),
                all: false
            }
        );
        assert_eq!(
            p(&["chain", "ack"]),
            SyncCommand::Ack {
                who: None,
                all: false
            }
        );
        assert_eq!(
            p(&["chain", "ack", "--all"]),
            SyncCommand::Ack {
                who: None,
                all: true
            }
        );
        assert!(parse_sync_cmd(&["chain", "ack", "abc", "--all"]).is_err());
        assert_eq!(p(&["chain", "rekey"]), SyncCommand::Rekey);
        assert_eq!(
            p(&["chain", "leave", "--dry-run"]),
            SyncCommand::Leave {
                alone: false,
                dry_run: true,
                force: false
            }
        );
        assert_eq!(
            p(&["chain", "leave", "--alone"]),
            SyncCommand::Leave {
                alone: true,
                dry_run: false,
                force: false
            }
        );
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
        for group in ["chain", "admin", "daemon", "pipe"] {
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
