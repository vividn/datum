//! Authentication wire types: bearer token + RFC 8628 device-code grant.

use serde::{Deserialize, Serialize};

/// Wire shape of the bearer token issued by
/// `/v1/auth/{register,login}/finish`, `/v1/auth/device-code/poll`
/// (success branch), and `/v1/auth/refresh`.
///
/// Server-only contract: this struct is what the server emits and what
/// the client deserializes. The CLI's on-disk shape is
/// `datum_sync_client::auth::PersistedAuthToken`, which wraps this and
/// adds the client-side metadata (`refreshed_at`, `server`) at
/// persistence time (da-ixmh). Required fields are `String`, not
/// `Option<String>`, so the type reads as "these always flow on the
/// wire" instead of "everything optional, good luck."
///
/// `refresh_token` is a 32-byte random value (URL-safe base64) the
/// server stores as a SHA-256 hash. The client trades it via
/// `POST /v1/auth/refresh` for a fresh `(token, refresh_token)` pair
/// when its bearer expires; the old refresh value is single-use and is
/// invalidated server-side as part of the rotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub token: String,
    /// User identifier (server emits the UUID as a string).
    /// `serde(alias = "user")` reads legacy CLI auth files that wrote
    /// `"user"` instead of `"user_id"`; new writes use `"user_id"`.
    #[serde(alias = "user")]
    pub user_id: String,
    /// Single-use refresh token. Issued on every fresh
    /// `/register/finish`, `/login/finish`, `device-code/poll` (success),
    /// and `/auth/refresh` response.
    pub refresh_token: String,
    /// Default cluster the user is bound to at issuance time. Absent on
    /// v1 tokens. Lets the client default cluster-scoped requests
    /// without a separate `/clusters` round-trip (da-soeu, sub-1 of
    /// multi-DB epic da-u6lm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,
}

/// Body for `POST /v1/auth/refresh`. Trades a single-use refresh token
/// for a fresh bearer + refresh pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// Empty body for `POST /v1/auth/device-code`. Kept as a typed struct
/// rather than `()` so axum's `Json` extractor accepts an empty `{}`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DeviceCodeRequest {}

/// Response body for `POST /v1/auth/device-code`. Field names + defaults
/// match RFC 8628 §3.2.
///
/// `verification_uri_complete`, `expires_in`, and `interval` are Optional
/// on the wire to stay forward-compatible with relays that omit them; the
/// reference server always populates them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    /// Poll interval in seconds. RFC 8628 default is 5.
    #[serde(default = "default_poll_interval")]
    pub interval: u64,
}

fn default_poll_interval() -> u64 {
    5
}

/// Query string for `GET /v1/auth/device-code/poll`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodePollQuery {
    pub device_code: String,
}

/// Body for the (server-internal) `POST /v1/auth/device-code/authorize`
/// endpoint a verification UI invokes after the user enters their
/// passphrase. Lives in the protocol crate so a future browser flow has
/// the canonical shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeAuthorizeRequest {
    pub device_code: String,
    pub username: String,
}

/// Response body for `GET /v1/auth/device-code/poll`. Polymorphic by
/// design: the server returns one of three shapes inline (success →
/// `token` set; pending → `status = "pending"`; error → `error` set with
/// e.g. `slow_down`/`expired`). Every field is Optional so a single
/// deserialize call handles all three cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollResponse {
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    /// Single-use refresh token, populated on the success branch only.
    /// The CLI persists this alongside the bearer so subsequent expiry
    /// can rotate without re-running the device-code flow.
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    /// Default cluster bound to the user at completion of the
    /// device-code grant. Populated on the success branch only; absent
    /// on v1 responses so old relays still parse.
    #[serde(default)]
    pub cluster_id: Option<String>,
}
