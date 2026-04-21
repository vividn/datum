//! `datum sync login` — RFC 8628 device-authorization grant.
//!
//! See `docs/sync-protocol.md` for the wire contract. Flow:
//!   1. POST /v1/auth/device-code → `{device_code, user_code, verification_uri, interval}`
//!   2. Show `verification_uri` + `user_code` to the user.
//!   3. Poll GET /v1/auth/device-code/poll?device_code=... every `interval`s.
//!   4. On 200 with a `token` field: persist to
//!      `$XDG_CONFIG_HOME/datum/auth/<server-host>.json`.
//!
//! Transient poll responses (`pending`, `slow_down`) are handled per spec.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::args::SyncLoginArgs;
use crate::config::{auth_token_path, load_raw_config_or_default, server_host_from_url};
use crate::sync::{AuthToken, Prompter};

/// Floor on the poll interval — even if the relay asks for 0s, we wait at
/// least this long to avoid a hot loop.
const POLL_INTERVAL_FLOOR: Duration = Duration::from_millis(50);

/// Cap on total polling time. 10 minutes matches the spec's default
/// `expires_in` (600s).
const POLL_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    #[allow(dead_code)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    expires_in: Option<u64>,
    /// Poll interval in seconds. RFC 8628 default is 5.
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
struct PollResponse {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    detail: Option<String>,
}

/// Production entry point.
pub fn run(args: SyncLoginArgs, prompter: &mut impl Prompter) -> Result<()> {
    let server = resolve_server(args.server.as_deref())?;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build reqwest client")?;
    run_with_client(&server, &client, prompter)?;
    Ok(())
}

/// Split from `run` so tests can inject a client pointed at a stub relay.
pub fn run_with_client(
    server: &str,
    client: &Client,
    prompter: &mut impl Prompter,
) -> Result<AuthToken> {
    let start = client.post(format!("{server}/v1/auth/device-code"));
    let resp = start
        .json(&serde_json::json!({}))
        .send()
        .context("POST /v1/auth/device-code")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        anyhow::bail!("device-code request failed: {status} {body}");
    }
    let device: DeviceCodeResponse = resp
        .json()
        .context("parse /v1/auth/device-code response")?;

    prompter.show_login_instructions(&device.verification_uri, &device.user_code);

    let mut interval = Duration::from_secs(device.interval.max(1));
    let deadline = std::time::Instant::now() + POLL_TIMEOUT;

    let poll_url = format!("{server}/v1/auth/device-code/poll");
    loop {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("login timed out waiting for authorization");
        }
        prompter.sleep(interval.max(POLL_INTERVAL_FLOOR));

        let resp = client
            .get(&poll_url)
            .query(&[("device_code", device.device_code.as_str())])
            .send()
            .context("GET /v1/auth/device-code/poll")?;

        let status = resp.status();
        let payload: PollResponse = resp
            .json()
            .context("parse device-code/poll response")?;

        // Slow-down hint — the relay asks us to back off. Per RFC 8628 §3.5,
        // we double the interval and keep polling.
        if payload.error.as_deref() == Some("slow_down") {
            interval = interval.saturating_mul(2);
            prompter.status("slow_down — backing off");
            continue;
        }

        // Expired code: unrecoverable, bubble up and let the user retry.
        if payload.error.as_deref() == Some("expired") {
            anyhow::bail!("device code expired — run `datum sync login` again");
        }

        // Still pending — keep polling at the current interval.
        if payload.status.as_deref() == Some("pending") {
            continue;
        }

        // A non-202 status with a token field means success.
        if status.is_success() {
            if let Some(tok) = payload.token {
                let user = payload.user_id.clone();
                let token = AuthToken {
                    token: tok,
                    refreshed_at: Utc::now().to_rfc3339(),
                    server: server.to_string(),
                    user,
                };
                write_token(server, &token)?;
                let label = token.user.as_deref().unwrap_or("user");
                prompter.status(&format!("✓ Authorized as {label}"));
                return Ok(token);
            }
        }

        // Any other shape is treated as a transient error we keep polling
        // through — except known terminal errors handled above.
        if let Some(err) = payload.error.as_deref() {
            anyhow::bail!("device-code/poll returned error: {err}");
        }
    }
}

/// Resolve the relay URL: explicit `--server` wins, otherwise YAML config.
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

/// Persist the token atomically under `auth_token_path(host)`.
fn write_token(server: &str, token: &AuthToken) -> Result<()> {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host)?;
    let body = serde_json::to_vec_pretty(token).context("serialize auth token")?;
    // Write with 0600 mode so other users on the box can't read the token.
    write_private_file(&path, &body)?;
    Ok(())
}

fn write_private_file(path: &std::path::Path, body: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("open {path:?}"))?;
        std::io::Write::write_all(&mut f, body)?;
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, body).with_context(|| format!("write {path:?}"))?;
        Ok(())
    }
}

/// Load a previously-saved token for `server`. Returns `None` if missing.
#[allow(dead_code)]
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

