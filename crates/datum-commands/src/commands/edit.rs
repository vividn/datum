//! `datum edit <id_or_slug>` — open an entry's JSON in $EDITOR.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::EditArgs;

pub fn edit(args: EditArgs, db: &Db) -> Result<()> {
    crate::cmd::cmd_edit(args, db)
}

pub fn edit_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("edit_from_argv: stub — R2 extracts bespoke parser")
}

pub fn edit_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("edit_from_string: stub — R2 extracts bespoke parser")
}
