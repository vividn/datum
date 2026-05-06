//! `datum spec` — field-spec management (set, show, list, aliases).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::SpecCommand;

pub fn spec(sub: SpecCommand, db: &Db) -> Result<()> {
    crate::cmd::cmd_spec(sub, db)
}
