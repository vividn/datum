//! `datum start <field>` — mark the beginning of a timed block.

use anyhow::Result;
use datum_core::{action::Action, db::Db};

use crate::args::LogArgs;

pub fn start(args: LogArgs, db: &Db) -> Result<()> {
    crate::commands::log_core::log_entry(Action::Start, args, db)
}
