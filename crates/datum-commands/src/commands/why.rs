//! `datum why <id_or_slug>` — show provenance (raw_input + alias_chain).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::WhyArgs;

pub fn why(args: WhyArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_why(args, db)
}

pub fn why_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("why_from_argv: stub — R2 extracts bespoke parser")
}

pub fn why_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("why_from_string: stub — R2 extracts bespoke parser")
}
