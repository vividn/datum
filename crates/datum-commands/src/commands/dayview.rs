//! `datum dayview [--date YYYY-MM-DD]` — render a day's timeline (SVG/JSON).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::DayviewArgs;

pub fn dayview(args: DayviewArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_dayview(args, db)
}

pub fn dayview_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("dayview_from_argv: stub — R2 extracts bespoke parser")
}

pub fn dayview_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("dayview_from_string: stub — R2 extracts bespoke parser")
}
