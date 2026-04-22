//! `datum rekey <id_or_slug>` — change an entry's field/time (moves its slug).

use anyhow::Result;
use datum_core::db::{ConflictStrategy, Db};

use crate::args::UpdateArgs;

pub fn rekey(args: UpdateArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_update(args, ConflictStrategy::Rekey, db)
}

pub fn rekey_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("rekey_from_argv: stub — R2 extracts bespoke parser")
}

pub fn rekey_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("rekey_from_string: stub — R2 extracts bespoke parser")
}
