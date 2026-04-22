//! `datum end` (alias `stop`) — mark the end of a timed block.
//!
//! `stop` is a synonym for `end` — same handler, different verb surface.

use anyhow::Result;
use datum_core::{db::Db, entry::Verb};

use crate::args::LogArgs;

pub fn end(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Verb::Stop, args, db)
}

/// Alias for [`end`] — `stop` and `end` share the same handler.
pub fn stop(args: LogArgs, db: &Db) -> Result<()> {
    end(args, db)
}

pub fn end_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("end_from_argv: stub — R2 extracts bespoke parser")
}

pub fn end_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("end_from_string: stub — R2 extracts bespoke parser")
}
