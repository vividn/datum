use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_kit_cmd(tail: &[&str]) -> Result<KitCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing kit subcommand (install|export|list|uninstall)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "install" => {
            let mut force = false;
            let mut assume_yes = false;
            let mut pos: Vec<&str> = Vec::new();
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Positional(s) => pos.push(s),
                    FlagSplit::Long("force") | FlagSplit::Short('f') => force = true,
                    FlagSplit::Long("yes") | FlagSplit::Short('y') => assume_yes = true,
                    f => return Err(unknown_flag(f)),
                }
            }
            let path = require_pos(&pos, "<PATH>")?;
            Ok(KitCommand::Install(KitInstallArgs {
                path,
                force,
                assume_yes,
            }))
        }
        "export" => {
            let mut pos: Vec<&str> = Vec::new();
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Positional(s) => pos.push(s),
                    f => return Err(unknown_flag(f)),
                }
            }
            let name = pos
                .first()
                .ok_or_else(|| ParseError::InvalidArg("kit export requires <NAME>".into()))?
                .to_string();
            let path = pos.get(1).map(|s| s.to_string());
            Ok(KitCommand::Export(KitExportArgs { name, path }))
        }
        "list" => Ok(KitCommand::List),
        "uninstall" => {
            let name = require_pos(rest, "<NAME>")?;
            Ok(KitCommand::Uninstall(KitUninstallArgs { name }))
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown kit subcommand: '{other}'"
        ))),
    }
}
