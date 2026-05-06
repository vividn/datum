//! `datum retime <id_or_slug>` — move an entry's time_start/time_end.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::RetimeArgs;

pub fn retime(args: RetimeArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_retime(args, db)
}
