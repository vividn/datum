//! Configuration: locate the database file and device ID.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Return the path to the datum SQLite database, creating directories if needed.
///
/// Resolution order:
///   1. DATUM_DB env var (explicit file path)
///   2. $DATUM_HOME/datum.db (custom home directory)
///   3. Default: $XDG_DATA_HOME/datum/datum.db (via dirs-next)
pub fn db_path() -> Result<String> {
    // 1. Explicit DB path
    if let Ok(p) = std::env::var("DATUM_DB") {
        if let Some(parent) = std::path::Path::new(&p).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Creating directory for DATUM_DB: {parent:?}"))?;
            }
        }
        return Ok(p);
    }
    let path = data_dir()?.join("datum.db");
    Ok(path.to_string_lossy().into_owned())
}

/// Return (or create) the device ID stored in the data directory.
pub fn device_id() -> Result<String> {
    let path = data_dir()?.join("device_id");
    if path.exists() {
        let id = std::fs::read_to_string(&path)
            .with_context(|| format!("Reading device_id from {path:?}"))?;
        return Ok(id.trim().to_string());
    }
    // Generate a new device ID on first run.
    let id = ulid::Ulid::new().to_string();
    std::fs::write(&path, &id)
        .with_context(|| format!("Writing device_id to {path:?}"))?;
    Ok(id)
}

fn data_dir() -> Result<PathBuf> {
    // DATUM_HOME overrides the default data directory
    if let Ok(home) = std::env::var("DATUM_HOME") {
        let dir = PathBuf::from(home);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Creating DATUM_HOME directory {dir:?}"))?;
        return Ok(dir);
    }
    let base = dirs_next::data_dir()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
        .context("Cannot determine data directory")?;
    let dir = base.join("datum");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Creating data directory {dir:?}"))?;
    Ok(dir)
}
