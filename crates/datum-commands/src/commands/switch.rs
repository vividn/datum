//! `datum switch <field> <state>` — transition a field to a new state.

use anyhow::Result;
use datum_core::{db::Db, entry::Verb};

use crate::args::LogArgs;

pub fn switch(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Verb::Switch, args, db)
}

pub fn switch_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("switch_from_argv: stub — R2 extracts bespoke parser")
}

pub fn switch_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("switch_from_string: stub — R2 extracts bespoke parser")
}
