//! `datum sync config` — read/write the sync section of `datumrc.toml`.
//!
//! Per-DB overrides live under `[databases.<name>].sync_server`; the global
//! fallback lives under `[sync].default_server`. `disable` sets the chosen
//! scope's value to the literal string `"none"`, which is treated as "sync
//! off" by [`crate::config::resolve_sync_server`].

use anyhow::{Context, Result};

use crate::config::{
    self, load_raw_config_or_default, resolve_sync_server, save_raw_config,
};

/// `datum sync config set-server <url> [--global]`. The per-DB path reads
/// the active DB name from `--db` or the `DATUM_DB` env var; bare paths
/// can't be set per-DB (they have no name).
pub fn set_server(url: String, global: bool) -> Result<()> {
    let mut cfg = load_raw_config_or_default()?;
    if global {
        let mut sync = cfg.sync.take().unwrap_or_default();
        sync.default_server = Some(url.clone());
        cfg.sync = Some(sync);
    } else {
        let db_name = active_db_name().context(
            "no active DB name; pass --global, or select a named DB via \
             --db <name> / DATUM_DB",
        )?;
        let entry = cfg.databases.entry(db_name).or_default();
        entry.sync_server = Some(url.clone());
    }
    save_raw_config(&cfg)?;
    println!("sync_server set to {url}");
    Ok(())
}

/// `datum sync config disable` — disable sync for the active DB when one is
/// named, otherwise disable globally.
pub fn disable() -> Result<()> {
    let mut cfg = load_raw_config_or_default()?;
    match active_db_name_opt() {
        Some(name) => {
            let entry = cfg.databases.entry(name.clone()).or_default();
            entry.sync_server = Some("none".to_string());
            println!("sync disabled for db '{name}'");
        }
        None => {
            let mut sync = cfg.sync.take().unwrap_or_default();
            sync.default_server = Some("none".to_string());
            cfg.sync = Some(sync);
            println!("sync disabled globally");
        }
    }
    save_raw_config(&cfg)?;
    Ok(())
}

/// `datum sync config show` — print the resolved sync state for the active DB.
pub fn show() -> Result<()> {
    let cfg = load_raw_config_or_default()?;
    let db = active_db_name_opt();
    let resolved = resolve_sync_server(&cfg, db.as_deref());
    let global = cfg
        .sync
        .as_ref()
        .and_then(|s| s.default_server.clone())
        .unwrap_or_else(|| "(unset)".to_string());
    let per_db = db
        .as_deref()
        .and_then(|n| cfg.databases.get(n))
        .and_then(|d| d.sync_server.clone())
        .unwrap_or_else(|| "(unset)".to_string());
    let auto_sync = db
        .as_deref()
        .and_then(|n| cfg.databases.get(n))
        .and_then(|d| d.auto_sync)
        .map(|b| b.to_string())
        .unwrap_or_else(|| "(unset)".to_string());
    println!("active db:         {}", db.as_deref().unwrap_or("(default)"));
    println!("global server:     {global}");
    println!("per-db server:     {per_db}");
    println!("per-db auto_sync:  {auto_sync}");
    println!(
        "effective server:  {}",
        resolved.unwrap_or_else(|| "(none)".to_string())
    );
    Ok(())
}

/// Resolve the active DB name, or an error if we can't determine one.
fn active_db_name() -> Result<String> {
    active_db_name_opt().ok_or_else(|| {
        anyhow::anyhow!(
            "no active DB name — use --db <name> or $DATUM_DB (bare paths \
             have no name, use --global instead)"
        )
    })
}

/// Active DB name without an error — `None` if the active target is a bare
/// path rather than a registered name.
fn active_db_name_opt() -> Option<String> {
    std::env::var("DATUM_DB")
        .ok()
        .filter(|v| !v.is_empty() && !config::looks_like_path(v))
}
