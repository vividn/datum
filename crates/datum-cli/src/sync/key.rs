//! `datum sync key` — master-key management.
//!
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
use datum_core::sync::{
    key_fingerprint, unwrap_master_key, wrap_master_key, WrappedKey,
};
use rand::RngCore;
use reqwest::blocking::Client;
use std::time::Duration;

use crate::config::{
    auth_token_path, load_raw_config_or_default, master_key_path, server_host_from_url,
};
use crate::sync::{AuthToken, Prompter};

/// `datum sync key init`
pub fn init(
    server_override: Option<String>,
    force: bool,
    prompter: &mut impl Prompter,
) -> Result<()> {
    let server = resolve_server(server_override.as_deref())?;
    let client = default_client()?;
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

    let token = load_token(server)?
        .ok_or_else(|| anyhow!("not logged in — run `datum sync login` first"))?;
    upload_wrapped(client, server, &token.token, &wrapped)?;

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
    prompter: &mut impl Prompter,
) -> Result<()> {
    let server = resolve_server(server_override.as_deref())?;
    let client = default_client()?;
    unlock_with_client(&server, &client, prompter)?;
    Ok(())
}

pub fn unlock_with_client(
    server: &str,
    client: &Client,
    prompter: &mut impl Prompter,
) -> Result<[u8; 32]> {
    let host = server_host_from_url(server);
    let key_path = master_key_path(&host)?;
    let token = load_token(server)?
        .ok_or_else(|| anyhow!("not logged in — run `datum sync login` first"))?;
    let wrapped = fetch_wrapped(client, server, &token.token)?
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
    prompter: &mut impl Prompter,
) -> Result<()> {
    let server = resolve_server(server_override.as_deref())?;
    let client = default_client()?;
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
    let token = load_token(server)?
        .ok_or_else(|| anyhow!("not logged in — run `datum sync login` first"))?;
    upload_wrapped(client, server, &token.token, &wrapped)?;
    prompter.status("✓ Passphrase rotated — server copy re-wrapped");
    Ok(())
}

/// `datum sync key show-fingerprint`
pub fn show_fingerprint(server_override: Option<String>) -> Result<()> {
    let server = resolve_server(server_override.as_deref())?;
    let host = server_host_from_url(&server);
    let key_path = master_key_path(&host)?;
    let master = read_key_file(&key_path)
        .with_context(|| format!("read master key at {}", key_path.display()))?;
    println!("{}", key_fingerprint(&master));
    Ok(())
}

// --- Shared helpers --------------------------------------------------------

fn default_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build reqwest client")
}

fn resolve_server(cli_server: Option<&str>) -> Result<String> {
    if let Some(s) = cli_server {
        return Ok(s.to_string());
    }
    let cfg = load_raw_config_or_default()?;
    cfg.sync
        .as_ref()
        .and_then(|s| s.default_server.clone())
        .filter(|v| !v.eq_ignore_ascii_case("none"))
        .ok_or_else(|| {
            anyhow!(
                "no sync server configured; pass --server <url> or run \
                 `datum sync config set-server <url> --global`"
            )
        })
}

/// Load the auth token for `server`; returns `Ok(None)` if there's no file.
pub fn load_token(server: &str) -> Result<Option<AuthToken>> {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host)?;
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("read token at {path:?}"))?;
    let token: AuthToken =
        serde_json::from_slice(&bytes).context("parse stored auth token")?;
    Ok(Some(token))
}

fn upload_wrapped(
    client: &Client,
    server: &str,
    bearer: &str,
    wrapped: &WrappedKey,
) -> Result<()> {
    let resp = client
        .post(format!("{server}/v1/keys/wrapped"))
        .bearer_auth(bearer)
        .json(wrapped)
        .send()
        .context("POST /v1/keys/wrapped")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        anyhow::bail!("wrapped-key upload failed: {status} {body}");
    }
    Ok(())
}

fn fetch_wrapped(
    client: &Client,
    server: &str,
    bearer: &str,
) -> Result<Option<WrappedKey>> {
    let resp = client
        .get(format!("{server}/v1/keys/wrapped"))
        .bearer_auth(bearer)
        .send()
        .context("GET /v1/keys/wrapped")?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        anyhow::bail!("wrapped-key fetch failed: {status} {body}");
    }
    let w: WrappedKey = resp.json().context("parse wrapped-key response")?;
    Ok(Some(w))
}

fn write_key_file(path: &std::path::Path, key: &[u8; 32]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create {parent:?}"))?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("open {path:?}"))?;
        f.write_all(key)?;
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, key).with_context(|| format!("write {path:?}"))?;
        Ok(())
    }
}

fn read_key_file(path: &std::path::Path) -> Result<[u8; 32]> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("read key at {path:?}"))?;
    if bytes.len() != 32 {
        anyhow::bail!("master key at {path:?} must be 32 bytes, got {}", bytes.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}
