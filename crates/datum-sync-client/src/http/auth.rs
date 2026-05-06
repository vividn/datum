//! Client side of `/v1/auth/refresh`.
//!
//! Trades a single-use refresh token for a fresh `(bearer, refresh_token)`
//! pair. The endpoint is unauthenticated — the refresh token IS the
//! credential — so we don't pass a bearer here.
//!
//! Failure modes the caller must distinguish:
//! - 401: refresh token rejected (expired, already-rotated, never-issued).
//!   Surfaces as [`HttpError::Unauthorized`]; the caller should bail with
//!   "session expired — run `datum sync login` again".
//! - Anything else: transient — surfaces as `HttpError::Other` and
//!   should NOT trigger a re-login prompt on its own.

use anyhow::Context;
use reqwest::blocking::Client;

use crate::http::error::HttpError;
use crate::protocol::messages::AuthToken;
use datum_sync_protocol::messages::auth::RefreshTokenRequest;

/// POST `/v1/auth/refresh` with the supplied single-use refresh token.
/// Returns the new [`AuthToken`] (which itself carries a fresh
/// `refresh_token` for the next rotation).
pub fn refresh(
    client: &Client,
    server: &str,
    refresh_token: &str,
) -> Result<AuthToken, HttpError> {
    let body = RefreshTokenRequest {
        refresh_token: refresh_token.to_string(),
    };
    let resp = client
        .post(format!("{server}/v1/auth/refresh"))
        .json(&body)
        .send()
        .context("POST /v1/auth/refresh")
        .map_err(HttpError::other)?;
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(HttpError::Unauthorized);
    }
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        return Err(HttpError::other(anyhow::anyhow!(
            "refresh failed: {status} {text}"
        )));
    }
    let mut token: AuthToken = resp
        .json()
        .context("parse /v1/auth/refresh response")
        .map_err(HttpError::other)?;
    // Server omits `server` (it doesn't know what URL the client
    // dialed); set it here so the persisted file stays consistent.
    if token.server.is_none() {
        token.server = Some(server.to_string());
    }
    if token.refreshed_at.is_none() {
        token.refreshed_at = Some(chrono::Utc::now().to_rfc3339());
    }
    Ok(token)
}
