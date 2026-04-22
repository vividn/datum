//! Enumerate the DBs that the daemon should auto-sync.
//!
//! A DB is a target when both:
//! 1. A sync server URL resolves for it (per-DB override or global default),
//!    and
//! 2. `auto_sync` is not explicitly set to `false` (default is on — the bead
//!    says "Auto-sync is ON when `sync_server` is configured").
//!
//! The enumerator pulls from `datumrc.yml` via `datum-commands::config`;
//! it does NOT inspect the DB registry beyond what the CLI already
//! exposes. A DB that exists on disk but has no `databases:` entry in
//! the yaml is still picked up IF `sync.default_server` is set globally.

use std::path::PathBuf;

use anyhow::Result;
use datum_commands::config::{
    data_dir, load_raw_config_or_default, load_registry, RawConfig,
    resolve_sync_server,
};

/// A DB the agent should watch and sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncTarget {
    /// Registry name; `"_default"` for the unnamed default DB.
    pub name: String,
    /// Absolute path to the sqlite file.
    pub path: PathBuf,
    /// Effective sync server URL.
    pub server: String,
}

const DEFAULT_DB_LABEL: &str = "_default";

/// Enumerate all eligible DBs. Called on startup and whenever the agent
/// is asked to re-scan (SIGHUP, future feature).
pub fn enumerate() -> Result<Vec<SyncTarget>> {
    let cfg = load_raw_config_or_default()?;
    targets_from(&cfg)
}

/// Pure core of [`enumerate`] — takes a `RawConfig` so tests don't have
/// to touch the filesystem.
pub fn targets_from(cfg: &RawConfig) -> Result<Vec<SyncTarget>> {
    let mut out: Vec<SyncTarget> = Vec::new();

    // 1) Named DBs in the `databases:` section.
    for (name, section) in &cfg.databases {
        if section.auto_sync == Some(false) {
            continue;
        }
        let Some(server) = resolve_sync_server(cfg, Some(name)) else {
            continue;
        };
        let path = resolve_db_path(name)?;
        out.push(SyncTarget {
            name: name.clone(),
            path,
            server,
        });
    }

    // 2) The unnamed default DB, if a global sync server is set and the
    //    DB file exists. Without the existence check we'd spawn a
    //    watcher for a nonexistent file.
    if let Some(server) = resolve_sync_server(cfg, None) {
        let path = data_dir()?.join("datum.db");
        if path.exists()
            && !out.iter().any(|t| t.path == path)
        {
            out.push(SyncTarget {
                name: DEFAULT_DB_LABEL.to_string(),
                path,
                server,
            });
        }
    }

    Ok(out)
}

/// Resolve a registered DB name to its absolute path. Falls back to
/// `<data_dir>/<name>.db` when the name isn't in the registry — this
/// matches the CLI's lazy-create behaviour.
fn resolve_db_path(name: &str) -> Result<PathBuf> {
    let registry = load_registry().unwrap_or_default();
    if let Some(entry) = registry.db.get(name) {
        return Ok(PathBuf::from(&entry.path));
    }
    Ok(data_dir()?.join(format!("{name}.db")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_commands::config::{DbSection, SyncSection};

    fn cfg_with_global(server: &str) -> RawConfig {
        RawConfig {
            sync: Some(SyncSection {
                default_server: Some(server.to_string()),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn per_db_sync_picked_up() {
        let mut cfg = RawConfig::default();
        cfg.databases.insert(
            "personal".to_string(),
            DbSection {
                sync_server: Some("https://relay.example/a".to_string()),
                auto_sync: None,
            },
        );
        cfg.databases.insert(
            "scratch".to_string(),
            DbSection {
                sync_server: Some("https://relay.example/b".to_string()),
                auto_sync: Some(false), // explicitly opted out
            },
        );

        let targets = targets_from(&cfg).unwrap();
        let names: Vec<_> = targets.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["personal"]);
        assert_eq!(targets[0].server, "https://relay.example/a");
    }

    #[test]
    fn per_db_none_disables_even_with_global() {
        let mut cfg = cfg_with_global("https://relay.example");
        cfg.databases.insert(
            "personal".to_string(),
            DbSection {
                sync_server: Some("none".to_string()),
                auto_sync: None,
            },
        );
        let targets = targets_from(&cfg).unwrap();
        // "personal" explicitly disabled, default DB picked up only if
        // its file exists — in a clean test env it may or may not, but
        // "personal" definitely shouldn't appear.
        assert!(targets.iter().all(|t| t.name != "personal"));
    }

    #[test]
    fn no_config_no_targets() {
        let cfg = RawConfig::default();
        let targets = targets_from(&cfg).unwrap();
        assert!(targets.is_empty());
    }
}
