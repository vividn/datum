//! `datum sync` — Easy Sync's core (da-k329q). Ops travel as JSON lines on
//! stdin/stdout, so two copies sync through a pipe:
//! `datum --db b sync ops --since "$(datum --db a sync vector)" | datum --db a sync apply`.

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
                "unknown sync subcommand: '{other}' (expected vector|ops|apply|verify)"
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
        assert!(parse_sync_cmd(&[]).is_err());
        assert!(parse_sync_cmd(&["vector", "extra"]).is_err());
    }
}
