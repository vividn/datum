//! `datum rekey <id_or_slug> old=new` — rename an entry's data keys, in
//! place: `old`'s value moves to the key `new` on the same row, as v2's
//! `rekey` does. The id template is read again afterwards, so the slug
//! follows: a key renamed to one the template names fills that part of the
//! id, and a template's key renamed away empties it.
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
