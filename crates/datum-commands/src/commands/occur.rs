//! `datum occur <field>` — record a point-in-time event.
//!
//! Thin wrapper around the shared `log_entry` dispatcher in `crate::cmd`.

use anyhow::Result;
use datum_core::{action::Action, db::Db};

use crate::args::LogArgs;

pub fn occur(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Action::Occur, args, db)
}
