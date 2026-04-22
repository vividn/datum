//! `datum get <id_or_slug>` — print a single entry.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::GetArgs;

pub fn get(args: GetArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_get(args, db)
}

pub fn get_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("get_from_argv: stub — R2 extracts bespoke parser")
}

pub fn get_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("get_from_string: stub — R2 extracts bespoke parser")
}
