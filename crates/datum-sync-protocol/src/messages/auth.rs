//! Authentication wire types: bearer token + RFC 8628 device-code grant.

use serde::{Deserialize, Serialize};

/// Wire shape of the bearer token issued by
/// `/v0/auth/{register,login}/finish`, `/v0/auth/device-code/poll`
/// (success branch), and `/v0/auth/refresh`.
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
/// `POST /v0/auth/refresh` for a fresh `(token, refresh_token)` pair
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

/// Body for `POST /v0/auth/refresh`. Trades a single-use refresh token
/// for a fresh bearer + refresh pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// Empty body for `POST /v0/auth/device-code`. Kept as a typed struct
/// rather than `()` so axum's `Json` extractor accepts an empty `{}`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DeviceCodeRequest {}

/// Response body for `POST /v0/auth/device-code`. Field names + defaults
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

/// Query string for `GET /v0/auth/device-code/poll`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodePollQuery {
    pub device_code: String,
}

/// Body for the (server-internal) `POST /v0/auth/device-code/authorize`
/// endpoint a verification UI invokes after the user enters their
/// passphrase. Lives in the protocol crate so a future browser flow has
/// the canonical shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeAuthorizeRequest {
    pub device_code: String,
    pub username: String,
}

/// Response body for `GET /v0/auth/device-code/poll`. Three wire shapes,
/// one Rust type — `#[serde(untagged)]` lets a single deserialize call
/// dispatch on the fields present:
///
/// - **Success**: the bearer token + user_id + refresh_token, matching
///   the [`AuthToken`] shape. Recognised by `token` being set.
/// - **Pending**: `{ "status": "pending" }`. The user hasn't completed
///   the verification UI yet; poll again at the current interval.
/// - **Error**: `{ "error": "<code>", "detail": <optional> }` —
///   `slow_down`, `expired`, etc. RFC 8628 §3.5 error shapes.
///
/// The wire format is unchanged from the prior struct-with-optionals
/// shape and is RFC 8628 compatible. The benefit of the enum is that
/// the client gets compile-time exhaustive matching on the response
/// kind, instead of inspecting which field happens to be Some.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PollResponse {
    /// User completed verification; the bearer is ready. The single-use
    /// `refresh_token` is included so the CLI can rotate without
    /// re-running the device-code flow on expiry.
    Success {
        token: String,
        #[serde(default)]
        user_id: Option<String>,
        #[serde(default)]
        refresh_token: Option<String>,
        /// Default cluster bound to the user at completion of the
        /// device-code grant. Absent on v1 responses so old relays
        /// still parse.
        #[serde(default)]
        cluster_id: Option<String>,
    },
    /// Authorization still pending — the verification UI hasn't been
    /// completed. Client keeps polling at the current interval.
    Pending { status: String },
    /// Terminal or transient error from the relay. `error` carries the
    /// machine-readable code (`slow_down`, `expired`, ...). `detail`
    /// is an optional human-readable hint.
    Error {
        error: String,
        #[serde(default)]
        detail: Option<String>,
    },
}
