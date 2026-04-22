//! `datum spec` — field-spec management (set, show, list, aliases).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::SpecCommand;

pub fn spec(sub: SpecCommand, db: &Db) -> Result<()> {
    crate::cmd::cmd_spec(sub, db)
}

pub fn spec_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("spec_from_argv: stub — R2 extracts bespoke parser")
}

pub fn spec_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("spec_from_string: stub — R2 extracts bespoke parser")
}
