//! `datum undo [steps]` — roll back recent writes via the EntryHistory trail.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::UndoArgs;

pub fn undo(args: UndoArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_undo(args, db)
}

pub fn undo_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("undo_from_argv: stub — R2 extracts bespoke parser")
}

pub fn undo_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("undo_from_string: stub — R2 extracts bespoke parser")
}
