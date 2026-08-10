//! `datum sync key` — CLI-side orchestration for the master-key lifecycle.
//!
//! Crypto lives in `datum-sync-client::crypto::key_wrap`; HTTP calls live
//! in `datum-sync-client::http::keys`. This file owns the CLI concerns:
//! which server to talk to, where the local key file lives, and how to
//! prompt the user.
//!
//! Subcommands:
//! - `init`: generate a fresh 32-byte master key, wrap it with a passphrase,
//!   upload the wrapped blob to the relay, and persist the plaintext key
//!   locally (mode 0600).
//! - `unlock`: download the wrapped blob from the relay, prompt for the
//!   passphrase, write the unwrapped key to the local key file.
//! - `rotate-passphrase`: re-wrap the existing local key with a new
//!   passphrase and re-upload.
//! - `show-fingerprint`: print the SHA-256 hex of the local key so two
//!   devices can eyeball-verify parity.

use anyhow::{anyhow, Context, Result};
use datum_sync_client::crypto::key_wrap::{
    key_fingerprint, unwrap_master_key, wrap_master_key, Zeroizing,
};
use datum_sync_client::http::{fetch_wrapped, upload_wrapped};
use rand::RngCore;
use reqwest::blocking::Client;

use crate::commands::sync::{build_client, resolve_server, AuthToken, Prompter};
use crate::config::{auth_token_path, master_key_path, server_host_from_url};
use datum_sync_client::PersistedAuthToken;

/// `datum sync key init`
///
/// `cli_db` is the global `--db` flag — it beats `DATUM_DB` when naming
/// the active DB for per-DB server resolution (da-cyn4).
pub fn init(
    server_override: Option<String>,
    force: bool,
    cli_db: Option<&str>,
    prompter: &mut impl Prompter,
) -> Result<()> {
    let server = resolve_server(server_override.as_deref(), cli_db)?;
    let client = build_client()?;
    init_with_client(&server, &client, force, prompter)?;
    Ok(())
}

/// Test hook — same as `init` but with an injectable client.
pub fn init_with_client(
    server: &str,
    client: &Client,
    force: bool,
    prompter: &mut impl Prompter,
) -> Result<[u8; 32]> {
    let host = server_host_from_url(server);
    let key_path = master_key_path(&host)?;
    if key_path.exists() && !force {
        anyhow::bail!(
            "local key already exists at {}; pass --force to overwrite",
            key_path.display()
        );
    }

    let passphrase = prompter.read_passphrase("Set passphrase: ", true)?;
    if passphrase.is_empty() {
        anyhow::bail!("passphrase cannot be empty");
    }

    let mut master = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut master);
    let wrapped = wrap_master_key(&passphrase, &master)?;

    let token = crate::commands::sync::require_token(server)?;
    let session = crate::commands::sync::build_session(server, &token);
    session.run(client, |bearer| {
        upload_wrapped(client, server, bearer, &wrapped)
    })?;

    write_key_file(&key_path, &master)?;
    prompter.status(&format!(
        "✓ Master key initialized — fingerprint: {}",
        key_fingerprint(&master)
    ));
    Ok(master)
}

/// `datum sync key unlock`
pub fn unlock(
    server_override: Option<String>,
    cli_db: Option<&str>,
    prompter: &mut impl Prompter,
) -> Result<()> {
    let server = resolve_server(server_override.as_deref(), cli_db)?;
    let client = build_client()?;
    unlock_with_client(&server, &client, prompter)?;
    Ok(())
}

pub fn unlock_with_client(
    server: &str,
    client: &Client,
    prompter: &mut impl Prompter,
) -> Result<Zeroizing<[u8; 32]>> {
    let host = server_host_from_url(server);
    let key_path = master_key_path(&host)?;
    let token = crate::commands::sync::require_token(server)?;
    let session = crate::commands::sync::build_session(server, &token);
    let wrapped = session
        .run(client, |bearer| fetch_wrapped(client, server, bearer))?
        .ok_or_else(|| anyhow!("no wrapped key found on server — run `datum sync key init`"))?;
    let passphrase = prompter.read_passphrase("Passphrase: ", false)?;
    let master = unwrap_master_key(&passphrase, &wrapped)
        .map_err(|_| anyhow!("unlock failed — wrong passphrase or tampered key"))?;
    write_key_file(&key_path, &master)?;
    prompter.status(&format!(
        "✓ Master key unlocked — fingerprint: {}",
        key_fingerprint(&master)
    ));
    Ok(master)
}

