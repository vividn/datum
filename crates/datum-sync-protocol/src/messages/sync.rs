//! Push/pull + key-wrap wire types.

use serde::{Deserialize, Serialize};

/// Ceiling on rows per `GET /v0/pull` response, and the value the relay
/// uses when a client sends no `limit`. It lives here, in the wire
/// contract, because both sides have to agree on it: the client sizes its
/// pull page from this constant and treats a short page as end-of-stream,
/// so a client that believed a different number would read the relay's
/// cap as "no more rows" and stop early (da-blcb). A `limit` above this is
/// a protocol error, not something the relay quietly substitutes for.
///
/// Each row is one client push page, so this bounds a pull response at
/// roughly `MAX_PULL_ROWS ×` the push body limit's worth of history — big
/// enough that a normal sync is one request, small enough that a first
/// sync of a long history is many bounded ones.
pub const MAX_PULL_ROWS: usize = 100;

/// `POST /v0/push` request body.
///
/// `payload_b64` is base64 url-safe no-pad of the raw [`crate::crypto`]-
/// sealed `EncryptedBlob` bytes (`nonce ‖ ciphertext+tag`). The server
/// stores the string opaquely — it has no key and no schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub device_id: String,
    pub payload_b64: String,
    /// Cluster the changeset belongs to. A user may have multiple
    /// clusters (multi-DB).
    pub cluster_id: String,
    /// Cluster generation at the time of push. The server rejects pushes
    /// whose generation disagrees with the cluster's current value with
    /// [`crate::errors::SyncProtocolError::GenerationMismatch`].
    pub generation: u64,
    /// The per-device `seq` the client predicted (`device_seq + 1`) and
    /// bound into the changeset's AAD at encrypt time. The server compares
    /// it to its own authoritative next seq; a disagreement is rejected
    /// with [`crate::errors::SyncProtocolError::SeqMismatch`] *before* the
    /// blob is stored, so a corrupt-AAD row never lands. Unlike the old
    /// store-then-assign flow this makes the mismatch recoverable: the
    /// client re-bases to the returned `expected`, re-encrypts, and
    /// retries (da-ca4q).
    pub predicted_seq: i64,
}

/// `POST /v0/push` response body. Returns the per-device monotonic seq
/// the relay assigned to this changeset, plus the cluster generation the
/// row was accepted into (lets the client confirm it stayed on the
/// expected generation across the round-trip).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub seq: i64,
    /// Cluster the row was accepted into.
    pub cluster_id: String,
    /// Cluster generation the server accepted the push into.
    pub generation: u64,
}

/// One row of `GET /v0/pull?since=<id>&device=<dev>`. Returned in
/// ascending server-assigned `id` order, with the caller's own device
/// excluded server-side.
///
/// `id` is the relay-global monotonic row id used by clients as the
/// `pull_seq` watermark (per spec §Sequence semantics). `seq` is the
/// per-device push counter, used as the AAD seq when unsealing the blob.
///
/// `cluster_id` + `generation`: rows are scoped to one cluster and one
/// generation. Pulls filter server-side by cluster; mismatching
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
    /// Cluster the row belongs to.
    pub cluster_id: String,
    /// Cluster generation when the row was accepted.
    pub generation: u64,
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
/// `/v0/keys/wrapped`. The relay stores this as opaque JSON; only a
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
    //! Wire-format pins for the post-pre-prod required-fields contract:
    //! `cluster_id` and `generation` are required on every push, response,
    //! and changeset row. Payloads omitting them fail to deserialize —
    //! enforced at the wire boundary.

    use super::*;

    #[test]
    fn push_request_round_trips_cluster_and_generation() {
        let req = PushRequest {
            device_id: "dev-1".into(),
            payload_b64: "AAAA".into(),
            cluster_id: "c-7".into(),
            generation: 42,
            predicted_seq: 5,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"cluster_id\":\"c-7\""), "wire key: {json}");
        assert!(json.contains("\"generation\":42"), "wire key: {json}");
        assert!(json.contains("\"predicted_seq\":5"), "wire key: {json}");

        let back: PushRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cluster_id, "c-7");
        assert_eq!(back.generation, 42);
        assert_eq!(back.predicted_seq, 5);
    }

    #[test]
    fn push_request_rejects_v1_payload_missing_cluster_fields() {
        // Pre-prod cleanup: cluster_id and generation are required. A
        // payload that omits them is malformed — the server SHOULD reject
        // it at the deserialize boundary.
        let v1 = r#"{"device_id":"dev","payload_b64":"AA"}"#;
        let err = serde_json::from_str::<PushRequest>(v1).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("cluster_id") || msg.contains("missing field"),
            "expected missing-field error, got: {msg}",
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
            cluster_id: "c-1".into(),
            generation: 3,
        };
        let json = serde_json::to_string(&cs).unwrap();
        let back: Changeset = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cluster_id, "c-1");
        assert_eq!(back.generation, 3);
    }

    #[test]
    fn push_response_round_trips_cluster_and_generation() {
        let resp = PushResponse {
            seq: 99,
            cluster_id: "c".into(),
            generation: 1,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: PushResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.seq, 99);
        assert_eq!(back.cluster_id, "c");
        assert_eq!(back.generation, 1);
    }
}
