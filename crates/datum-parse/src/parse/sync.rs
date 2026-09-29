//! `datum sync` — Easy Sync. The core (da-k329q) moves ops as JSON lines on
//! stdin/stdout, so two copies sync through a pipe:
//! `datum --db b sync ops --since "$(datum --db a sync vector)" | datum --db a sync apply`;
//! the rest pairs copies and syncs them over iroh (da-rugg8).

use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_sync_cmd(tail: &[&str]) -> Result<SyncCommand> {
    let (sub, rest) = match tail.split_first() {
        Some((sub, rest)) => (*sub, rest),
        None => ("", tail),
    };
    let mut rest = rest.iter();
    let cmd = match sub {
        "vector" => SyncCommand::Vector,
        "apply" => SyncCommand::Apply,
        "now" => SyncCommand::Now,
        "run" => SyncCommand::Run,
        "peers" => SyncCommand::Peers,
        "forget" => SyncCommand::Forget {
            peer: rest
                .next()
                .ok_or_else(|| ParseError::InvalidArg("forget needs a peer".into()))?
                .to_string(),
        },
        "connect" => SyncCommand::Connect {
            ticket: rest.next().map(|t| t.to_string()),
        },
        "add-client" => {
            let (mut qr, mut expires_secs) = (false, 600);
            while let Some(t) = rest.next() {
                let value = match split_flag(t) {
                    FlagSplit::Long("qr") => {
                        qr = true;
                        continue;
                    }
                    FlagSplit::LongAttached("expires", v) => v,
                    FlagSplit::Long("expires") => rest.next().ok_or_else(|| {
                        ParseError::InvalidArg("--expires needs a duration".into())
                    })?,
                    f => return Err(unknown_flag(f)),
                };
                expires_secs = u64::try_from(crate::time::parse_duration(value)?.num_seconds())
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or_else(|| {
                        ParseError::InvalidArg(format!(
                            "--expires must be a positive duration: {value}"
                        ))
                    })?;
            }
            SyncCommand::AddClient { qr, expires_secs }
        }
        "verify" => {
            let verbose = match rest.next().map(|t| split_flag(t)) {
                Some(FlagSplit::Long("verbose") | FlagSplit::Short('v')) => true,
                Some(f) => return Err(unknown_flag(f)),
                None => false,
            };
            SyncCommand::Verify { verbose }
        }
        "ops" => {
            let since = match rest.next().map(|t| split_flag(t)) {
                Some(FlagSplit::LongAttached("since", v)) => Some(v.to_string()),
                Some(FlagSplit::Long("since")) => Some(
                    rest.next()
                        .ok_or_else(|| {
                            ParseError::InvalidArg("--since needs a version vector".into())
                        })?
                        .to_string(),
                ),
                Some(f) => return Err(unknown_flag(f)),
                None => None,
            };
            SyncCommand::Ops { since }
        }
        other => {
            return Err(ParseError::InvalidArg(format!(
                "unknown sync subcommand: '{other}' (expected add-client|connect|now|run|peers|forget|vector|ops|apply|verify)"
            )))
        }
    };
    match rest.next() {
        Some(t) => Err(unknown_flag(split_flag(t))),
        None => Ok(cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_subcommand() {
        assert_eq!(parse_sync_cmd(&["vector"]).unwrap(), SyncCommand::Vector);
        assert_eq!(parse_sync_cmd(&["apply"]).unwrap(), SyncCommand::Apply);
        assert_eq!(
            parse_sync_cmd(&["verify", "--verbose"]).unwrap(),
            SyncCommand::Verify { verbose: true }
        );
        assert_eq!(
            parse_sync_cmd(&["ops", "--since", "{}"]).unwrap(),
            SyncCommand::Ops {
                since: Some("{}".into())
            }
        );
        assert_eq!(
            parse_sync_cmd(&["ops", "--since={\"a\":1}"]).unwrap(),
            SyncCommand::Ops {
                since: Some("{\"a\":1}".into())
            }
        );
        assert_eq!(
            parse_sync_cmd(&["add-client", "--qr", "--expires", "1h"]).unwrap(),
            SyncCommand::AddClient {
                qr: true,
                expires_secs: 3600
            }
        );
        assert_eq!(
            parse_sync_cmd(&["add-client", "--expires=30s"]).unwrap(),
            SyncCommand::AddClient {
                qr: false,
                expires_secs: 30
            }
        );
        assert_eq!(
            parse_sync_cmd(&["connect"]).unwrap(),
            SyncCommand::Connect { ticket: None }
        );
        assert_eq!(
            parse_sync_cmd(&["forget", "abc"]).unwrap(),
            SyncCommand::Forget { peer: "abc".into() }
        );
        assert!(parse_sync_cmd(&["forget"]).is_err());
        assert!(parse_sync_cmd(&["add-client", "--expires", "0s"]).is_err());
        assert!(parse_sync_cmd(&["now", "extra"]).is_err());
        assert!(parse_sync_cmd(&[]).is_err());
        assert!(parse_sync_cmd(&["vector", "extra"]).is_err());
    }
}
