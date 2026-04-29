//! `datum backup <path>` — JSONL or tar.gz snapshot of the entry set.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::BackupArgs;

pub fn backup(args: BackupArgs, db: &Db) -> Result<()> {
    super::backup_restore::cmd_backup(args, db)
}

pub fn backup_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("backup_from_argv: stub — R2 extracts bespoke parser")
}

pub fn backup_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("backup_from_string: stub — R2 extracts bespoke parser")
}
