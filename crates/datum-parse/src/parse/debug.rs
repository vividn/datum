//! `datum debug` — test situations made in the current database (da-4dvk7).
//! Off the command list on purpose; `datum help debug` documents it.

use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_debug_cmd(tail: &[&str]) -> Result<DebugCommand> {
    let (sub, rest) = match tail.split_first() {
        Some((sub, rest)) => (*sub, rest),
        None => {
            return Err(ParseError::InvalidArg(
                "missing debug subcommand (expected conflicts|ops; see `datum help debug`)".into(),
            ))
        }
    };
    let mut pos = Vec::new();
    for tok in rest {
        match split_flag(tok) {
            FlagSplit::Positional(s) => pos.push(s),
            f => return Err(unknown_flag(f)),
        }
    }
    match sub {
        "conflicts" => match pos.as_slice() {
            [] => Ok(DebugCommand::Conflicts(3)),
            [n] => n.parse().map(DebugCommand::Conflicts).map_err(|_| {
                ParseError::InvalidArg(format!("debug conflicts takes a count, not '{n}'"))
            }),
            _ => Err(ParseError::InvalidArg(
                "debug conflicts takes one count".into(),
            )),
        },
        "ops" => Ok(DebugCommand::Ops(require_pos(&pos, "<HID>")?)),
        other => Err(ParseError::InvalidArg(format!(
            "unknown debug subcommand: '{other}' (expected conflicts|ops)"
        ))),
    }
}
