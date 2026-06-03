use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_alias_cmd(tail: &[&str]) -> Result<AliasCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing alias subcommand (show|set|list|remove)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "show" => {
            let name = require_pos(rest, "<NAME>")?;
            Ok(AliasCommand::Show { name })
        }
        "set" => {
            let name = rest
                .first()
                .ok_or_else(|| ParseError::InvalidArg("alias set requires <NAME>".into()))?
                .to_string();
            let expansion = rest
                .get(1)
                .ok_or_else(|| ParseError::InvalidArg("alias set requires <EXPANSION>".into()))?
                .to_string();
            Ok(AliasCommand::Set { name, expansion })
        }
        "list" => Ok(AliasCommand::List),
        "remove" => {
            let name = require_pos(rest, "<NAME>")?;
            Ok(AliasCommand::Remove { name })
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown alias subcommand: '{other}'"
        ))),
    }
}
