use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};
use crate::input::Show;

pub(crate) fn parse_alias_cmd(tail: &[&str]) -> Result<AliasCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing alias subcommand (show|set|list|remove|install|uninstall)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "show" => {
            let name = require_pos(&positionals(rest)?, "<NAME>")?;
            Ok(AliasCommand::Show { name })
        }
        "set" => {
            // Only the NAME may not be a flag: an EXPANSION may start with
            // one (`alias set yc "-y occur coffee"`). A third word is
            // refused, not dropped (da-m99fr).
            let name = rest
                .first()
                .ok_or_else(|| ParseError::InvalidArg("alias set requires <NAME>".into()))?;
            if looks_like_flag(name) {
                return Err(unknown_flag(split_flag(name)));
            }
            let expansion = require_pos(&rest[1..], "<EXPANSION>")?;
            let name = name.to_string();
            Ok(AliasCommand::Set { name, expansion })
        }
        "list" => {
            let names = rest.first() == Some(&"--names");
            if let Some(tok) = rest.get(usize::from(names)) {
                return Err(unknown_flag(split_flag(tok)));
            }
            Ok(AliasCommand::List {
                show: Show::default(),
                names,
            })
        }
        "remove" => {
            let name = require_pos(&positionals(rest)?, "<NAME>")?;
            Ok(AliasCommand::Remove { name })
        }
        "install" | "uninstall" => {
            let shell = require_pos(&positionals(rest)?, "<zsh|bash>")?;
            if !matches!(shell.as_str(), "zsh" | "bash") {
                return Err(ParseError::InvalidArg(format!(
                    "alias {sub}: unknown shell '{shell}' (zsh|bash)"
                )));
            }
            Ok(match *sub {
                "install" => AliasCommand::Install { shell },
                _ => AliasCommand::Uninstall { shell },
            })
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown alias subcommand: '{other}'"
        ))),
    }
}
