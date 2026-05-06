//! `datum grep <pattern>` — regex search over entries.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::GrepArgs;

pub fn grep(args: GrepArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_grep(args, db)
}
