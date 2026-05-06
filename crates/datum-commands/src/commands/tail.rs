//! `datum tail` — list the most recent entries.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::TailArgs;

pub fn tail(args: TailArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_tail(args, db)
}
