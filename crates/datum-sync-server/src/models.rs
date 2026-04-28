use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
pub struct AuthToken {
    pub token: String,
    pub user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushRequest {
    pub device_id: String,
    /// base64 url-safe no-pad encoded encrypted bytes (per docs/sync-protocol.md)
    pub payload_b64: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushResponse {
    pub seq: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Changeset {
    /// Server-assigned global monotonic row id (clients use this as pull_seq watermark)
    pub id: i64,
    pub device_id: String,
    pub seq: i64,
    pub payload_b64: String,
    pub uploaded_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceCodeRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceCodePollQuery {
    pub device_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceCodeAuthorizeRequest {
    pub device_code: String,
    pub username: String,
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
