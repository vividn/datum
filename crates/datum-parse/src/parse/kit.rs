use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_kit_cmd(tail: &[&str]) -> Result<KitCommand> {
    let Some(sub) = tail.first() else {
        return Err(ParseError::InvalidArg(
            "missing kit subcommand (install|export|list|uninstall|show)".into(),
        ));
    };
    let rest: &[&str] = &tail[1..];
    match *sub {
        "install" => {
            let mut force = false;
            let mut assume_yes = false;
            let mut dry_run = false;
            let mut set = std::collections::BTreeMap::new();
            let mut pos: Vec<&str> = Vec::new();
            let mut i = 0;
            while i < rest.len() {
                let mut next = i + 1;
                match split_flag(rest[i]) {
                    FlagSplit::Positional(s) => pos.push(s),
                    FlagSplit::Long("force") | FlagSplit::Short('f') => force = true,
                    FlagSplit::Long("yes") | FlagSplit::Short('y') => assume_yes = true,
                    FlagSplit::Long("dry-run") => dry_run = true,
                    FlagSplit::Long("set") | FlagSplit::LongAttached("set", _) => {
                        let attached = rest[i].strip_prefix("--set=");
                        let (v, ni) = take_value(rest, i, attached, "--set")?;
                        let Some((name, value)) = v.split_once('=') else {
                            return Err(ParseError::InvalidArg(format!(
                                "--set takes NAME=VALUE, not '{v}'"
                            )));
                        };
                        set.insert(name.to_string(), value.to_string());
                        next = ni;
                    }
                    f => return Err(unknown_flag(f)),
                }
                i = next;
            }
            let path = require_pos(&pos, "<PATH>")?;
            Ok(KitCommand::Install(KitInstallArgs {
                path,
                force,
                assume_yes,
                dry_run,
                set,
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
            let name = require_pos(&positionals(rest)?, "<NAME>")?;
            Ok(KitCommand::Uninstall(KitUninstallArgs { name }))
        }
        "show" => {
            let name = require_pos(&positionals(rest)?, "<NAME>")?;
            Ok(KitCommand::Show { name })
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown kit subcommand: '{other}'"
        ))),
    }
}
