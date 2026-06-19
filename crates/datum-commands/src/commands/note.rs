//! `datum note <field>` — timeless jot, identified by *when* (da-r17h).
//!
//! `note` is the auto-id sibling of [`add`](super::add). Both are timeless
//! (no `occur_time`); the split is on identity:
//!   * `note` auto-ids on its `create_time` → slug `field:c<create_time>`.
//!     "Record this moment, won't look it up." Many notes per field coexist
//!     because each create_time is distinct.
//!   * `add` requires an explicit id (a keyed record looked up by *what*).
//!
//! Like `add`, it infers occur/start/stop/switch shape from `start=`/`end=`/
//! `dur=`/`state=` keys, routing through the shared `log_entry` dispatcher.

use anyhow::Result;
use datum_core::{action::Action, db::Db};

use crate::args::LogArgs;

pub fn note(args: LogArgs, db: &Db) -> Result<()> {
    crate::commands::log_core::log_entry(Action::Note, args, db)
}
