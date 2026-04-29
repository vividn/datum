//! Authentication wire types: bearer token + RFC 8628 device-code grant.

use serde::{Deserialize, Serialize};

/// Bearer token returned by `/v1/auth/{register,login}/finish` and
/// `/v1/auth/device-code/poll` (success branch). Also the on-disk
/// persistence shape used by the CLI for `$XDG_CONFIG_HOME/datum/auth/<host>.json`.
///
/// Server populates `token` + `user_id`. Client additionally fills
/// `refreshed_at` + `server` at persistence time. The `user` alias on
/// `user_id` reads legacy persisted files written before this crate
/// unified the shape — new writes use `user_id`.
///
/// Field-name reconciliation between server `{token, user_id}` and client
/// `{token, user, refreshed_at, server}` is tracked in da-6hn PR3; PR1
/// (this crate's introduction) preserves both wire and on-disk compat
/// via Optional fields and the `user` serde alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub token: String,
    /// User identifier. Server emits the user UUID as a string; client
    /// persists the same value. `serde(alias = "user")` lets us read
    /// legacy CLI auth files that wrote `"user"` instead of `"user_id"`.
    #[serde(default, alias = "user", skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// RFC 3339 timestamp of when the token was minted or refreshed.
    /// Set client-side at persistence time; absent in raw server responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<String>,
    /// Origin server URL the token was minted against. Set client-side
    /// at persistence time; lets callers sanity-check token/server
    /// mismatches before firing a request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
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
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}
