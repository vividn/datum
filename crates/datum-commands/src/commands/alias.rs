//! `datum alias` — CLI alias management (set, list, delete).
//!
//! Aliases are stored in the database and resolved in the CLI pre-pass
//! before the parser sees argv (see `crate::args::expand_argv`).

use anyhow::Result;
use datum_core::db::Db;

use crate::args::AliasCommand;

pub fn alias(sub: AliasCommand, db: &Db) -> Result<()> {
    crate::cmd::cmd_alias(sub, db)
}

pub fn alias_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("alias_from_argv: stub — R2 extracts bespoke parser")
}

pub fn alias_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("alias_from_string: stub — R2 extracts bespoke parser")
}
