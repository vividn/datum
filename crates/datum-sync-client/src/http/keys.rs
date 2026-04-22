//! Client-side of `/v1/keys/wrapped` — upload and fetch the
//! passphrase-wrapped master key blob.
//!
//! The server is content-blind about the blob itself (see `docs/sync-protocol.md`);
//! it only stores it keyed by authenticated user.

use anyhow::{Context, Result};
use reqwest::blocking::Client;

use crate::crypto::key_wrap::WrappedKey;

/// POST the wrapped key to the relay. Surfaces the server body on failure
/// so `anyhow` error messages stay actionable.
pub fn upload_wrapped(
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

/// GET the wrapped key for the authenticated user. `Ok(None)` on 404.
pub fn fetch_wrapped(
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
