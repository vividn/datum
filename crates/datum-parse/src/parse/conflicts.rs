//! `datum conflicts` — the entries whose ops leave a key contested (da-05rkv):
//! values two copies set without seeing each other's, which the sync fold
//! writes into the entry's `conflicts` column on every copy alike.
//! Integrity findings (double_open, orphan_end, overlap, …) are not here:
//! they resolve by fixing the data, and `datum check` shows them.
//!
//! A bare `datum conflicts` is `conflicts list`.

use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_conflicts_cmd(tail: &[&str]) -> Result<ConflictsCommand> {
    let (sub, rest): (&str, &[&str]) = match tail.first() {
        Some(t) if !looks_like_flag(t) => (*t, &tail[1..]),
        _ => ("list", tail),
    };

    match sub {
        "list" => match rest.first() {
            None => Ok(ConflictsCommand::List(ConflictsListArgs::default())),
            Some(tok) => Err(unknown_flag(split_flag(tok))),
        },
        "resolve" => {
            let mut pos: Vec<&str> = Vec::new();
            let mut picks = Vec::new();
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Positional(s) => match s.split_once('=') {
                        Some((k, v)) if !pos.is_empty() => picks.push((k.into(), v.into())),
                        _ => pos.push(s),
                    },
                    f => return Err(unknown_flag(f)),
                }
            }
            if pos.len() > 1 {
                return Err(ParseError::InvalidArg(format!(
                    "conflicts resolve takes one entry, then KEY=VALUE picks: '{}'",
                    pos[1]
                )));
            }
            let hid = require_pos(&pos, "<HID>")?;
            Ok(ConflictsCommand::Resolve(ConflictsResolveArgs {
                hid,
                picks,
            }))
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown conflicts subcommand: '{other}' (expected list|resolve)"
        ))),
    }
}
