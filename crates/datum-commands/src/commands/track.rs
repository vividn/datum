//! `datum track <field>` — enable per-field tracking (opt-in registry entry).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::TrackArgs;

pub fn track(args: TrackArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_track(args, db)
}

pub fn track_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("track_from_argv: stub — R2 extracts bespoke parser")
}

pub fn track_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("track_from_string: stub — R2 extracts bespoke parser")
}
