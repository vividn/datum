//! `datum setup` — initialize or migrate the SQLite schema.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::SetupArgs;

pub fn setup(args: SetupArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_setup(args, db)
}

pub fn setup_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("setup_from_argv: stub — R2 extracts bespoke parser")
}

pub fn setup_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("setup_from_string: stub — R2 extracts bespoke parser")
}
