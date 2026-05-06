//! `datum get <id_or_slug>` — print a single entry.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::GetArgs;

pub fn get(args: GetArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_get(args, db)
}
