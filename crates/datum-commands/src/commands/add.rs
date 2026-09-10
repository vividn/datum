//! `datum add <field>` — freeform entry; infer shape from keys.
//!
//! `add` infers occur/start/stop/switch semantics from the presence of
//! `start_time=`, `end_time=`, `dur=`, or `state=` keys (see D3; the
//! time keys are lifted through the `-t` path, da-mjwq). Under the hood it
//! routes through the shared `log_entry` dispatcher.

use anyhow::Result;
use datum_core::{action::Action, db::Db};

use crate::args::LogArgs;

pub fn add(args: LogArgs, db: &Db) -> Result<()> {
    crate::commands::log_core::log_entry(Action::Add, args, db)
}
