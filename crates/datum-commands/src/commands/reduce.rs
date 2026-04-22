//! `datum reduce <view>` — run a view with reduce enabled (grouping + rollup).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::MapArgs;

pub fn reduce(args: MapArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_reduce(args, db)
}

pub fn reduce_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("reduce_from_argv: stub — R2 extracts bespoke parser")
}

pub fn reduce_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("reduce_from_string: stub — R2 extracts bespoke parser")
}
