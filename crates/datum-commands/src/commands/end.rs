//! `datum end` (alias `stop`) — mark the end of a timed block.
//!
//! `stop` is a synonym for `end` — same handler, different verb surface.

use anyhow::Result;
use datum_core::{action::Action, db::Db};

use crate::args::LogArgs;

pub fn end(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Action::Stop, args, db)
}

/// Alias for [`end`] — `stop` and `end` share the same handler.
pub fn stop(args: LogArgs, db: &Db) -> Result<()> {
    end(args, db)
}
