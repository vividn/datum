//! `datum history <id_or_slug>` — show the modification trail for an entry.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::HistoryArgs;

pub fn history(args: HistoryArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_history(args, db)
}

pub fn history_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("history_from_argv: stub — R2 extracts bespoke parser")
}

pub fn history_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("history_from_string: stub — R2 extracts bespoke parser")
}
