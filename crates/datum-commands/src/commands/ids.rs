//! `datum ids` — list entry ids (or slugs) in time order.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::IdsArgs;

pub fn ids(args: IdsArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_ids(args, db)
}

pub fn ids_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("ids_from_argv: stub — R2 extracts bespoke parser")
}

pub fn ids_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("ids_from_string: stub — R2 extracts bespoke parser")
}
