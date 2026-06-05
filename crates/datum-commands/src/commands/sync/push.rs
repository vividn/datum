//! `datum sync [--push]` — thin CLI-side wrapper around
//! [`datum_sync_client::sync_once::push_once`].
//!
//! All push logic — packing changesets, advancing watermarks, encrypting
//! and POSTing — lives in `datum-sync-client` so the daemon
//! (`datum-sync-agent`) and any future WASM/mobile client can reuse it
//! without dragging in clap or the CLI surface.
//!
//! This module re-exports [`PushOutcome`] from the client library so the
//! existing `datum_commands::commands::sync::push::PushOutcome` symbol
//! keeps resolving for downstream callers.

use anyhow::Result;
use datum_core::db::Db;
use datum_sync_client::http::Session;
use datum_sync_client::sync_once;
use reqwest::blocking::Client;

pub use datum_sync_client::sync_once::{PushOutcome, SeqRebase};

/// Pack changes since the last push watermark, encrypt, and upload. A
/// `dry_run` packs and counts but skips both the HTTP call and the
/// watermark advance.
pub fn run(
    server: &str,
    client: &Client,
    session: &Session,
    device_id: &str,
    master_key: &[u8; 32],
    db: &Db,
    dry_run: bool,
) -> Result<PushOutcome> {
    sync_once::push_once(server, client, session, device_id, master_key, db, dry_run)
}
