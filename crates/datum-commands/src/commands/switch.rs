//! `datum switch <field> <state>` — transition a field to a new state.

use anyhow::Result;
use datum_core::{db::Db, entry::Verb};

use crate::args::LogArgs;

pub fn switch(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Verb::Switch, args, db)
}
