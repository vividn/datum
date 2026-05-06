//! `datum undo [steps]` — roll back recent writes via the EntryHistory trail.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::UndoArgs;

pub fn undo(args: UndoArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_undo(args, db)
}
