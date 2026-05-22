//! Cluster-control endpoints. Pairs with `datum-sync-server::cluster`
//! (da-tat1) for the `POST /v1/clusters/{cluster_id}/bump-generation`
//! handler that signals a purge / restore / rewind to the relay.
//!
//! Gated by the `http` cargo feature because it depends on `reqwest`.

use anyhow::Context;
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::http::error::HttpError;

/// Response body for `POST /v1/clusters/{cluster_id}/bump-generation`.
/// Mirrors the server's `cluster::BumpResponse` field-for-field — kept
/// private to this module so callers receive just the new generation.
#[derive(Debug, Deserialize)]
struct BumpResponse {
    #[allow(dead_code)] // echoed back for client-side correlation; we already have it
    cluster_id: String,
    generation: i64,
}

/// Bump the cluster's generation by one and return the new value. Used
/// by `datum db purge --cluster` / `datum db restore --cluster` to fork
/// the cluster so other devices on the old generation get a
/// [`crate::http::HttpError::GenerationMismatch`] on their next sync
/// and know to re-init.
///
/// 404 surfaces as a typed [`HttpError::Other`] — the bump endpoint
/// returns 404 for "cluster doesn't exist OR is owned by another user"
/// (the two are indistinguishable by design — no cross-owner cluster
/// enumeration; da-tat1 `bump_generation_endpoint`).
pub fn bump_generation(
    client: &Client,
    server: &str,
    bearer: &str,
    cluster_id: &str,
) -> Result<u64, HttpError> {
    let resp = client
        .post(format!("{server}/v1/clusters/{cluster_id}/bump-generation"))
        .bearer_auth(bearer)
        .send()
        .context("POST /v1/clusters/{cluster}/bump-generation")
        .map_err(HttpError::other)?;
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(HttpError::Unauthorized);
    }
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(HttpError::other(anyhow::anyhow!(
            "bump-generation failed: {status} {body}"
        )));
    }
    let parsed: BumpResponse = resp
        .json()
        .context("parse bump-generation response")
        .map_err(HttpError::other)?;
    if parsed.generation < 0 {
        return Err(HttpError::other(anyhow::anyhow!(
            "relay returned negative generation: {}",
            parsed.generation
        )));
    }
    Ok(parsed.generation as u64)
}
