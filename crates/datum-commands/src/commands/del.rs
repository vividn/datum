//! `datum del <id_or_slug>` — mark an entry deleted (tombstone, sync-safe).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::DeleteArgs;

pub fn del(args: DeleteArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_delete(args, db)
}

pub fn del_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("del_from_argv: stub — R2 extracts bespoke parser")
}

pub fn del_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("del_from_string: stub — R2 extracts bespoke parser")
}
