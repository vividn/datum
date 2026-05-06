//! Changeset push + pull HTTP clients.
//!
//! Wire format is locked by `docs/sync-protocol.md`:
//!
//! - `POST /v1/push` — body `{device_id, payload_b64}`, response `{seq}`.
//!   The server assigns a per-device monotonic `seq` and echoes it back.
//! - `GET /v1/pull?since=<seq>&device=<id>` — returns peers' changesets in
//!   ascending `seq` order. The caller's own device is excluded server-side.
//!
//! The payload is the raw bytes of an [`EncryptedBlob`] (nonce ‖ ciphertext ‖ tag),
//! base64 url-safe no-pad encoded. This module knows nothing about plaintext
//! or keys — encryption / decryption is the caller's job (see
//! [`crate::crypto::changeset`]).
//!
//! Gated by the `http` cargo feature because it depends on `reqwest`; the
//! WASM surface reimplements the same wire format over its own fetch shim.

use anyhow::Context;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use datum_sync_protocol::messages::sync::{
    Changeset as WireChangeset, PushRequest, PushResponse,
};
use reqwest::blocking::Client;

use crate::http::error::HttpError;

/// One row from `GET /v1/pull`, with the payload already base64-decoded to
/// raw encrypted bytes. The wire shape itself is
/// [`datum_sync_protocol::messages::sync::Changeset`]; this struct is the
/// post-decode native form used by callers (`payload: Vec<u8>` instead of
/// `payload_b64: String`). The `uploaded_at` field is an RFC-3339 string
/// per the spec; callers that need a [`chrono::DateTime`] can parse it on
/// their side.
#[derive(Debug, Clone)]
pub struct PulledChangeset {
    /// Global monotonic id assigned by the relay — the value clients use
    /// to advance their local `pull_seq` watermark (per spec §Sequence
    /// semantics, `pull_seq` is the relay's row id, not a per-device seq).
    pub id: i64,
    pub device_id: String,
    /// Per-device push counter, used as the AAD seq when
    /// [`crate::crypto::changeset::decrypt_changeset`] unseals the blob.
    pub seq: i64,
    /// Raw [`EncryptedBlob`] bytes (already base64-decoded).
    pub payload: Vec<u8>,
    pub uploaded_at: String,
}

/// POST a single encrypted changeset blob to the relay. Returns the
/// server-assigned per-device `seq`.
///
/// `payload` is the raw bytes of an [`EncryptedBlob`]; this helper handles
/// the base64 encoding on the wire.
pub fn push_blob(
    client: &Client,
    server: &str,
    bearer: &str,
    device_id: &str,
    payload: &[u8],
) -> Result<i64, HttpError> {
    let body = PushRequest {
        device_id: device_id.to_string(),
        payload_b64: URL_SAFE_NO_PAD.encode(payload),
    };
    let resp = client
        .post(format!("{server}/v1/push"))
        .bearer_auth(bearer)
        .json(&body)
        .send()
        .context("POST /v1/push")
        .map_err(HttpError::other)?;
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(HttpError::Unauthorized);
    }
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        return Err(HttpError::other(anyhow::anyhow!(
            "push failed: {status} {text}"
        )));
    }
    let parsed: PushResponse = resp
        .json()
        .context("parse push response")
        .map_err(HttpError::other)?;
    Ok(parsed.seq)
}

/// Fetch every changeset the relay has with global id > `since` that was
/// authored by a device other than `device_id`. Rows are returned in
/// ascending server-assigned seq order. Empty vec means nothing new.
pub fn pull_batch(
    client: &Client,
    server: &str,
    bearer: &str,
    device_id: &str,
    since: i64,
) -> Result<Vec<PulledChangeset>, HttpError> {
    let resp = client
        .get(format!("{server}/v1/pull"))
        .bearer_auth(bearer)
        .query(&[
            ("since", since.to_string()),
            ("device", device_id.to_string()),
        ])
        .send()
        .context("GET /v1/pull")
        .map_err(HttpError::other)?;
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(HttpError::Unauthorized);
    }
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        return Err(HttpError::other(anyhow::anyhow!(
            "pull failed: {status} {text}"
        )));
    }
    let rows: Vec<WireChangeset> = resp
        .json()
        .context("parse pull response")
        .map_err(HttpError::other)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let payload = URL_SAFE_NO_PAD
            .decode(row.payload_b64.as_bytes())
            .with_context(|| {
                format!("decode payload_b64 for seq {} from {}", row.seq, row.device_id)
            })
            .map_err(HttpError::other)?;
        out.push(PulledChangeset {
            id: row.id,
            device_id: row.device_id,
            seq: row.seq,
            payload,
            uploaded_at: row.uploaded_at,
        });
    }
    Ok(out)
}
