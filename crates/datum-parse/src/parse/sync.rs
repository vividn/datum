//! `datum sync` — Easy Sync. The core (da-k329q) moves ops as JSON lines on
//! stdin/stdout, so two copies sync through a pipe:
//! `datum --db b sync ops --since "$(datum --db a sync vector)" | datum --db a sync apply`;
//! the rest syncs the members of a sync chain over iroh (da-rugg8, da-6177s).

use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::Show;

pub(crate) fn parse_sync_cmd(tail: &[&str]) -> Result<SyncCommand> {
    let (sub, rest) = match tail.split_first() {
        Some((sub, rest)) => (*sub, rest),
        None => ("", tail),
    };
    let mut rest = rest.iter();
    let cmd = match sub {
        "vector" => SyncCommand::Vector,
        // `--json` is the swept `--show json`, set by `apply_show`.
        "apply" => SyncCommand::Apply {
            show: Show::Default,
        },
        "now" => SyncCommand::Now,
        "run" => SyncCommand::Run,
        "rekey" => SyncCommand::Rekey,
        "connect" => SyncCommand::Connect {
            ticket: rest.next().map(|t| t.to_string()),
        },
        "invite" => {
            let o = opts(&mut rest, &["expires", "admin-key"], &["qr"], 0..=0)?;
            SyncCommand::Invite {
                qr: o.on("qr"),
                expires_secs: o.expires()?,
                admin_key: o.get("admin-key"),
            }
        }
        "revoke" | "add-admin" | "remove-admin" => {
            let mut o = opts(&mut rest, &["admin-key", "label"], &[], 1..=1)?;
            SyncCommand::Admin {
                act: match sub {
                    "revoke" => SyncAdminAct::Revoke,
                    "add-admin" => SyncAdminAct::AddAdmin,
                    _ => SyncAdminAct::RemoveAdmin,
                },
                who: o.positional.remove(0),
                label: o.get("label"),
                admin_key: o.get("admin-key"),
            }
        }
        // `--json` is the swept `--show json`, set by `apply_show`.
        "members" => SyncCommand::Members {
            show: Show::Default,
        },
        "status" => SyncCommand::Status {
            show: Show::Default,
        },
        "rename" => {
            let mut o = opts(&mut rest, &["admin-key"], &[], 1..=2)?;
            let label = o.positional.pop().unwrap_or_default();
            SyncCommand::Rename {
                who: o.positional.pop(),
                label,
                admin_key: o.get("admin-key"),
            }
        }
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
            let mut o = opts(&mut rest, &["label", "addr", "invite"], &["dialed"], 1..=1)?;
            let need = |name: &str| {
                o.get(name)
                    .ok_or_else(|| ParseError::InvalidArg(format!("handshake needs --{name}")))
            };
            SyncCommand::Handshake {
                label: need("label")?,
                addr: need("addr")?,
                invite: o.get("invite"),
                dialed: o.on("dialed"),
                endpoint: o.positional.remove(0),
            }
        }
        "new-invite" => {
            let o = opts(&mut rest, &["expires", "admin-key"], &[], 0..=0)?;
            SyncCommand::NewInvite {
                expires_secs: o.expires()?,
                admin_key: o.get("admin-key"),
            }
        }
        "ack" => SyncCommand::Ack {
            who: rest
                .next()
                .ok_or_else(|| ParseError::InvalidArg("ack needs an endpoint or admin".into()))?
                .to_string(),
        },
        "verify" => {
            let verbose = match rest.next().map(|t| split_flag(t)) {
                Some(FlagSplit::Long("verbose") | FlagSplit::Short('v')) => true,
                Some(f) => return Err(unknown_flag(f)),
                None => false,
            };
            SyncCommand::Verify { verbose }
        }
        "ops" => {
            let o = opts(&mut rest, &["since"], &["no-overlap"], 0..=0)?;
            SyncCommand::Ops {
                since: o.get("since"),
                overlap: !o.on("no-overlap"),
            }
        }
        other => {
            return Err(ParseError::InvalidArg(format!(
                "unknown sync subcommand: '{other}' (expected invite|connect|members|status|rename|ack|revoke|add-admin|remove-admin|now|run|rekey|vector|ops|apply|verify)"
            )))
        }
    };
    match rest.next() {
        Some(t) => Err(unknown_flag(split_flag(t))),
        None => Ok(cmd),
    }
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

    #[test]
    fn parses_each_subcommand() {
        assert_eq!(parse_sync_cmd(&["vector"]).unwrap(), SyncCommand::Vector);
        assert_eq!(
            parse_sync_cmd(&["apply"]).unwrap(),
            SyncCommand::Apply {
                show: Show::Default
            }
        );
        assert_eq!(
            parse_sync_cmd(&["verify", "--verbose"]).unwrap(),
            SyncCommand::Verify { verbose: true }
        );
        assert_eq!(
            parse_sync_cmd(&["ops", "--since", "{}"]).unwrap(),
            SyncCommand::Ops {
                since: Some("{}".into()),
                overlap: true
            }
        );
        assert_eq!(
            parse_sync_cmd(&["ops", "--since={\"a\":1}"]).unwrap(),
            SyncCommand::Ops {
                since: Some("{\"a\":1}".into()),
                overlap: true
            }
        );
        assert_eq!(
            parse_sync_cmd(&["ops", "--since", "{}", "--no-overlap"]).unwrap(),
            SyncCommand::Ops {
                since: Some("{}".into()),
                overlap: false
            }
        );
        assert_eq!(
            parse_sync_cmd(&["handshake", "ab", "--label", "l", "--addr={}", "--dialed"]).unwrap(),
            SyncCommand::Handshake {
                endpoint: "ab".into(),
                label: "l".into(),
                addr: "{}".into(),
                invite: None,
                dialed: true
            }
        );
        assert!(parse_sync_cmd(&["handshake", "ab", "--label", "l"]).is_err());
        assert_eq!(
            parse_sync_cmd(&["new-invite", "--expires", "1m"]).unwrap(),
            SyncCommand::NewInvite {
                expires_secs: 60,
                admin_key: None
            }
        );
        assert_eq!(
            parse_sync_cmd(&["invite", "--qr", "--expires", "1h"]).unwrap(),
            SyncCommand::Invite {
                qr: true,
                expires_secs: 3600,
                admin_key: None
            }
        );
        assert_eq!(
            parse_sync_cmd(&["connect"]).unwrap(),
            SyncCommand::Connect { ticket: None }
        );
        assert_eq!(parse_sync_cmd(&["rekey"]).unwrap(), SyncCommand::Rekey);
        assert_eq!(
            parse_sync_cmd(&["invite", "--admin-key=k", "--qr"]).unwrap(),
            SyncCommand::Invite {
                qr: true,
                expires_secs: 600,
                admin_key: Some("k".into())
            }
        );
        assert_eq!(
            parse_sync_cmd(&["add-admin", "k", "--label", "desk"]).unwrap(),
            SyncCommand::Admin {
                act: SyncAdminAct::AddAdmin,
                who: "k".into(),
                label: Some("desk".into()),
                admin_key: None
            }
        );
        for gone in ["init", "add-endpoint", "add-client", "peers", "forget"] {
            assert!(parse_sync_cmd(&[gone]).is_err(), "{gone}");
        }
        assert!(parse_sync_cmd(&["revoke"]).is_err());
        assert!(parse_sync_cmd(&["revoke", "a", "b"]).is_err());
        assert_eq!(
            parse_sync_cmd(&["ack", "abc"]).unwrap(),
            SyncCommand::Ack { who: "abc".into() }
        );
        assert!(parse_sync_cmd(&["rekey", "extra"]).is_err());
        assert_eq!(
            parse_sync_cmd(&["rename", "desk"]).unwrap(),
            SyncCommand::Rename {
                who: None,
                label: "desk".into(),
                admin_key: None
            }
        );
        assert_eq!(
            parse_sync_cmd(&["rename", "abcd", "phone", "--admin-key", "k"]).unwrap(),
            SyncCommand::Rename {
                who: Some("abcd".into()),
                label: "phone".into(),
                admin_key: Some("k".into())
            }
        );
        assert!(parse_sync_cmd(&["rename"]).is_err());
        assert!(parse_sync_cmd(&["rename", "a", "b", "c"]).is_err());
        assert_eq!(
            parse_sync_cmd(&["members"]).unwrap(),
            SyncCommand::Members {
                show: Show::Default
            }
        );
        assert_eq!(
            parse_sync_cmd(&["record-session", "ab", "--vector", "{}", "--verify=match"]).unwrap(),
            SyncCommand::RecordSession {
                endpoint: "ab".into(),
                vector: Some("{}".into()),
                verify: Some(true),
                error: None
            }
        );
        assert!(parse_sync_cmd(&["record-session", "ab", "--verify", "maybe"]).is_err());
        assert!(parse_sync_cmd(&["invite", "--expires", "0s"]).is_err());
        assert!(parse_sync_cmd(&["now", "extra"]).is_err());
        assert!(parse_sync_cmd(&[]).is_err());
        assert!(parse_sync_cmd(&["vector", "extra"]).is_err());
    }
}
