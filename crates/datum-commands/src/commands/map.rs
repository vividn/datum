//! `datum map <view>` — run a view (non-reduced).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::MapArgs;

pub fn map(args: MapArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_map(args, db)
}

pub fn map_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("map_from_argv: stub — R2 extracts bespoke parser")
}

pub fn map_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("map_from_string: stub — R2 extracts bespoke parser")
}
