//! `datum count` — count live entries, optionally grouped or time-bounded.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::CountArgs;

pub fn count(args: CountArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_count(args, db)
}

pub fn count_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("count_from_argv: stub — R2 extracts bespoke parser")
}

pub fn count_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("count_from_string: stub — R2 extracts bespoke parser")
}
