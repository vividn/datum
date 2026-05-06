//! `datum start <field>` — mark the beginning of a timed block.

use anyhow::Result;
use datum_core::{db::Db, entry::Verb};

use crate::args::LogArgs;

pub fn start(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Verb::Start, args, db)
}
