//! `datum track <field>` — enable per-field tracking (opt-in registry entry).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::TrackArgs;

pub fn track(args: TrackArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_track(args, db)
}
