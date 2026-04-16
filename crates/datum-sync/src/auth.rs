use axum::{extract::State, Json};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::{
    db,
    error::{AppError, AppResult},
    jwt::issue_jwt,
    AppState,
};

/// Derive a deterministic UUID for a user_id string.
fn user_uuid(user_id: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, user_id.as_bytes())
}

// ---- Register ----

#[derive(Deserialize)]
pub struct RegisterBeginReq {
    pub user_id: String,
}

#[derive(Serialize)]
pub struct RegisterBeginResp {
    pub state_id: String,
    pub challenge: CreationChallengeResponse,
}

pub async fn register_begin(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterBeginReq>,
) -> AppResult<Json<RegisterBeginResp>> {
    db::create_user(&state.db, &req.user_id)?;

    // Build exclude list so users can't re-register the same key
    let existing = db::get_credentials_for_user(&state.db, &req.user_id)?;
    let exclude: Option<Vec<CredentialID>> = if existing.is_empty() {
        None
    } else {
        Some(
            existing
                .iter()
                .filter_map(|(_, data)| serde_json::from_str::<Passkey>(data).ok())
                .map(|sk| sk.cred_id().clone())
                .collect(),
        )
    };

    let (ccr, reg_state) = state
        .webauthn
        .start_passkey_registration(user_uuid(&req.user_id), &req.user_id, &req.user_id, exclude)
        .map_err(|e| AppError::WebAuthn(e.to_string()))?;

    let state_id = Uuid::new_v4().to_string();
    state
        .reg_states
        .lock()
        .unwrap()
        .insert(format!("{}:{}", req.user_id, state_id), reg_state);

    Ok(Json(RegisterBeginResp {
        state_id,
        challenge: ccr,
    }))
}

#[derive(Deserialize)]
pub struct RegisterFinishReq {
    pub user_id: String,
    pub state_id: String,
    pub registration: RegisterPublicKeyCredential,
}

#[derive(Serialize)]
pub struct TokenResp {
    pub token: String,
}

pub async fn register_finish(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterFinishReq>,
) -> AppResult<Json<TokenResp>> {
    let key = format!("{}:{}", req.user_id, req.state_id);
    let reg_state = state
        .reg_states
        .lock()
        .unwrap()
        .remove(&key)
        .ok_or_else(|| AppError::Auth("Invalid or expired registration state".into()))?;

    let passkey = state
        .webauthn
        .finish_passkey_registration(&req.registration, &reg_state)
        .map_err(|e| AppError::WebAuthn(e.to_string()))?;

    let cred_id = BASE64_URL_SAFE_NO_PAD.encode(passkey.cred_id().as_ref());
    let cred_data =
        serde_json::to_string(&passkey).map_err(|e| AppError::Internal(e.to_string()))?;

    db::store_credential(&state.db, &cred_id, &req.user_id, &cred_data)?;

    let token = issue_jwt(&req.user_id, &state.jwt_secret)?;
    Ok(Json(TokenResp { token }))
}

// ---- Login ----

#[derive(Deserialize)]
pub struct LoginBeginReq {
    pub user_id: String,
}

#[derive(Serialize)]
pub struct LoginBeginResp {
    pub state_id: String,
    pub challenge: RequestChallengeResponse,
}

pub async fn login_begin(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginBeginReq>,
) -> AppResult<Json<LoginBeginResp>> {
    if !db::user_exists(&state.db, &req.user_id)? {
        return Err(AppError::NotFound(format!("User '{}' not found", req.user_id)));
    }

    let creds = db::get_credentials_for_user(&state.db, &req.user_id)?;
    if creds.is_empty() {
        return Err(AppError::Auth(
            "No credentials registered for this user".into(),
        ));
    }

    let passkeys: Vec<Passkey> = creds
        .iter()
        .filter_map(|(_, data)| serde_json::from_str(data).ok())
        .collect();

    if passkeys.is_empty() {
        return Err(AppError::Internal(
            "Failed to deserialize stored credentials".into(),
        ));
    }

    let (rcr, auth_state) = state
        .webauthn
        .start_passkey_authentication(&passkeys)
        .map_err(|e| AppError::WebAuthn(e.to_string()))?;

    let state_id = Uuid::new_v4().to_string();
    state
        .auth_states
        .lock()
        .unwrap()
        .insert(state_id.clone(), (req.user_id, auth_state));

    Ok(Json(LoginBeginResp {
        state_id,
        challenge: rcr,
    }))
}

#[derive(Deserialize)]
pub struct LoginFinishReq {
    pub state_id: String,
    pub credential: PublicKeyCredential,
}

pub async fn login_finish(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginFinishReq>,
) -> AppResult<Json<TokenResp>> {
    let (user_id, auth_state) = state
        .auth_states
        .lock()
        .unwrap()
        .remove(&req.state_id)
        .ok_or_else(|| AppError::Auth("Invalid or expired login state".into()))?;

    let auth_result = state
        .webauthn
        .finish_passkey_authentication(&req.credential, &auth_state)
        .map_err(|e| AppError::WebAuthn(e.to_string()))?;

    // Update credential counter (best-effort — prevents clone detection regression)
    let used_cred_id = BASE64_URL_SAFE_NO_PAD.encode(auth_result.cred_id().as_ref());
    if let Ok(creds) = db::get_credentials_for_user(&state.db, &user_id) {
        for (id, data) in &creds {
            if *id == used_cred_id {
                if let Ok(mut passkey) = serde_json::from_str::<Passkey>(data) {
                    passkey.update_credential(&auth_result);
                    if let Ok(updated) = serde_json::to_string(&passkey) {
                        let _ = db::update_credential(&state.db, id, &updated);
                    }
                }
                break;
            }
        }
    }

    let token = issue_jwt(&user_id, &state.jwt_secret)?;
    Ok(Json(TokenResp { token }))
}
