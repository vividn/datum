//! `datum history <id_or_slug>` — show the modification trail for an entry.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::HistoryArgs;

pub fn history(args: HistoryArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_history(args, db)
}
