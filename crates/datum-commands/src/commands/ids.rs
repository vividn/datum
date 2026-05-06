//! `datum ids` — list entry ids (or slugs) in time order.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::IdsArgs;

pub fn ids(args: IdsArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_ids(args, db)
}