/// `datum sync key rotate-passphrase`
pub fn rotate_passphrase(
    server_override: Option<String>,
    cli_db: Option<&str>,
    prompter: &mut impl Prompter,
) -> Result<()> {
    let server = resolve_server(server_override.as_deref(), cli_db)?;
    let client = build_client()?;
    rotate_passphrase_with_client(&server, &client, prompter)?;
    Ok(())
}

pub fn rotate_passphrase_with_client(
    server: &str,
    client: &Client,
    prompter: &mut impl Prompter,
) -> Result<()> {
    let host = server_host_from_url(server);
    let key_path = master_key_path(&host)?;
    let master = read_key_file(&key_path)
        .context("read local master key — run `datum sync key unlock` first")?;
    let new_pass = prompter.read_passphrase("New passphrase: ", true)?;
    if new_pass.is_empty() {
        anyhow::bail!("passphrase cannot be empty");
    }
    let wrapped = wrap_master_key(&new_pass, &master)?;
    let token = crate::commands::sync::require_token(server)?;
    let session = crate::commands::sync::build_session(server, &token);
    session.run(client, |bearer| {
        upload_wrapped(client, server, bearer, &wrapped)
    })?;
    prompter.status("✓ Passphrase rotated — server copy re-wrapped");
    Ok(())
}

/// `datum sync key show-fingerprint`
pub fn show_fingerprint(server_override: Option<String>, cli_db: Option<&str>) -> Result<()> {
    let server = resolve_server(server_override.as_deref(), cli_db)?;
    let host = server_host_from_url(&server);
    let key_path = master_key_path(&host)?;
    let master = read_key_file(&key_path)
        .with_context(|| format!("read master key at {}", key_path.display()))?;
    println!("{}", key_fingerprint(&master));
    Ok(())
}

// --- Shared helpers --------------------------------------------------------

/// Load the auth token for `server`; returns `Ok(None)` if there's no
/// file. da-ixmh: returns the on-disk [`PersistedAuthToken`] shape;
/// callers that need the bearer / refresh / user_id read through the
/// `Deref<Target = AuthToken>` impl.
pub fn load_token(server: &str) -> Result<Option<PersistedAuthToken>> {
    // Bootstrap / test override: a bearer in DATUM_SYNC_TOKEN is used
    // directly, bypassing the on-disk login token. This lets a relay admin
    // mint a credential (`datum-sync-server mint-token <user>`) and hand it
    // to a client with no login flow — the supported path for standing up
    // multi-client sync while the browser verification UI is unbuilt
    // (da-1am4). DATUM_SYNC_REFRESH_TOKEN is optional; a fresh 30-day bearer
    // needs no refresh for a session.
    if let Ok(bearer) = std::env::var("DATUM_SYNC_TOKEN") {
        let bearer = bearer.trim();
        if !bearer.is_empty() {
            let wire = AuthToken {
                token: bearer.to_string(),
                user_id: String::new(),
                refresh_token: std::env::var("DATUM_SYNC_REFRESH_TOKEN").unwrap_or_default(),
                cluster_id: None,
            };
            return Ok(Some(PersistedAuthToken::from_wire(wire, server)));
        }
    }

    let host = server_host_from_url(server);
    let path = auth_token_path(&host)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read token at {path:?}"))?;
    let token: PersistedAuthToken =
        serde_json::from_slice(&bytes).context("parse stored auth token")?;
    Ok(Some(token))
}

fn write_key_file(path: &std::path::Path, key: &[u8; 32]) -> Result<()> {
    crate::config::write_atomic(path, key, 0o600).with_context(|| format!("write {path:?}"))
}

fn read_key_file(path: &std::path::Path) -> Result<[u8; 32]> {
    let bytes = std::fs::read(path).with_context(|| format!("read key at {path:?}"))?;
    if bytes.len() != 32 {
        anyhow::bail!(
            "master key at {path:?} must be 32 bytes, got {}",
            bytes.len()
        );
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}
