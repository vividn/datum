//! `datum head` — list the oldest entries.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::HeadArgs;

pub fn head(args: HeadArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_head(args, db)
}

pub fn head_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("head_from_argv: stub — R2 extracts bespoke parser")
}

pub fn head_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("head_from_string: stub — R2 extracts bespoke parser")
}
