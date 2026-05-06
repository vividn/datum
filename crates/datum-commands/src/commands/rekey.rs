//! `datum rekey <id_or_slug>` — change an entry's slug via id-template
//! re-interpolation (moves its slug; preserves the old row).
//!
//! Thin wrapper over [`crate::commands::update_core::update_with_strategy`]
//! with `UpdateStrategy::Rekey`.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::UpdateArgs;
use crate::commands::update_core::{update_batch_from_args, UpdateStrategy};

pub fn rekey(args: UpdateArgs, db: &Db) -> Result<()> {
    update_batch_from_args(args, UpdateStrategy::Rekey, db)
}
