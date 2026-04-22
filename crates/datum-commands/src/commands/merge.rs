//! `datum merge <id_or_slug>` — deep-merge into an existing entry's payload.

use anyhow::Result;
use datum_core::db::{ConflictStrategy, Db};

use crate::args::UpdateArgs;

pub fn merge(args: UpdateArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_update(args, ConflictStrategy::Merge, db)
}

pub fn merge_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("merge_from_argv: stub — R2 extracts bespoke parser")
}

pub fn merge_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("merge_from_string: stub — R2 extracts bespoke parser")
}
