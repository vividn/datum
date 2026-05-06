//! `datum update <id_or_slug>` — shallow-replace an entry's payload.
//!
//! Thin wrapper over [`crate::commands::update_core::update_with_strategy`]
//! with `UpdateStrategy::Replace`.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::UpdateArgs;
use crate::commands::update_core::{update_batch_from_args, UpdateStrategy};

pub fn update(args: UpdateArgs, db: &Db) -> Result<()> {
    update_batch_from_args(args, UpdateStrategy::Replace, db)
}
