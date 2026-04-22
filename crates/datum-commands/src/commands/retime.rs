//! `datum retime <id_or_slug>` — move an entry's time_start/time_end.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::RetimeArgs;

pub fn retime(args: RetimeArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_retime(args, db)
}

pub fn retime_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("retime_from_argv: stub — R2 extracts bespoke parser")
}

pub fn retime_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("retime_from_string: stub — R2 extracts bespoke parser")
}
