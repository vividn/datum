//! `datum now` — snapshot of the currently-tracked fields and their states.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::NowArgs;

pub fn now(args: NowArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_now(args, db)
}
