//! `datum occur <field>` — record a point-in-time event.
//!
//! R1 wrapper around the shared `log_entry` dispatcher in `crate::cmd`.
//! R2 will extract argv/string parsing into this file.

use anyhow::Result;
use datum_core::{db::Db, entry::Verb};

use crate::args::LogArgs;

/// Typed API — callable from programs once R2 splits args parsing.
pub fn occur(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Verb::Occur, args, db)
}

/// Parse argv and run the command. R1 stub.
pub fn occur_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("occur_from_argv: stub — R2 extracts bespoke parser")
}

/// Parse a single string (frontend text entry) and run the command. R1 stub.
pub fn occur_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("occur_from_string: stub — R2 extracts bespoke parser")
}
