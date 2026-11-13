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

const DAY: u64 = 24 * 3600;
/// How long an invite holds unless `--expires` says (da-4qpwf).
const WEEK: u64 = 7 * DAY;
/// The longest `--expires`: a hundred years, well inside what a clock can
/// be added to; `never` is an agent's code with no end (vividn, #2065).
const MOST: u64 = 100 * 365 * DAY;

pub(crate) fn parse_sync_cmd(tail: &[&str]) -> Result<SyncCommand> {
    let Some((sub, rest)) = tail.split_first() else {
        return Ok(SyncCommand::Now { verbose: false });
    };
    if matches!(*sub, "-v" | "--verbose") && rest.is_empty() {
        return Ok(SyncCommand::Now { verbose: true });
    }
    // The sync daemon on or off, as `backup --auto` and `upgrade --auto`
    // turn theirs (da-tsna7).
    match (*sub, rest) {
        ("--auto", []) => return Ok(SyncCommand::Daemon(SyncDaemonAct::Install)),
        ("--no-auto", []) => return Ok(SyncCommand::Daemon(SyncDaemonAct::Uninstall)),
        ("--auto" | "--no-auto", [extra, ..]) => {
            return Err(ParseError::InvalidArg(format!(
                "sync {sub} takes nothing more: {extra}"
            )))
        }
        _ => {}
    }
    let mut rest = rest.iter();
    let cmd = match *sub {
        // `--json` is the swept `--show json`, set by `apply_show`.
        "status" => {
            let flags = opts(&mut rest, &[], &["show-revoked", "no-grants"], 0..=0)?;
            SyncCommand::Status {
                show: Show::Default,
                show_revoked: flags.on("show-revoked"),
                no_grants: flags.on("no-grants"),
            }
        }
        // The chain's pin (da-w8zp5): no member upgrades past it.
        "pin" | "unpin" => {
            let n = usize::from(*sub == "pin");
            let mut o = opts(&mut rest, &["admin-key"], &[], n..=n)?;
            SyncCommand::Pin {
                version: match o.positional.pop() {
                    Some(v) => Some(super::info_db::release_version("sync pin takes", &v)?),
                    None => None,
                },
                admin_key: o.get("admin-key"),
            }
        }
        "pause" => SyncCommand::Pause,
        "grid" => {
            opts(&mut rest, &[], &[], 0..=0)?;
            SyncCommand::Grid {
                show: Show::Default,
            }
        }
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
        "pending-join" => {
            let o = opts(
                &mut rest,
                &["hold", "about", "label", "user", "refused"],
                &["clear", "gave-way"],
                0..=0,
            )?;
            SyncCommand::PendingJoin {
                hold: o.get("hold"),
                about: o.get("about"),
                label: o.get("label"),
                username: o.get("user"),
                refused: o.get("refused"),
                gave_way: o.on("gave-way"),
                clear: o.on("clear"),
            }
        }
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
                    "label", "addr", "invite", "chain", "holds", "datum", "schema", "floor",
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
                floor: match o.get("floor") {
                    Some(n) => Some(n.parse().map_err(|_| {
                        ParseError::InvalidArg(format!("--floor is a version, not {n}"))
                    })?),
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
                &["expires", "admin-key", "fields", "kit", "window"],
                &["reader", "writer", "specs"],
                0..=0,
            )?;
            let (role, filter) = o.role()?;
            SyncCommand::NewInvite {
                role,
                filter,
                expires_secs: o.expires()?,
                admin_key: o.get("admin-key"),
            }
        }
        "approve" => SyncCommand::Approve {
            link: opts(&mut rest, &[], &[], 1..=1)?.positional.remove(0),
        },
        "account" => {
            let o = opts(
                &mut rest,
                &["invite", "relay", "with", "read", "seal", "approve"],
                &["record"],
                0..=0,
            )?;
            SyncCommand::Account {
                invite: o.get("invite"),
                record: o.on("record"),
                relay: o.get("relay"),
                with: o.get("with"),
                read: o.get("read"),
                seal: o.get("seal"),
                approve: o.get("approve"),
            }
        }
        "restore" => SyncCommand::Restore {
            code: opts(&mut rest, &[], &[], 0..=1)?.positional.pop(),
        },
        "login" => SyncCommand::Login,
        "logout" => SyncCommand::Logout,
        "qr" => SyncCommand::Qr {
            text: opts(&mut rest, &[], &[], 1..=1)?.positional.remove(0),
        },
        other => return Err(unknown(
            "sync",
            other,
            "status|grid|diff|refold|chain|admin|daemon|pause|resume|pin|unpin|restore|login|logout|help",
        )),
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
    const SET: &str =
        "invite|join|agent|leave|list|rename|expel|purge|role|ack|rekey|reenter|upgrade";
    Ok(match rest.next().copied() {
        Some("invite") => {
            let values = [
                "expires",
                "admin-key",
                "fields",
                "kit",
                "window",
                "label",
                "user",
                "as",
                "like",
                "devices",
                "ticket",
                "invite",
            ];
            let switches = ["qr", "reader", "writer", "defer", "specs"];
            let mut o = opts(rest, &values, &switches, 0..=1)?;
            let (device, like) = (o.positional.pop(), o.get("like"));
            if device.is_some() && like.is_some() {
                return Err(ParseError::InvalidArg(
                    "`sync chain invite` takes a device or --like <DB>, not both".into(),
                ));
            }
            if like.is_none() {
                refuse(&o, &["devices", "defer"], "goes with --like <DB>")?;
            }
            if device.is_none() {
                refuse(&o, &["as", "ticket", "invite"], "goes with a device")?;
            }
            if device.is_some() || like.is_some() {
                let why = "goes with a bare `sync chain invite`, which prints a link";
                let only = ["qr", "label", "user", "fields", "kit", "window", "specs"];
                refuse(&o, &only, why)?;
            }
            // The role and the expiry: a device may be off for days.
            if let Some(like) = like {
                let why = "does not go with --like <DB>: each device takes the role it has there";
                refuse(&o, &["reader", "writer"], why)?;
                let devices = o.get("devices").map(|d| {
                    let d = d.split(',').map(str::trim).filter(|d| !d.is_empty());
                    d.map(Into::into).collect::<Vec<String>>()
                });
                if devices.as_ref().is_some_and(Vec::is_empty) {
                    return Err(ParseError::InvalidArg("--devices names no device".into()));
                }
                return Ok(SyncCommand::GrantLike {
                    like,
                    devices,
                    expires_secs: o.expires_or(WEEK)?,
                    admin_key: o.get("admin-key"),
                    defer: o.on("defer"),
                });
            }
            if let Some(device) = device {
                // No filter to drop: `--fields` and `--kit` are refused
                // beside a device above.
                return Ok(SyncCommand::Grant {
                    device,
                    role: o.role()?.0,
                    expires_secs: o.expires_or(WEEK)?,
                    name: o.get("as"),
                    admin_key: o.get("admin-key"),
                    ticket: match (o.get("ticket"), o.get("invite")) {
                        (Some(t), Some(i)) => Some((t, i)),
                        (None, None) => None,
                        _ => {
                            return Err(ParseError::InvalidArg(
                                "--ticket and --invite go together".into(),
                            ))
                        }
                    },
                });
            }
            let (role, filter) = o.role()?;
            SyncCommand::Invite {
                role,
                filter,
                qr: o.on("qr"),
                expires_secs: o.expires()?,
                label: o.get("label"),
                user: o.get("user"),
                admin_key: o.get("admin-key"),
            }
        }
        Some("join") => {
            let mut o = opts(rest, &["label", "user"], &[], 0..=1)?;
            SyncCommand::Join {
                ticket: o.positional.pop(),
                label: o.get("label"),
                user: o.get("user"),
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
            let mut o = opts(rest, &["admin-key", "user"], &["unset", "check"], 0..=2)?;
            let (unset, user) = (o.on("unset"), o.get("user"));
            // The username is this install's: no WHO beside it.
            let (n, who) = (o.positional.len(), usize::from(user.is_none()));
            let fits = match unset {
                true => n <= who,
                false => (who..=1 + who).contains(&n),
            };
            if !fits {
                return Err(ParseError::InvalidArg(
                    "rename takes [<WHO>] <LABEL>, [<WHO>] --unset, or [<LABEL> | --unset] --user <U>"
                        .into(),
                ));
            }
            let label = match unset {
                true => Some(None),
                false if n == 0 => None,
                false => Some(o.positional.pop()),
            };
            SyncCommand::Rename {
                label,
                user,
                who: o.positional.pop(),
                admin_key: o.get("admin-key"),
                check: o.on("check"),
            }
        }
        Some("expel") => {
            let mut o = opts(rest, &["admin-key"], &["unjoin", "wipe", "yes"], 1..=1)?;
            SyncCommand::Admin {
                act: SyncAdminAct::Expel {
                    unjoin: o.on("unjoin"),
                    wipe: o.on("wipe"),
                    yes: o.on("yes"),
                    show: Show::Default,
                },
                who: o.positional.remove(0),
                label: None,
                admin_key: o.get("admin-key"),
                allow_unlock: false,
            }
        }
        // One device's purge is `expel <WHO> --unjoin` (da-c9fiy, da-zfss8).
        Some("purge") => {
            let o = opts(rest, &["admin-key"], &["data", "all", "yes"], 0..=1)?;
            if !o.positional.is_empty() {
                return Err(ParseError::InvalidArg(
                    "`sync chain purge <WHO>` is now `sync chain expel <WHO> --unjoin`".into(),
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
        Some("role") => {
            let mut o = opts(
                rest,
                &["admin-key", "fields", "kit", "window"],
                &["specs"],
                2..=2,
            )?;
            let word = o.positional.pop().unwrap_or_default();
            let Some(role) = SyncRole::parse(&word) else {
                return Err(ParseError::InvalidArg(format!(
                    "a role is member, reader or writer, not {word}"
                )));
            };
            let filter = o.filter(role)?;
            if filter.is_some() && role.relays() {
                return Err(ParseError::InvalidArg(
                    "only a reader or a writer is filtered: `role <WHO> reader --fields …`".into(),
                ));
            }
            SyncCommand::Admin {
                act: SyncAdminAct::Role { role, filter },
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
        Some("agent") => parse_agent(rest)?,
        Some("rekey") => SyncCommand::Rekey,
        Some("reenter") => SyncCommand::Reenter,
        Some("upgrade") => SyncCommand::Upgrade,
        Some(other) => return Err(unknown("sync chain", other, SET)),
        None => return Err(missing("sync chain", SET)),
    })
}

/// `datum sync chain agent` (da-bdyor): its level is a role and a filter.
/// `full` reads everything (`--write`: a member); `light` reads a window,
/// one day unless `--window` says, and every spec and alias (`--write`: a
/// writer of it); `write` is a writer sent nothing but the chain's records,
/// of `--fields` alone where given.
fn parse_agent(rest: &mut Rest) -> Result<SyncCommand> {
    let values = ["level", "label", "window", "fields", "expires", "admin-key"];
    let o = opts(rest, &values, &["write"], 0..=0)?;
    let level = o.get("level").unwrap_or_default();
    let refuse_at =
        |flags: &[&str]| refuse(&o, flags, &format!("does not go with --level {level}"));
    let write = o.on("write");
    let named = |f: Option<String>| {
        let f = f.unwrap_or_default();
        let f = f.split(',').map(str::trim).filter(|f| !f.is_empty());
        SyncFields::Named(f.map(Into::into).collect())
    };
    let (role, filter) = match level.as_str() {
        "full" => {
            refuse_at(&["window", "fields"])?;
            (
                if write {
                    SyncRole::Member
                } else {
                    SyncRole::Reader
                },
                None,
            )
        }
        "light" => {
            refuse_at(&["fields"])?;
            let window = o.filter(SyncRole::Reader)?.and_then(|f| f.window_secs);
            let filter = SyncFilter {
                fields: None,
                window_secs: Some(window.unwrap_or(24 * 3600)),
                specs: true,
            };
            (
                if write {
                    SyncRole::Writer
                } else {
                    SyncRole::Reader
                },
                Some(filter),
            )
        }
        "write" => {
            refuse_at(&["window", "write"])?;
            let filter = SyncFilter {
                fields: Some(named(o.get("fields"))),
                ..SyncFilter::default()
            };
            (SyncRole::Writer, Some(filter))
        }
        _ => {
            return Err(ParseError::InvalidArg(
                "`sync chain agent` takes --level full, light or write".into(),
            ))
        }
    };
    Ok(SyncCommand::Agent {
        label: o.get("label").unwrap_or_else(|| "agent".into()),
        level,
        role,
        filter,
        // A code is its device's secret: it lapses in 30 days unless
        // `--expires` says otherwise, `never` included (da-4cw3v).
        expires_secs: match o.get("expires").as_deref() {
            Some("never") => None,
            _ => Some(o.expires_or(30 * DAY)?),
        },
        admin_key: o.get("admin-key"),
        show: Show::Default,
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
    const SET: &str = "vector|ops|apply|records|hold|standing|known|verify|digest|held";
    Ok(match rest.next().copied() {
        Some("vector") => SyncCommand::Vector,
        Some("records") => SyncCommand::Records,
        Some("hold") => SyncCommand::Hold,
        Some("standing") => {
            let o = opts(rest, &["chain"], &[], 0..=0)?;
            SyncCommand::Standing {
                chain: o.get("chain").unwrap_or_default(),
            }
        }
        Some("known") => SyncCommand::Known,
        // `--json` is the swept `--show json`, set by `apply_show`.
        Some("apply") => SyncCommand::Apply {
            show: Show::Default,
        },
        Some("verify") => {
            let o = opts(rest, &["peer"], &["ordered", "verbose"], 0..=0)?;
            SyncCommand::Verify {
                ordered: o.on("ordered"),
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

/// Refused when any of `flags` is given: `--<flag> <why>`.
fn refuse(o: &Opts, flags: &[&str], why: &str) -> Result<()> {
    match flags.iter().find(|f| o.get(f).is_some() || o.on(f)) {
        Some(f) => Err(ParseError::InvalidArg(format!("--{f} {why}"))),
        None => Ok(()),
    }
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

    /// A reader's or writer's filter (da-u81u1): `--fields a,b` or `--kit
    /// <KIT>`, `--window <DUR>` and `--specs` (da-nflkk); a writer's, or
    /// one with `--specs`, may name no field, `--fields ''` (da-o5o6m).
    fn filter(&self, role: SyncRole) -> Result<Option<SyncFilter>> {
        let specs = self.on("specs");
        let fields = match (self.get("fields"), self.get("kit")) {
            (Some(_), Some(_)) => {
                return Err(ParseError::InvalidArg("--fields or --kit, not both".into()))
            }
            (Some(f), None) => {
                let fields: Vec<String> = f
                    .split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(Into::into)
                    .collect();
                if fields.is_empty() && role != SyncRole::Writer && !specs {
                    return Err(ParseError::InvalidArg(
                        "--fields names no field: only a writer's may (`--writer --fields ''`), \
                         or one sent every spec (`--specs --fields ''`)"
                            .into(),
                    ));
                }
                Some(SyncFields::Named(fields))
            }
            (None, Some(k)) => Some(SyncFields::Kit(k)),
            (None, None) => None,
        };
        let window_secs = match self.get("window") {
            Some(w) => Some(
                u64::try_from(crate::time::parse_duration(&w)?.num_seconds())
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or_else(|| {
                        ParseError::InvalidArg(format!("--window is a length of time, not {w}"))
                    })?,
            ),
            None => None,
        };
        let filter = SyncFilter {
            fields,
            window_secs,
            specs,
        };
        Ok((filter != SyncFilter::default()).then_some(filter))
    }

    /// The role `--reader` or `--writer` asks for, and the filter beside it:
    /// a filter alone is a reader's.
    fn role(&self) -> Result<(SyncRole, Option<SyncFilter>)> {
        let role = match (self.on("reader"), self.on("writer")) {
            (true, true) => {
                return Err(ParseError::InvalidArg(
                    "--reader or --writer, not both".into(),
                ))
            }
            (_, true) => SyncRole::Writer,
            (true, _) => SyncRole::Reader,
            _ if ["fields", "kit", "window"]
                .iter()
                .any(|v| self.get(v).is_some())
                || self.on("specs") =>
            {
                SyncRole::Reader
            }
            _ => SyncRole::Member,
        };
        Ok((role, self.filter(role)?))
    }

    /// `--expires`, 7 days by default: a link is sent, and its redeemer
    /// joins whenever it opens it (da-4qpwf).
    fn expires(&self) -> Result<u64> {
        self.expires_or(WEEK)
    }

    fn expires_or(&self, default: u64) -> Result<u64> {
        let Some(value) = self.values.get("expires") else {
            return Ok(default);
        };
        // Months of 30 days and years of 365, which no other duration
        // takes: `m` is a minute (da-4cw3v).
        let long = [("mo", 30 * DAY), ("y", 365 * DAY)]
            .iter()
            .find_map(|(unit, secs)| {
                // Digits and a point alone: `inf`, `nan` and `1e3` are no
                // count of months that f64 should read.
                let n = value.strip_suffix(unit)?;
                let plain = n.bytes().all(|b| b.is_ascii_digit() || b == b'.');
                let n: f64 = n.parse().ok().filter(|_| plain)?;
                Some((n * *secs as f64) as u64)
            });
        let secs = long.or_else(|| {
            let secs = crate::time::parse_duration(value).ok()?.num_seconds();
            u64::try_from(secs).ok()
        });
        match secs.filter(|s| *s > 0) {
            Some(secs) if secs > MOST => Err(ParseError::InvalidArg(format!(
                "--expires is at most 100y: {value} is longer (an agent's code takes `never`)"
            ))),
            Some(secs) => Ok(secs),
            None => Err(ParseError::InvalidArg(format!(
                "--expires must be a positive duration: {value}"
            ))),
        }
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

    /// An agent's level is a role and a filter (da-bdyor).
    #[test]
    fn an_agents_level_is_a_role_and_a_filter() {
        let agent = |argv: &[&str]| match p(&[&["chain", "agent"], argv].concat()) {
            SyncCommand::Agent {
                role,
                filter,
                expires_secs,
                label,
                ..
            } => (role, filter, expires_secs, label),
            c => panic!("{c:?}"),
        };
        let full = agent(&["--level", "full"]);
        assert_eq!(
            full,
            (SyncRole::Reader, None, Some(30 * DAY), "agent".into())
        );
        // A code is its device's secret, so it lapses unless asked not to
        // (da-4cw3v): months of 30 days, years of 365.
        for (expires, secs) in [
            ("60d", Some(60 * DAY)),
            ("90d", Some(90 * DAY)),
            ("6mo", Some(180 * DAY)),
            ("1y", Some(365 * DAY)),
            ("1.5y", Some(365 * DAY * 3 / 2)),
            ("2h", Some(7200)),
            ("1day", Some(DAY)),
            ("never", None),
        ] {
            let argv = ["--level", "full", "--expires", expires];
            assert_eq!(agent(&argv).2, secs, "{expires}");
        }
        assert_eq!(agent(&["--level", "full", "--write"]).0, SyncRole::Member);
        let light = SyncFilter {
            fields: None,
            window_secs: Some(24 * 3600),
            specs: true,
        };
        assert_eq!(agent(&["--level", "light"]).1, Some(light.clone()));
        let written = agent(&["--level", "light", "--write", "--window", "2h"]);
        let two_hours = SyncFilter {
            window_secs: Some(7200),
            ..light
        };
        assert_eq!((written.0, written.1), (SyncRole::Writer, Some(two_hours)));
        let argv = [
            "--level",
            "write",
            "--fields",
            "tea,coffee",
            "--label",
            "bot",
            "--expires",
            "30d",
        ];
        assert_eq!(
            agent(&argv),
            (
                SyncRole::Writer,
                Some(named(&["tea", "coffee"])),
                Some(30 * 24 * 3600),
                "bot".into()
            )
        );
        assert_eq!(agent(&["--level", "write"]).1, Some(named(&[])));
        for bad in [
            &[][..],
            &["--level", "huge"],
            &["--level", "full", "--window", "1d"],
            &["--level", "write", "--write"],
            &["--level", "light", "--fields", "a"],
            &["--level", "full", "--expires", "0s"],
            &["--level", "full", "--expires", "mo"],
            // No number f64 reads but a plain one, and nothing past 100
            // years, which `now` could not be added to (vremya, #2065).
            &["--level", "full", "--expires", "infy"],
            &["--level", "full", "--expires", "nany"],
            &["--level", "full", "--expires", "1e3y"],
            &["--level", "full", "--expires", "300000y"],
            &["--level", "full", "--expires", "100000000d"],
        ] {
            assert!(
                parse_sync_cmd(&[&["chain", "agent"], bad].concat()).is_err(),
                "{bad:?}"
            );
        }
        assert!(matches!(
            p(&["restore"]),
            SyncCommand::Restore { code: None }
        ));
        assert!(matches!(
            p(&["restore", "c"]),
            SyncCommand::Restore { code: Some(_) }
        ));
    }

    fn named(fields: &[&str]) -> SyncFilter {
        SyncFilter {
            fields: Some(SyncFields::Named(
                fields.iter().map(|f| f.to_string()).collect(),
            )),
            ..SyncFilter::default()
        }
    }

    /// A window and every spec (da-nflkk): a reader's or a writer's, alone
    /// or beside fields, and alone a reader's.
    #[test]
    fn a_window_and_every_spec_are_a_filter() {
        let light = SyncFilter {
            fields: None,
            window_secs: Some(86_400),
            specs: true,
        };
        match p(&["chain", "invite", "--window", "1d", "--specs"]) {
            SyncCommand::Invite { role, filter, .. } => {
                assert_eq!((role, filter), (SyncRole::Reader, Some(light.clone())))
            }
            other => panic!("{other:?}"),
        }
        match p(&[
            "chain",
            "role",
            "b",
            "writer",
            "--window=2h",
            "--fields",
            "tea",
        ]) {
            SyncCommand::Admin {
                act: SyncAdminAct::Role { role, filter },
                ..
            } => {
                let f = SyncFilter {
                    window_secs: Some(7_200),
                    ..named(&["tea"])
                };
                assert_eq!((role, filter), (SyncRole::Writer, Some(f)));
            }
            other => panic!("{other:?}"),
        }
        match p(&["new-invite", "--specs", "--fields", ""]) {
            SyncCommand::NewInvite { filter, .. } => assert_eq!(
                filter,
                Some(SyncFilter {
                    specs: true,
                    ..named(&[])
                })
            ),
            other => panic!("{other:?}"),
        }
        assert!(parse_sync_cmd(&["chain", "invite", "--window", "soon"]).is_err());
        assert!(parse_sync_cmd(&["chain", "role", "b", "member", "--specs"]).is_err());
    }

    #[test]
    fn parses_each_subcommand() {
        assert_eq!(p(&[]), SyncCommand::Now { verbose: false });
        assert_eq!(p(&["-v"]), SyncCommand::Now { verbose: true });
        assert_eq!(p(&["--verbose"]), SyncCommand::Now { verbose: true });
        assert_eq!(
            p(&["status"]),
            SyncCommand::Status {
                show: Show::Default,
                show_revoked: false,
                no_grants: false
            }
        );
        assert_eq!(
            p(&["status", "--show-revoked"]),
            SyncCommand::Status {
                show: Show::Default,
                show_revoked: true,
                no_grants: false
            }
        );
        assert_eq!(
            p(&["status", "--no-grants"]),
            SyncCommand::Status {
                show: Show::Default,
                show_revoked: false,
                no_grants: true
            }
        );
        assert_eq!(p(&["pause"]), SyncCommand::Pause);
        assert_eq!(p(&["login"]), SyncCommand::Login);
        assert_eq!(p(&["logout"]), SyncCommand::Logout);
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
        assert_eq!(p(&["pipe", "known"]), SyncCommand::Known);
        assert_eq!(p(&["pipe", "hold"]), SyncCommand::Hold);
        assert_eq!(
            p(&["pipe", "standing", "--chain", "c1"]),
            SyncCommand::Standing { chain: "c1".into() }
        );
        assert_eq!(
            p(&["pipe", "apply"]),
            SyncCommand::Apply {
                show: Show::Default
            }
        );
        assert_eq!(
            p(&["pipe", "verify", "--verbose"]),
            SyncCommand::Verify {
                ordered: false,
                verbose: true,
                peer: None
            }
        );
        assert_eq!(
            p(&["pipe", "verify", "--peer", "ab"]),
            SyncCommand::Verify {
                ordered: false,
                verbose: false,
                peer: Some("ab".into())
            }
        );
        assert_eq!(
            p(&["pipe", "verify", "--ordered"]),
            SyncCommand::Verify {
                ordered: true,
                verbose: false,
                peer: None
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
                floor: None,
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
                "--floor",
                "56",
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
                floor: Some(56),
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
                role: SyncRole::Member,
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
                role: SyncRole::Reader,
                filter: None,
                qr: true,
                expires_secs: WEEK,
                label: Some("laptop".into()),
                user: None,
                admin_key: Some("k".into())
            }
        );
        assert_eq!(
            p(&["chain", "join"]),
            SyncCommand::Join {
                ticket: None,
                label: None,
                user: None
            }
        );
        assert_eq!(
            p(&["chain", "join", "--label", "phone", "--user", "ann", "t"]),
            SyncCommand::Join {
                ticket: Some("t".into()),
                label: Some("phone".into()),
                user: Some("ann".into())
            }
        );
        assert!(parse_sync_cmd(&["chain", "join", "t", "u"]).is_err());
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
                label: Some(Some("phone".into())),
                user: None,
                admin_key: Some("k".into()),
                check: false
            }
        );
        let rename = |argv: &[&str]| match p(argv) {
            SyncCommand::Rename {
                who, label, user, ..
            } => (who, label, user),
            other => panic!("{other:?}"),
        };
        let some = |s: &str| Some(s.to_string());
        // `--user` alone keeps the label; with LABEL or `--unset` it changes it too.
        assert_eq!(
            rename(&["chain", "rename", "--user", "ann"]),
            (None, None, some("ann"))
        );
        assert_eq!(
            rename(&["chain", "rename", "desk", "--user", "ann"]),
            (None, Some(some("desk")), some("ann"))
        );
        assert_eq!(
            rename(&["chain", "rename", "--unset", "--user", "ann"]),
            (None, Some(None), some("ann"))
        );
        assert_eq!(
            rename(&["chain", "rename", "--unset"]),
            (None, Some(None), None)
        );
        // The username is this install's, never another member's.
        assert!(parse_sync_cmd(&["chain", "rename", "abcd", "desk", "--user", "ann"]).is_err());
        assert!(parse_sync_cmd(&["chain", "rename", "abcd", "--unset", "--user", "ann"]).is_err());
        assert!(parse_sync_cmd(&["chain", "rename"]).is_err());
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
            p(&["chain", "expel", "b"]),
            SyncCommand::Admin {
                act: SyncAdminAct::Expel {
                    unjoin: false,
                    wipe: false,
                    yes: false,
                    show: Show::Default,
                },
                who: "b".into(),
                label: None,
                admin_key: None,
                allow_unlock: false
            }
        );
        let expel = |unjoin, wipe, yes, admin_key: Option<&str>| SyncCommand::Admin {
            act: SyncAdminAct::Expel {
                unjoin,
                wipe,
                yes,
                show: Show::Default,
            },
            who: "b".into(),
            label: None,
            admin_key: admin_key.map(Into::into),
            allow_unlock: false,
        };
        assert_eq!(
            p(&[
                "chain",
                "expel",
                "b",
                "--unjoin",
                "--yes",
                "--admin-key",
                "k"
            ]),
            expel(true, false, true, Some("k"))
        );
        assert_eq!(
            p(&["chain", "expel", "b", "--wipe", "--yes"]),
            expel(false, true, true, None)
        );
        assert_eq!(
            p(&["chain", "expel", "--wipe", "b", "--unjoin"]),
            expel(true, true, false, None)
        );
        assert!(parse_sync_cmd(&["chain", "expel", "--unjoin"]).is_err());
        for gone in ["unjoin", "wipe"] {
            let old = parse_sync_cmd(&["chain", gone, "b", "--yes"]).unwrap_err();
            assert!(old.to_string().contains("unknown sync chain"), "{old}");
        }
        let old = parse_sync_cmd(&["chain", "purge", "b", "--yes"]).unwrap_err();
        assert!(
            old.to_string()
                .contains("`sync chain expel <WHO> --unjoin`"),
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
                    role: SyncRole::Reader,
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
                    role: SyncRole::Reader,
                    filter: Some(named(&["coffee", "tea"]))
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
                role: SyncRole::Reader,
                filter: Some(SyncFilter {
                    fields: Some(SyncFields::Kit("health".into())),
                    ..SyncFilter::default()
                }),
                qr: false,
                expires_secs: WEEK,
                label: None,
                user: None,
                admin_key: None
            }
        );
        assert!(parse_sync_cmd(&["chain", "role", "b"]).is_err());
        // A writer (da-o5o6m): filtered or not, and alone its filter may
        // name no field.
        assert_eq!(
            p(&["chain", "invite", "--writer", "--fields", ""]),
            SyncCommand::Invite {
                role: SyncRole::Writer,
                filter: Some(named(&[])),
                qr: false,
                expires_secs: WEEK,
                label: None,
                user: None,
                admin_key: None
            }
        );
        assert!(parse_sync_cmd(&["chain", "invite", "--reader", "--writer"]).is_err());
        assert!(parse_sync_cmd(&["chain", "invite", "--reader", "--fields", ""]).is_err());
        assert_eq!(
            p(&["chain", "role", "b", "writer"]),
            SyncCommand::Admin {
                act: SyncAdminAct::Role {
                    role: SyncRole::Writer,
                    filter: None
                },
                who: "b".into(),
                label: None,
                admin_key: None,
                allow_unlock: false
            }
        );
        assert_eq!(
            p(&["new-invite", "--writer"]),
            SyncCommand::NewInvite {
                role: SyncRole::Writer,
                filter: None,
                expires_secs: WEEK,
                admin_key: None
            }
        );
        assert_eq!(
            p(&["chain", "invite", "9ae6", "--writer"]),
            SyncCommand::Grant {
                device: "9ae6".into(),
                role: SyncRole::Writer,
                name: None,
                expires_secs: WEEK,
                admin_key: None,
                ticket: None
            }
        );
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
        assert_eq!(p(&["chain", "upgrade"]), SyncCommand::Upgrade);
        assert_eq!(
            p(&["pin", "v3.2", "--admin-key", "k"]),
            SyncCommand::Pin {
                version: Some("3.2".into()),
                admin_key: Some("k".into())
            }
        );
        assert_eq!(
            p(&["unpin"]),
            SyncCommand::Pin {
                version: None,
                admin_key: None
            }
        );
        for bad in [&["pin"][..], &["pin", "latest"], &["unpin", "3.2"]] {
            assert!(parse_sync_cmd(bad).is_err(), "{bad:?}");
        }
        assert_eq!(p(&["chain", "rekey"]), SyncCommand::Rekey);
        assert_eq!(p(&["chain", "reenter"]), SyncCommand::Reenter);
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
        assert!(parse_sync_cmd(&["chain", "expel", "k", "--label", "x"]).is_err());
        for group in ["chain", "admin", "daemon", "pipe"] {
            let err = parse_sync_cmd(&[group]).unwrap_err().to_string();
            assert!(
                err.contains(&format!("missing sync {group} subcommand")),
                "{err}"
            );
            assert!(parse_sync_cmd(&[group, "nope"]).is_err());
        }
        assert!(parse_sync_cmd(&["chain", "expel"]).is_err());
        assert!(parse_sync_cmd(&["chain", "expel", "a", "b"]).is_err());
        assert!(parse_sync_cmd(&["chain", "rename"]).is_err());
        assert!(parse_sync_cmd(&["chain", "rekey", "extra"]).is_err());
        assert!(parse_sync_cmd(&["chain", "invite", "--expires", "0s"]).is_err());
        assert!(parse_sync_cmd(&["pipe", "vector", "extra"]).is_err());
        assert!(parse_sync_cmd(&["status", "extra"]).is_err());
    }

    /// `chain invite <DEVICE>` (da-n0uj1.4, da-p76eu): a member unless
    /// `--reader`, a week's expiry unless told, and datum.day's ticket
    /// comes with its invite or not at all.
    #[test]
    fn invite_takes_a_device() {
        let invite = |rest: &[&'static str]| [&["chain", "invite"][..], rest].concat();
        assert_eq!(
            p(&invite(&["9ae6", "--reader", "--as", "work"])),
            SyncCommand::Grant {
                device: "9ae6".into(),
                role: SyncRole::Reader,
                name: Some("work".into()),
                expires_secs: WEEK,
                admin_key: None,
                ticket: None,
            }
        );
        assert_eq!(
            p(&invite(&["9ae6", "--expires", "1h"])),
            SyncCommand::Grant {
                device: "9ae6".into(),
                role: SyncRole::Member,
                name: None,
                expires_secs: 3600,
                admin_key: None,
                ticket: None,
            }
        );
        assert_eq!(
            p(&invite(&["9ae6", "--ticket", "T", "--invite", "01j"])),
            SyncCommand::Grant {
                device: "9ae6".into(),
                role: SyncRole::Member,
                name: None,
                expires_secs: WEEK,
                admin_key: None,
                ticket: Some(("T".into(), "01j".into())),
            }
        );
        assert!(parse_sync_cmd(&invite(&["9ae6", "--ticket", "T"])).is_err());
        assert!(parse_sync_cmd(&invite(&["9ae6", "tom"])).is_err());
    }

    /// `chain invite --like` (da-n0uj1.5, da-pvax0, da-p76eu): a database,
    /// every device or those `--devices` names, a week's expiry as a device's.
    #[test]
    fn invite_like_takes_a_database_and_devices() {
        assert_eq!(
            p(&["chain", "invite", "--like", "home"]),
            SyncCommand::GrantLike {
                like: "home".into(),
                devices: None,
                expires_secs: WEEK,
                admin_key: None,
                defer: false,
            }
        );
        assert_eq!(
            p(&[
                "chain",
                "invite",
                "--like",
                "home",
                "--devices",
                "9ae6, tom",
                "--expires",
                "1h",
                "--defer",
            ]),
            SyncCommand::GrantLike {
                like: "home".into(),
                devices: Some(vec!["9ae6".into(), "tom".into()]),
                expires_secs: 3600,
                admin_key: None,
                defer: true,
            }
        );
        assert!(parse_sync_cmd(&["chain", "invite", "--like", "home", "--devices", ","]).is_err());
    }

    /// A bare invite holds a week, as a link sent does (da-4qpwf), and takes none of the other forms' flags.
    #[test]
    fn a_bare_invite_keeps_a_week_and_its_own_flags() {
        assert_eq!(
            p(&["chain", "invite", "--qr", "--label", "x", "--user", "ann"]),
            SyncCommand::Invite {
                role: SyncRole::Member,
                filter: None,
                qr: true,
                expires_secs: WEEK,
                label: Some("x".into()),
                user: Some("ann".into()),
                admin_key: None,
            }
        );
    }

    /// Every combination the three forms refuse, each naming the flag.
    #[test]
    fn invite_refuses_what_its_form_does_not_take() {
        let refused: &[(&[&str], &str)] = &[
            (&["9ae6", "--like", "home"], "not both"),
            (&["--as", "x"], "--as goes with a device"),
            (
                &["--ticket", "T", "--invite", "i"],
                "--ticket goes with a device",
            ),
            (&["--invite", "i"], "--invite goes with a device"),
            (&["--devices", "a"], "--devices goes with --like"),
            (&["--defer"], "--defer goes with --like"),
            (&["9ae6", "--devices", "a"], "--devices goes with --like"),
            (&["9ae6", "--defer"], "--defer goes with --like"),
            (&["9ae6", "--qr"], "--qr goes with a bare"),
            (&["9ae6", "--label", "x"], "--label goes with a bare"),
            (&["9ae6", "--fields", "a"], "--fields goes with a bare"),
            (&["9ae6", "--kit", "k"], "--kit goes with a bare"),
            (&["--like", "h", "--as", "x"], "--as goes with a device"),
            (
                &["--like", "h", "--ticket", "T"],
                "--ticket goes with a device",
            ),
            (&["--like", "h", "--qr"], "--qr goes with a bare"),
            (&["--like", "h", "--label", "x"], "--label goes with a bare"),
            (
                &["--like", "h", "--fields", "a"],
                "--fields goes with a bare",
            ),
            (&["--like", "h", "--kit", "k"], "--kit goes with a bare"),
            (
                &["--like", "h", "--reader"],
                "--reader does not go with --like",
            ),
        ];
        for (rest, why) in refused {
            let args = [&["chain", "invite"][..], rest].concat();
            let Err(ParseError::InvalidArg(said)) = parse_sync_cmd(&args) else {
                panic!("{rest:?} is refused");
            };
            assert!(said.contains(why), "{rest:?}: {said}");
        }
    }

    /// `sync grant` went into `chain invite`, with no alias (da-p76eu).
    #[test]
    fn grant_is_no_subcommand() {
        for args in [&["grant", "9ae6"][..], &["grant", "--like", "home"][..]] {
            let Err(ParseError::InvalidArg(said)) = parse_sync_cmd(args) else {
                panic!("{args:?} is refused");
            };
            assert!(said.contains("unknown sync subcommand: 'grant'"), "{said}");
        }
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
