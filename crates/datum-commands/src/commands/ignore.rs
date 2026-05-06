//! `datum ignore <field>` — mark a field as ignored (hidden from `now` etc).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::IgnoreArgs;

pub fn ignore(args: IgnoreArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_ignore(args, db)
}
