//! Client side of `/v0/auth/refresh`.
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

/// POST `/v0/auth/refresh` with the supplied single-use refresh token.
/// Returns the new [`AuthToken`] (which itself carries a fresh
/// `refresh_token` for the next rotation).
pub fn refresh(client: &Client, server: &str, refresh_token: &str) -> Result<AuthToken, HttpError> {
    let body = RefreshTokenRequest {
        refresh_token: refresh_token.to_string(),
    };
    let resp = client
        .post(format!("{server}/v0/auth/refresh"))
        .json(&body)
        .send()
        .context("POST /v0/auth/refresh")
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
    // da-ixmh: the wire shape is the wire shape — the `server` /
    // `refreshed_at` stamping that used to happen here moved to the
    // CLI's persistence layer (`PersistedAuthToken::from_wire`). HTTP
    // returns exactly what came off the wire; nobody downstream has to
    // wonder whether they're looking at a server response or a partly-
    // hydrated persistence shape.
    let _ = server; // intentionally unused now; kept for API symmetry
    let token: AuthToken = resp
        .json()
        .context("parse /v0/auth/refresh response")
        .map_err(HttpError::other)?;
    Ok(token)
}
