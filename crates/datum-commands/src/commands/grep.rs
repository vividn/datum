//! `datum grep <pattern>` — regex search over entries.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::GrepArgs;

pub fn grep(args: GrepArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_grep(args, db)
}

pub fn grep_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("grep_from_argv: stub — R2 extracts bespoke parser")
}

pub fn grep_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("grep_from_string: stub — R2 extracts bespoke parser")
}
