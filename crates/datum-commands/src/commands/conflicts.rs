//! `datum conflicts` — D5 conflict queue management (list, resolve, auto-resolve).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::ConflictsCommand;

pub fn conflicts(sub: ConflictsCommand, db: &Db) -> Result<()> {
    crate::cmd::cmd_conflicts(sub, db)
}
