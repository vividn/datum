//! Push/pull + key-wrap wire types.

use serde::{Deserialize, Serialize};

/// `POST /v1/push` request body.
///
/// `payload_b64` is base64 url-safe no-pad of the raw [`crate::crypto`]-
/// sealed `EncryptedBlob` bytes (`nonce ‖ ciphertext+tag`). The server
/// stores the string opaquely — it has no key and no schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub device_id: String,
    pub payload_b64: String,
}

/// `POST /v1/push` response body. Returns the per-device monotonic seq
/// the relay assigned to this changeset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub seq: i64,
}

/// One row of `GET /v1/pull?since=<id>&device=<dev>`. Returned in
/// ascending server-assigned `id` order, with the caller's own device
/// excluded server-side.
///
/// `id` is the relay-global monotonic row id used by clients as the
/// `pull_seq` watermark (per spec §Sequence semantics). `seq` is the
/// per-device push counter, used as the AAD seq when unsealing the blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Changeset {
    /// Server-assigned global monotonic row id (clients use this as
    /// `pull_seq` watermark).
    pub id: i64,
    pub device_id: String,
    pub seq: i64,
    pub payload_b64: String,
    pub uploaded_at: String,
}

/// Argon2id KDF parameters used to derive a key-encryption key from a
/// user passphrase. Field names + base64 variant are wire-locked by
/// `docs/sync-protocol.md` — do not rename without bumping the protocol
/// version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory cost in KiB (typical: 65536 = 64 MiB).
    pub m_cost: u32,
    /// Time cost (passes). Typical: 3.
    pub t_cost: u32,
    /// Parallelism. Typical: 1.
    pub p_cost: u32,
    /// Base64 url-safe no-pad encoded salt bytes (16 bytes recommended).
    pub salt_b64: String,
}

/// Passphrase-wrapped master key, as round-tripped through
/// `/v1/keys/wrapped`. The relay stores this as opaque JSON; only a
/// holder of the passphrase can unwrap it.
///
/// Wire-locked by `docs/sync-protocol.md` §Wrapped key exchange — field
/// names + algorithm strings cannot change without a protocol bump.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WrappedKey {
    /// Base64 url-safe no-pad ciphertext of the master key. Includes the
    /// 16-byte AES-GCM authenticator tag.
    pub wrapped_key_b64: String,
    /// Always `"argon2id"` in v1.
    pub kdf: String,
    pub kdf_params: KdfParams,
    /// Always `"aes-256-gcm"` in v1.
    pub wrap_alg: String,
    /// Base64 url-safe no-pad 12-byte AES-GCM nonce.
    pub nonce_b64: String,
}
