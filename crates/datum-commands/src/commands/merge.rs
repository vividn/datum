//! `datum merge <id_or_slug>` — merge into an entry's data as datum does:
//! a key both hold collects both values into one list, no duplicates.
//!
//! Thin wrapper over [`crate::commands::update_core::update_with_strategy`]
//! with `UpdateStrategy::Merge`.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::UpdateArgs;
use crate::commands::update_core::{update_batch_from_args, UpdateStrategy};

pub fn merge(args: UpdateArgs, db: &Db) -> Result<()> {
    update_batch_from_args(args, UpdateStrategy::Merge, db)
}
