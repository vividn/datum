//! `datum count` — count live entries, optionally grouped or time-bounded.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::CountArgs;

pub fn count(args: CountArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_count(args, db)
}
