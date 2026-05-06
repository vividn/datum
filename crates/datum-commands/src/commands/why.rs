//! `datum why <id_or_slug>` — show provenance (raw_input + alias_chain).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::WhyArgs;

pub fn why(args: WhyArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_why(args, db)
}
