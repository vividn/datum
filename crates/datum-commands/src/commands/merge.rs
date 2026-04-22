//! `datum merge <id_or_slug>` — deep-merge into an entry's payload.
//!
//! Thin wrapper over [`crate::commands::update_core::update_with_strategy`]
//! with `UpdateStrategy::Merge`.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::UpdateArgs;
use crate::commands::update_core::{update_from_args, UpdateStrategy};

pub fn merge(args: UpdateArgs, db: &Db) -> Result<()> {
    update_from_args(args, UpdateStrategy::Merge, db).map(|_| ())
}

pub fn merge_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("merge_from_argv: stub — R2 extracts bespoke parser")
}

pub fn merge_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("merge_from_string: stub — R2 extracts bespoke parser")
}
