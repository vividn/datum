//! `datum info [<target>]` — print database metadata (schema, counts, size).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::InfoArgs;

pub fn info(args: InfoArgs, cli_db: Option<&str>, active_db: &Db) -> Result<()> {
    crate::cmd::cmd_info(args, cli_db, active_db)
}
