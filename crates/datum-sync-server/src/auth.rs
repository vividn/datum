use axum::{extract::State, Json};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use crate::{
    db, error::AppError, jwt, refresh,
    models::{
        AuthToken, LoginBeginRequest, LoginBeginResponse, LoginFinishRequest,
        RefreshTokenRequest, RegisterBeginRequest, RegisterBeginResponse,
        RegisterFinishRequest,
    },
    AppState,
};

pub async fn register_begin(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterBeginRequest>,
) -> Result<Json<RegisterBeginResponse>, AppError> {
    let (user_id, ccr) = {
        let conn = state.db.lock().unwrap();

        let user_id = match db::get_user_by_username(&conn, &req.username)? {
            Some(id) => id,
            None => db::create_user(&conn, &req.username)?,
        };

        let display_name = req.display_name.as_deref().unwrap_or(&req.username);

        let (ccr, reg_state) = state
            .webauthn
            .start_passkey_registration(user_id, &req.username, display_name, None)?;

        state
            .reg_states
            .lock()
            .unwrap()
            .insert(user_id.to_string(), reg_state);

        (user_id, ccr)
    };

    Ok(Json(RegisterBeginResponse {
        user_id: user_id.to_string(),
        challenge: ccr,
    }))
}

pub async fn register_finish(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterFinishRequest>,
) -> Result<Json<AuthToken>, AppError> {
    let user_id = Uuid::parse_str(&req.user_id)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let reg_state = state
        .reg_states
        .lock()
        .unwrap()
        .remove(&user_id.to_string())
        .ok_or_else(|| AppError::BadRequest("No pending registration for this user".to_string()))?;

    let passkey = state
        .webauthn
        .finish_passkey_registration(&req.credential, &reg_state)?;

    let refresh_token = {
        let conn = state.db.lock().unwrap();
        db::save_credential(&conn, &user_id, &passkey)?;
        refresh::mint_and_persist(&conn, &user_id)?
    };

    let token = jwt::create_token(&user_id, &state.jwt_secret)?;
    Ok(Json(AuthToken {
        token,
        user_id: Some(user_id.to_string()),
        refresh_token: Some(refresh_token),
        refreshed_at: None,
        server: None,
    }))
}

pub async fn login_begin(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginBeginRequest>,
) -> Result<Json<LoginBeginResponse>, AppError> {
    let (user_id, rcr) = {
        let conn = state.db.lock().unwrap();

        let user_id = db::get_user_by_username(&conn, &req.username)?
            .ok_or(AppError::NotFound)?;

        let passkeys = db::get_passkeys(&conn, &user_id)?;
        if passkeys.is_empty() {
            return Err(AppError::BadRequest(
                "No credentials found for user".to_string(),
            ));
        }

        let (rcr, auth_state) = state
            .webauthn
            .start_passkey_authentication(&passkeys)?;

        state
            .auth_states
            .lock()
            .unwrap()
            .insert(user_id.to_string(), auth_state);

        (user_id, rcr)
    };

    Ok(Json(LoginBeginResponse {
        user_id: user_id.to_string(),
        challenge: rcr,
    }))
}

pub async fn login_finish(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginFinishRequest>,
) -> Result<Json<AuthToken>, AppError> {
    let user_id = Uuid::parse_str(&req.user_id)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let auth_state = state
        .auth_states
        .lock()
        .unwrap()
        .remove(&user_id.to_string())
        .ok_or_else(|| {
            AppError::BadRequest("No pending authentication for this user".to_string())
        })?;

    let auth_result = state
        .webauthn
        .finish_passkey_authentication(&req.credential, &auth_state)?;

    // Update passkey counter for the used credential and mint a fresh
    // refresh token under the same lock so we don't have to grab the
    // mutex twice in a row.
    let refresh_token = {
        let conn = state.db.lock().unwrap();
        let mut passkeys = db::get_passkeys(&conn, &user_id)?;
        for pk in passkeys.iter_mut() {
            if pk.update_credential(&auth_result).is_some() {
                db::save_credential(&conn, &user_id, pk)?;
                break;
            }
        }
        refresh::mint_and_persist(&conn, &user_id)?
    };

    let token = jwt::create_token(&user_id, &state.jwt_secret)?;
    Ok(Json(AuthToken {
        token,
        user_id: Some(user_id.to_string()),
        refresh_token: Some(refresh_token),
        refreshed_at: None,
        server: None,
    }))
}

/// `POST /v1/auth/refresh`
///
/// Trade a single-use refresh token for a fresh `(token, refresh_token)`
/// pair. Rotation is enforced server-side: the consumed row is deleted
/// before the replacement is inserted, so a leaked refresh token is
/// useful at most once and an attacker who races a legitimate client
/// loses access on the next legitimate refresh.
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshTokenRequest>,
) -> Result<Json<AuthToken>, AppError> {
    let token_hash = refresh::hash_token(&req.refresh_token);

    let user_id = {
        let conn = state.db.lock().unwrap();
        let (user_id, expires_at) =
            db::lookup_refresh_token(&conn, &token_hash)?.ok_or(AppError::Unauthorized)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AppError::Internal("system clock before unix epoch".to_string()))?
            .as_secs() as i64;
        if now >= expires_at {
            // Garbage-collect the expired row so a flood of expired
            // refresh attempts can't unbound the table size.
            db::delete_refresh_token(&conn, &token_hash)?;
            return Err(AppError::Unauthorized);
        }
        // Single-use: drop the old token before issuing the replacement
        // so a leaked refresh value cannot be redeemed twice.
        db::delete_refresh_token(&conn, &token_hash)?;
        user_id
    };

    let new_refresh = {
        let conn = state.db.lock().unwrap();
        refresh::mint_and_persist(&conn, &user_id)?
    };
    let token = jwt::create_token(&user_id, &state.jwt_secret)?;

    Ok(Json(AuthToken {
        token,
        user_id: Some(user_id.to_string()),
        refresh_token: Some(new_refresh),
        refreshed_at: None,
        server: None,
    }))
}
