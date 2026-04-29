//! Server models.
//!
//! Wire types live in `datum-sync-protocol` so client and server can never
//! drift. Server-only types — webauthn challenge bodies, JWT claims, the
//! axum-extracted `AuthenticatedUser` — stay here because they either pull
//! `webauthn-rs` (not WASM-safe) or only make sense on the relay side.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Re-export the shared wire types so existing `crate::models::Foo` imports
// keep working unchanged. PR2+ may move callers to import directly from
// `datum_sync_protocol`.
pub use datum_sync_protocol::messages::auth::{
    AuthToken, DeviceCodeAuthorizeRequest, DeviceCodePollQuery, DeviceCodeRequest,
    DeviceCodeResponse, PollResponse,
};
pub use datum_sync_protocol::messages::sync::{
    Changeset, KdfParams, PushRequest, PushResponse, WrappedKey,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RegisterBeginRequest {
    pub username: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterBeginResponse {
    pub user_id: String,
    #[serde(flatten)]
    pub challenge: webauthn_rs::prelude::CreationChallengeResponse,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterFinishRequest {
    pub user_id: String,
    pub credential: webauthn_rs::prelude::RegisterPublicKeyCredential,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginBeginRequest {
    pub username: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginBeginResponse {
    pub user_id: String,
    #[serde(flatten)]
    pub challenge: webauthn_rs::prelude::RequestChallengeResponse,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginFinishRequest {
    pub user_id: String,
    pub credential: webauthn_rs::prelude::PublicKeyCredential,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user_id
    pub exp: usize,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
}
