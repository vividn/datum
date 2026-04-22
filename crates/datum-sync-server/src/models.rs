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
    pub payload: String, // base64-encoded encrypted bytes
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PushResponse {
    pub seq: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Changeset {
    pub device_id: String,
    pub seq: i64,
    pub payload: String,
    pub uploaded_at: String,
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
