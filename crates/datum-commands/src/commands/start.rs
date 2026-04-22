//! `datum start <field>` — mark the beginning of a timed block.

use anyhow::Result;
use datum_core::{db::Db, entry::Verb};

use crate::args::LogArgs;

pub fn start(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Verb::Start, args, db)
}

pub fn start_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("start_from_argv: stub — R2 extracts bespoke parser")
}

pub fn start_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("start_from_string: stub — R2 extracts bespoke parser")
}
