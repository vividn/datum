//! `datum migrate` — legacy datum (CouchDB JSON) → daydumb migration.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::MigrateCommand;

pub fn migrate(cmd: MigrateCommand, db: &Db) -> Result<()> {
    crate::cmd::cmd_migrate(cmd, db)
}

pub fn migrate_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("migrate_from_argv: stub — R2 extracts bespoke parser")
}

pub fn migrate_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("migrate_from_string: stub — R2 extracts bespoke parser")
}
