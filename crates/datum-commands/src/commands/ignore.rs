//! `datum ignore <field>` — mark a field as ignored (hidden from `now` etc).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::IgnoreArgs;

pub fn ignore(args: IgnoreArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_ignore(args, db)
}

pub fn ignore_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("ignore_from_argv: stub — R2 extracts bespoke parser")
}

pub fn ignore_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("ignore_from_string: stub — R2 extracts bespoke parser")
}
