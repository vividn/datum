//! `datum now` — snapshot of the currently-tracked fields and their states.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::NowArgs;

pub fn now(args: NowArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_now(args, db)
}

pub fn now_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("now_from_argv: stub — R2 extracts bespoke parser")
}

pub fn now_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("now_from_string: stub — R2 extracts bespoke parser")
}
