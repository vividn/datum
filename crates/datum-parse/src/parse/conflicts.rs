//! `datum conflicts` — sync-conflict triage (da-ttsm, shape per da-q0fm).
//!
//! Scoped to the sync axis on purpose. da-z46t removed a
//! `list/resolve/auto-resolve` surface over the *whole* integrity queue
//! because integrity findings (double_open, orphan_end, overlap, …)
//! resolve by fixing the data — `datum check` shows them and `datum
//! edit`/`delete` clears them; a resolution menu would pretend otherwise.
//! What genuinely needs a picker is the pair of kinds sync-pull files when
//! two devices claim one identity: `lww_overwrite` and
//! `identity_collision`. Those are what this command lists.
//!
//! A bare `datum conflicts` is `conflicts list` — "what came back from
//! the other device?" is the whole reason to type it.

use super::core::*;
use crate::cli::*;
use crate::error::{ParseError, Result};

pub(crate) fn parse_conflicts_cmd(tail: &[&str]) -> Result<ConflictsCommand> {
    let (sub, rest): (&str, &[&str]) = match tail.first() {
        Some(t) if !looks_like_flag(t) => (*t, &tail[1..]),
        _ => ("list", tail),
    };

    match sub {
        "list" => {
            let mut args = ConflictsListArgs::default();
            for tok in rest {
                match split_flag(tok) {
                    FlagSplit::Long("all") | FlagSplit::Short('a') => args.all = true,
                    f => return Err(unknown_flag(f)),
                }
            }
            Ok(ConflictsCommand::List(args))
        }
        "resolve" => {
            let mut keep: Option<ConflictsKeep> = None;
            let mut pos: Vec<&str> = Vec::new();
            for tok in rest {
                let choice = match split_flag(tok) {
                    FlagSplit::Positional(s) => {
                        pos.push(s);
                        continue;
                    }
                    FlagSplit::Long("take-local") => ConflictsKeep::Local,
                    FlagSplit::Long("take-remote") => ConflictsKeep::Remote,
                    FlagSplit::Long("keep-both") => ConflictsKeep::Both,
                    f => return Err(unknown_flag(f)),
                };
                if keep.is_some_and(|prior| prior != choice) {
                    return Err(ParseError::InvalidArg(
                        "conflicts resolve takes exactly one of --take-local / \
                         --take-remote / --keep-both"
                            .into(),
                    ));
                }
                keep = Some(choice);
            }
            let hid = require_pos(&pos, "<HID>")?;
            let keep = keep.ok_or_else(|| {
                ParseError::InvalidArg(
                    "conflicts resolve needs --take-local, --take-remote, or --keep-both".into(),
                )
            })?;
            Ok(ConflictsCommand::Resolve(ConflictsResolveArgs {
                hid,
                keep,
            }))
        }
        other => Err(ParseError::InvalidArg(format!(
            "unknown conflicts subcommand: '{other}' (expected list|resolve)"
        ))),
    }
}
