//! `datum del <id_or_slug>` — mark an entry deleted (tombstone, sync-safe).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::DeleteArgs;

pub fn del(args: DeleteArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_delete(args, db)
}
