//! `datum config` — inspect per-db and global configuration.
//!
//! Named `commands::config` to avoid colliding with the crate-level
//! `crate::config` module that holds XDG-path / registry helpers.

use anyhow::Result;

use crate::args::ConfigCommand;

pub fn config(sub: ConfigCommand, cli_db: Option<&str>) -> Result<()> {
    crate::cmd::cmd_config(sub, cli_db)
}

pub fn config_from_argv(_argv: &[String]) -> Result<()> {
    anyhow::bail!("config_from_argv: stub — R2 extracts bespoke parser")
}

pub fn config_from_string(_input: &str) -> Result<()> {
    anyhow::bail!("config_from_string: stub — R2 extracts bespoke parser")
}
