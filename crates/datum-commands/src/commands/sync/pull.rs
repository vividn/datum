//! `datum sync [--pull]` — thin CLI-side wrapper around
//! [`datum_sync_client::sync_once::pull_once`].
//!
//! All pull logic — fetch / decrypt / LWW-apply / conflict-detect /
//! advance pull watermark — lives in `datum-sync-client` so the daemon
//! and any future WASM/mobile client can drive a pull cycle without
//! pulling in clap or the CLI surface.
//!
//! This module re-exports [`PullOutcome`] from the client library so the
//! existing `datum_commands::commands::sync::pull::PullOutcome` symbol
//! keeps resolving for downstream callers (notably the daemon's
//! `do_pull` return type).

use anyhow::Result;
use datum_core::db::Db;
use datum_sync_client::http::Session;
use datum_sync_client::sync_once;
use reqwest::blocking::Client;

pub use datum_sync_client::sync_once::PullOutcome;

pub fn run(
    server: &str,
    client: &Client,
    session: &Session,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    dry_run: bool,
) -> Result<PullOutcome> {
    sync_once::pull_once(server, client, session, device_id, master_key, db, dry_run)
}
