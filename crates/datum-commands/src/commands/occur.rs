//! `datum occur <field>` — record a point-in-time event.
//!
//! Thin wrapper around the shared `log_entry` dispatcher in `crate::cmd`.

use anyhow::Result;
use datum_core::{db::Db, entry::Verb};

use crate::args::LogArgs;

pub fn occur(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Verb::Occur, args, db)
}
