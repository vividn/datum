//! Filesystem watcher for one sync target.
//!
//! Watches the target DB's **parent directory** (non-recursive) and
//! filters events down to the DB file and its SQLite sidecars
//! (`-wal`, `-shm`, `-journal`). Watching the parent rather than the
//! file itself gives one uniform behaviour for every lifecycle state:
//!
//! - the DB doesn't exist yet (lazy-create) — picked up on create,
//! - the DB is deleted and recreated — no re-targeting needed,
//! - writes land in the WAL sidecar rather than the main file — still
//!   observed.
//!
//! Without the filter, every sibling's activity in a shared data dir
//! (other DBs' WAL checkpoints, temp files) would wake every worker.

use std::path::Path;

use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

/// Spawn a `notify` watcher that forwards every meaningful write event
/// for the DB at `path` as a `()` on `tx`. Keeping the returned watcher
/// alive for the lifetime of the worker is important — dropping it
/// stops the callback.
pub fn spawn_watcher(path: &Path, tx: mpsc::UnboundedSender<()>) -> Result<RecommendedWatcher> {
    let db_file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .with_context(|| format!("DB path has no file name: {}", path.display()))?
        .to_string();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any
            ) && event
                .paths
                .iter()
                .any(|p| is_target_write(p, &db_file_name))
            {
                let _ = tx.send(());
            }
        }
    })?;

    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    watcher
        .watch(parent, RecursiveMode::NonRecursive)
        .with_context(|| format!("watch {}", parent.display()))?;
    Ok(watcher)
}

/// True when `event_path` is a write to the target DB: the file itself
/// or one of its SQLite sidecars. The watch is non-recursive on the
/// DB's parent dir, so every event already comes from that dir and
/// comparing file names is sufficient (and avoids symlink-canonicalization
/// mismatches in the paths `notify` reports).
fn is_target_write(event_path: &Path, db_file_name: &str) -> bool {
    event_path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|name| name.strip_prefix(db_file_name))
        .is_some_and(|rest| matches!(rest, "" | "-wal" | "-shm" | "-journal"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn filter_matches_db_and_sidecars_only() {
        let db = "datum.db";
        for hit in [
            "datum.db",
            "datum.db-wal",
            "datum.db-shm",
            "datum.db-journal",
        ] {
            assert!(is_target_write(Path::new(hit), db), "{hit} should match");
        }
        for miss in [
            "other.db",
            "other.db-wal",
            "datum.db2",        // prefix but not a sidecar
            "datum.db-wal.bak", // sidecar prefix but not a sidecar
            "device_id",
        ] {
            assert!(
                !is_target_write(Path::new(miss), db),
                "{miss} should not match"
            );
        }
    }

    /// The bead's end-to-end case: a configured-but-absent DB is picked
    /// up when it's finally created, and a sibling DB's writes in the
    /// same dir do NOT wake this target's worker.
    #[tokio::test]
    async fn absent_db_picked_up_on_create_and_siblings_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("mine.db");
        assert!(!db_path.exists());

        let (tx, mut rx) = mpsc::unbounded_channel();
        let _watcher = spawn_watcher(&db_path, tx).unwrap();

        // A sibling write must not produce a tick.
        std::fs::write(dir.path().join("sibling.db"), b"x").unwrap();
        let got = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await;
        assert!(got.is_err(), "sibling write produced a spurious tick");

        // Creating the (previously absent) target file must.
        std::fs::write(&db_path, b"y").unwrap();
        tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("no tick after target DB was created")
            .expect("watcher channel closed");
    }
}
