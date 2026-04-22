//! RFC 8628 device-authorization grant against the datum-sync-server relay.
//!
//! Flow:
//!   1. POST /v1/auth/device-code → `{device_code, user_code, verification_uri, interval}`
//!   2. Show `verification_uri` + `user_code` to the user.
//!   3. Poll GET /v1/auth/device-code/poll?device_code=... every `interval`s.
//!   4. On 200 with a `token` field: return the [`AuthToken`].
//!
//! Transient poll responses (`pending`, `slow_down`) are handled per spec.
//! Persistence is the caller's job — the CLI writes to
//! `$XDG_CONFIG_HOME/datum/auth/<host>.json` with mode 0600; an agent or
//! WASM client might stash the token elsewhere.

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use std::time::Duration;

use crate::protocol::messages::{AuthToken, DeviceCodeResponse, PollResponse};

/// Floor on the poll interval — even if the relay asks for 0s, we wait at
/// least this long to avoid a hot loop.
const POLL_INTERVAL_FLOOR: Duration = Duration::from_millis(50);

/// Cap on total polling time. 10 minutes matches the spec's default
/// `expires_in` (600s).
const POLL_TIMEOUT: Duration = Duration::from_secs(600);

/// Interactive I/O abstraction for the device-flow. Production wires a
/// stdout/stderr prompter; tests pass in a scripted one to keep flows
/// hermetic.
pub trait DeviceFlowPrompter {
    fn show_login_instructions(&mut self, uri: &str, user_code: &str);
    fn status(&mut self, msg: &str);
    fn sleep(&mut self, dur: Duration) {
        std::thread::sleep(dur);
    }
}

/// Run the full device-code flow against `server` using `client` for HTTP
/// and `prompter` for user-facing side effects. Returns the minted token
/// on success.
pub fn run_device_flow<P: DeviceFlowPrompter + ?Sized>(
    server: &str,
    client: &Client,
    prompter: &mut P,
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
                    refreshed_at: chrono::Utc::now().to_rfc3339(),
                    server: server.to_string(),
                    user,
                };
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
