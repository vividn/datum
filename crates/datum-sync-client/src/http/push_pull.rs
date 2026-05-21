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
use datum_sync_protocol::messages::sync::{Changeset as WireChangeset, PushRequest, PushResponse};
use datum_sync_protocol::SyncProtocolError;
use reqwest::blocking::Client;

use crate::http::error::HttpError;

/// Outcome of a successful push. Echoes back the per-device monotonic
/// `seq` the relay assigned plus the cluster_id + generation the row
/// landed on (so the client can persist server-assigned values into
/// local `sync_state` on first sync). Optional for v1 compat.
#[derive(Debug, Clone)]
pub struct PushAck {
    pub seq: i64,
    pub cluster_id: Option<String>,
    pub generation: Option<u64>,
}

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
    /// Cluster the row was accepted into. Absent on v1 server responses.
    /// (da-eu7o.)
    pub cluster_id: Option<String>,
    /// Cluster generation when the row was accepted. Absent on v1
    /// server responses. (da-eu7o.)
    pub generation: Option<u64>,
}

/// POST a single encrypted changeset blob to the relay. Returns the
/// server-assigned per-device `seq` plus the cluster_id + generation the
/// row was accepted into (so the caller can persist them on first sync
/// or after a `bump-generation` round-trip).
///
/// `payload` is the raw bytes of an [`EncryptedBlob`]; this helper handles
/// the base64 encoding on the wire.
///
/// `cluster_id` / `generation` are optional on the wire (v1 clients send
/// neither). New clients that have stored a cluster_id locally should
/// always send both so the server can run the generation-match gate;
/// a [`SyncProtocolError::GenerationMismatch`] from the relay surfaces
/// as [`HttpError::GenerationMismatch`].
pub fn push_blob(
    client: &Client,
    server: &str,
    bearer: &str,
    device_id: &str,
    payload: &[u8],
    cluster_id: Option<&str>,
    generation: Option<u64>,
) -> Result<PushAck, HttpError> {
    let body = PushRequest {
        device_id: device_id.to_string(),
        payload_b64: URL_SAFE_NO_PAD.encode(payload),
        cluster_id: cluster_id.map(str::to_string),
        generation,
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
    if status.as_u16() == 409 {
        // The server's IntoResponse for GenerationMismatch emits the
        // tagged-enum SyncProtocolError JSON body directly. Parse-fail
        // here means a non-protocol 409, surface as Other.
        let text = resp.text().unwrap_or_default();
        return Err(parse_generation_mismatch(&text, "push"));
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
    Ok(PushAck {
        seq: parsed.seq,
        cluster_id: parsed.cluster_id,
        generation: parsed.generation,
    })
}

fn parse_generation_mismatch(body: &str, context: &str) -> HttpError {
    match serde_json::from_str::<SyncProtocolError>(body) {
        Ok(err) => HttpError::GenerationMismatch(err),
        Err(parse_err) => HttpError::other(anyhow::anyhow!(
            "{context} returned 409 with unparseable body ({parse_err}): {body}"
        )),
    }
}

/// Fetch every changeset the relay has with global id > `since` that was
/// authored by a device other than `device_id`. Rows are returned in
/// ascending server-assigned seq order. Empty vec means nothing new.
///
/// `cluster_id` / `generation` are optional for v1 compat. New clients
/// that have a stored cluster_id should pass both so the server runs
/// the generation gate; a mismatch surfaces as
/// [`HttpError::GenerationMismatch`] (the client must reset its
/// watermark and re-pull from `generation = expected`).
pub fn pull_batch(
    client: &Client,
    server: &str,
    bearer: &str,
    device_id: &str,
    since: i64,
    cluster_id: Option<&str>,
    generation: Option<u64>,
) -> Result<Vec<PulledChangeset>, HttpError> {
    let mut query: Vec<(&str, String)> = vec![
        ("since", since.to_string()),
        ("device", device_id.to_string()),
    ];
    if let Some(c) = cluster_id {
        query.push(("cluster_id", c.to_string()));
    }
    if let Some(g) = generation {
        query.push(("generation", g.to_string()));
    }
    let resp = client
        .get(format!("{server}/v1/pull"))
        .bearer_auth(bearer)
        .query(&query)
        .send()
        .context("GET /v1/pull")
        .map_err(HttpError::other)?;
    let status = resp.status();
    if status.as_u16() == 401 {
        return Err(HttpError::Unauthorized);
    }
    if status.as_u16() == 409 {
        let text = resp.text().unwrap_or_default();
        return Err(parse_generation_mismatch(&text, "pull"));
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
                format!(
                    "decode payload_b64 for seq {} from {}",
                    row.seq, row.device_id
                )
            })
            .map_err(HttpError::other)?;
        out.push(PulledChangeset {
            id: row.id,
            device_id: row.device_id,
            seq: row.seq,
            payload,
            uploaded_at: row.uploaded_at,
            cluster_id: row.cluster_id,
            generation: row.generation,
        });
    }
    Ok(out)
}
