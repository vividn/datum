//! `datum conflicts` — D5 conflict queue management (list, resolve, auto-resolve).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::ConflictsCommand;

pub fn conflicts(sub: ConflictsCommand, db: &Db) -> Result<()> {
    crate::cmd::cmd_conflicts(sub, db)
}

pub fn conflicts_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("conflicts_from_argv: stub — R2 extracts bespoke parser")
}

pub fn conflicts_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("conflicts_from_string: stub — R2 extracts bespoke parser")
}
