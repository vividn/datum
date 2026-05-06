//! `datum head` — list the oldest entries.
//!
//! Thin dispatch wrapper: forces `--oldest-first` and routes through
//! `cmd_tail` so head and tail share one rendering / filtering codepath.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::TailArgs;

pub fn head(mut args: TailArgs, db: &Db) -> Result<()> {
    args.oldest_first = true;
    crate::commands::tail::tail(args, db)
}
