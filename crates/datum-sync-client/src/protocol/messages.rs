//! Wire types for the sync protocol. Exactly the JSON shape on the HTTP
//! boundary — the relay has no schema knowledge beyond this contract.

use serde::{Deserialize, Serialize};

/// Bearer token persisted per server after a successful device-code flow.
/// Serialized to `$XDG_CONFIG_HOME/datum/auth/<host>.json` by the CLI; the
/// client library is agnostic to storage — callers pass the already-loaded
/// token into `http::*` functions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub token: String,
    /// RFC3339 timestamp of when the token was issued or last refreshed.
    pub refreshed_at: String,
    /// Server URL the token was minted against. Lets callers sanity-check
    /// token/server mismatches before firing a request.
    pub server: String,
    #[serde(default)]
    pub user: Option<String>,
}

/// Response body for `POST /v1/auth/device-code`. Field names and defaults
/// match RFC 8628 §3.2.
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
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// Response body for `GET /v1/auth/device-code/poll`. Either a `token` is
/// present (success), `status = "pending"` (keep polling), or an `error`
/// field is set (`slow_down`, `expired`, etc.).
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
