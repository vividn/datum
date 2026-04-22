//! `datum tail` — list the most recent entries.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::TailArgs;

pub fn tail(args: TailArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_tail(args, db)
}

pub fn tail_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("tail_from_argv: stub — R2 extracts bespoke parser")
}

pub fn tail_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("tail_from_string: stub — R2 extracts bespoke parser")
}
