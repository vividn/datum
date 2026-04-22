//! `datum update <id_or_slug>` — overwrite an entry's payload.
//!
//! Shares a handler with `merge` and `rekey` — the only difference is the
//! default conflict strategy.

use anyhow::Result;
use datum_core::db::{ConflictStrategy, Db};

use crate::args::UpdateArgs;

pub fn update(args: UpdateArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_update(args, ConflictStrategy::Update, db)
}

pub fn update_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("update_from_argv: stub — R2 extracts bespoke parser")
}

pub fn update_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("update_from_string: stub — R2 extracts bespoke parser")
}
