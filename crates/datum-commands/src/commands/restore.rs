//! `datum restore <path>` — replay entries from a backup.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::RestoreArgs;

pub fn restore(args: RestoreArgs, db: &Db) -> Result<()> {
    super::backup_restore::cmd_restore(args, db)
}

pub fn restore_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("restore_from_argv: stub — R2 extracts bespoke parser")
}

pub fn restore_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("restore_from_string: stub — R2 extracts bespoke parser")
}
