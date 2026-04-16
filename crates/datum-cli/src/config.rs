//! Configuration: locate the database file and device ID.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Return the path to the datum SQLite database, creating directories if needed.
pub fn db_path() -> Result<String> {
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
    let base = dirs_next::data_dir()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
        .context("Cannot determine data directory")?;
    let dir = base.join("datum");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Creating data directory {dir:?}"))?;
    Ok(dir)
}
