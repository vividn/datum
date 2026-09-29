//! `datum sync` — Easy Sync. The merge is `datum_core::db::sync` (da-k329q),
//! and here is the pipe it runs over: ops as JSON lines on stdout (`ops`)
//! and stdin (`apply`). Pairing and syncing over iroh is `datum_sync`
//! (da-rugg8), behind the `net` feature.

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
            for line in db.sync_ops_since(&since, true)? {
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
        SyncCommand::Rekey => {
            println!("new endpoint key {}", db.sync_rekey()?);
            println!("peers paired with the old key must pair with this copy again");
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
        #[cfg(feature = "net")]
        SyncCommand::AddClient { qr, expires_secs } => {
            datum_sync::add_client(db, std::time::Duration::from_secs(expires_secs), qr)?
        }
        #[cfg(feature = "net")]
        SyncCommand::Connect { ticket } => {
            let ticket = match ticket {
                Some(t) => t,
                None => {
                    let mut t = String::new();
                    std::io::stdin().read_to_string(&mut t)?;
                    t
                }
            };
            datum_sync::connect(db, &ticket)?
        }
        #[cfg(feature = "net")]
        SyncCommand::Now => datum_sync::now_all(db)?,
        #[cfg(feature = "net")]
        SyncCommand::Run => datum_sync::run(db)?,
        #[cfg(feature = "net")]
        SyncCommand::Peers => datum_sync::list(db)?,
        #[cfg(feature = "net")]
        SyncCommand::Forget { peer } => datum_sync::forget(db, &peer)?,
        #[cfg(not(feature = "net"))]
        _ => anyhow::bail!("peer-to-peer sync is not in this build (no networking)"),
    }
    Ok(())
}

/// The new-peer warning (da-rugg8): once, on the first command after a peer
/// is added, since a peer can read and write everything.
pub fn warn_new_peers(db: &Db) {
    let Ok(mut stmt) = db
        .conn()
        .prepare("SELECT label, endpoint_id, added_at FROM sync_peers WHERE announced = 0")
    else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    }) else {
        return;
    };
    let mut any = false;
    for (label, id, added) in rows.flatten() {
        any = true;
        eprintln!(
            "⚠ new sync peer {label} ({}) added {added} — datum sync peers to review",
            &id[..id.len().min(10)]
        );
    }
    if any {
        let _ = db.conn().execute(
            "UPDATE sync_peers SET announced = 1 WHERE announced = 0",
            [],
        );
    }
}
