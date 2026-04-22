//! `datum add <field>` — freeform entry; infer shape from keys.
//!
//! `add` infers occur/start/stop/switch semantics from the presence of
//! `start=`, `end=`, `dur=`, or `state=` keys (see D3). Under the hood it
//! routes through the shared `log_entry` dispatcher.

use anyhow::Result;
use datum_core::{db::Db, entry::Verb};

use crate::args::LogArgs;

pub fn add(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Verb::Add, args, db)
}

pub fn add_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("add_from_argv: stub — R2 extracts bespoke parser")
}

pub fn add_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("add_from_string: stub — R2 extracts bespoke parser")
}
