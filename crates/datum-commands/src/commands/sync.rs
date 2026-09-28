//! `datum sync` — Easy Sync's core (da-k329q). The merge is
//! `datum_core::db::sync`; this is the pipe it runs over: ops as JSON lines
//! on stdout (`ops`) and stdin (`apply`).

use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{Context, Result};
use datum_core::db::Db;

use crate::args::SyncCommand;

pub fn sync(cmd: SyncCommand, db: &Db) -> Result<()> {
    match cmd {
        SyncCommand::Vector => {
            println!("{}", serde_json::to_string(&db.sync_vector()?)?);
        }
        SyncCommand::Ops { since } => {
            let since: BTreeMap<String, i64> = match since {
                Some(v) => serde_json::from_str(&v)
                    .with_context(|| format!("--since is not a version vector: {v}"))?,
                None => BTreeMap::new(),
            };
            for line in db.sync_ops_since(&since)? {
                println!("{line}");
            }
        }
        SyncCommand::Apply => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input)?;
            let s = db.sync_apply(&input)?;
            for why in &s.refused {
                eprintln!("refused: {why}");
            }
            println!(
                "applied {} · already had {} · held {} · conflicts {}",
                s.applied, s.already_had, s.held, s.conflicts
            );
            if !s.refused.is_empty() {
                anyhow::bail!("{} op(s) refused", s.refused.len());
            }
        }
        SyncCommand::Verify { verbose } => {
            let (hash, per_column) = db.sync_state_hash()?;
            println!("{hash}");
            if verbose {
                for (column, hash) in per_column {
                    println!("  {column:<24} {hash}");
                }
            }
        }
    }
    Ok(())
}
