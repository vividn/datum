//! Push/pull + key-wrap wire types.

use serde::{Deserialize, Serialize};

/// `POST /v1/push` request body.
///
/// `payload_b64` is base64 url-safe no-pad of the raw [`crate::crypto`]-
/// sealed `EncryptedBlob` bytes (`nonce ‖ ciphertext+tag`). The server
/// stores the string opaquely — it has no key and no schema.
///
/// `cluster_id` + `generation` (da-soeu, sub-1 of the multi-DB epic
/// da-u6lm): both Optional on the wire so a v1 client that omits them
/// stays parseable; the server treats absent values as the user's default
/// cluster on the current generation. New clients send them always.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub device_id: String,
    pub payload_b64: String,
    /// Cluster the changeset belongs to. A user may have multiple
    /// clusters (multi-DB). Absent → default cluster.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    /// Cluster generation at the time of push. The server rejects pushes
    /// whose generation disagrees with the cluster's current value with
    /// [`crate::errors::SyncProtocolError::GenerationMismatch`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
}

/// `POST /v1/push` response body. Returns the per-device monotonic seq
/// the relay assigned to this changeset, plus the cluster generation the
/// row was accepted into (lets the client confirm it stayed on the
/// expected generation across the round-trip).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub seq: i64,
    /// Cluster the row was accepted into. Optional for v1 compat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    /// Cluster generation the server accepted the push into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
}

/// One row of `GET /v1/pull?since=<id>&device=<dev>`. Returned in
/// ascending server-assigned `id` order, with the caller's own device
/// excluded server-side.
///
/// `id` is the relay-global monotonic row id used by clients as the
/// `pull_seq` watermark (per spec §Sequence semantics). `seq` is the
/// per-device push counter, used as the AAD seq when unsealing the blob.
///
/// `cluster_id` + `generation` (da-soeu): rows are scoped to one cluster
/// and one generation. Pulls filter server-side by cluster; mismatching
/// generations surface as [`crate::errors::SyncProtocolError::GenerationMismatch`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Changeset {
    /// Server-assigned global monotonic row id (clients use this as
    /// `pull_seq` watermark).
    pub id: i64,
    pub device_id: String,
    pub seq: i64,
    pub payload_b64: String,
    pub uploaded_at: String,
    /// Cluster the row belongs to. Absent on v1 rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
    /// Cluster generation when the row was accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<u64>,
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

#[cfg(test)]
mod tests {
    //! Wire-format pins for da-soeu (sub-1 of multi-DB epic da-u6lm):
    //! `cluster_id` and `generation` must serialize to the documented
    //! JSON keys, round-trip, AND stay parseable when a v1 client omits
    //! them — that's the backward-compat contract.

    use super::*;

    #[test]
    fn push_request_round_trips_cluster_and_generation() {
        let req = PushRequest {
            device_id: "dev-1".into(),
            payload_b64: "AAAA".into(),
            cluster_id: Some("c-7".into()),
            generation: Some(42),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"cluster_id\":\"c-7\""), "wire key: {json}");
        assert!(json.contains("\"generation\":42"), "wire key: {json}");

        let back: PushRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cluster_id.as_deref(), Some("c-7"));
        assert_eq!(back.generation, Some(42));
    }

    #[test]
    fn push_request_parses_v1_payload_without_new_fields() {
        // v1 client: no cluster_id / generation. New server must still
        // deserialize.
        let v1 = r#"{"device_id":"dev","payload_b64":"AA"}"#;
        let req: PushRequest = serde_json::from_str(v1).unwrap();
        assert!(req.cluster_id.is_none());
        assert!(req.generation.is_none());

        // And re-serializing omits the absent keys (skip_serializing_if).
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            !json.contains("cluster_id"),
            "absent keys must not leak: {json}"
        );
        assert!(
            !json.contains("generation"),
            "absent keys must not leak: {json}"
        );
    }

    #[test]
    fn changeset_round_trips_cluster_and_generation() {
        let cs = Changeset {
            id: 1,
            device_id: "dev".into(),
            seq: 2,
            payload_b64: "AA".into(),
            uploaded_at: "2026-05-21T17:00:00Z".into(),
            cluster_id: Some("c-1".into()),
            generation: Some(3),
        };
        let json = serde_json::to_string(&cs).unwrap();
        let back: Changeset = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cluster_id.as_deref(), Some("c-1"));
        assert_eq!(back.generation, Some(3));
    }

    #[test]
    fn push_response_round_trips_cluster_and_generation() {
        let resp = PushResponse {
            seq: 99,
            cluster_id: Some("c".into()),
            generation: Some(1),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: PushResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.seq, 99);
        assert_eq!(back.cluster_id.as_deref(), Some("c"));
        assert_eq!(back.generation, Some(1));
    }
}
