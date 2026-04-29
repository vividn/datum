use axum::{extract::State, Json};
use std::sync::Arc;
use uuid::Uuid;
use crate::{
    db, error::AppError, jwt,
    models::{
        AuthToken, LoginBeginRequest, LoginBeginResponse, LoginFinishRequest,
        RegisterBeginRequest, RegisterBeginResponse, RegisterFinishRequest,
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

    {
        let conn = state.db.lock().unwrap();
        db::save_credential(&conn, &user_id, &passkey)?;
    }

    let token = jwt::create_token(&user_id, &state.jwt_secret)?;
    Ok(Json(AuthToken {
        token,
        user_id: user_id.to_string(),
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

    // Update passkey counter for the used credential
    {
        let conn = state.db.lock().unwrap();
        let mut passkeys = db::get_passkeys(&conn, &user_id)?;
        for pk in passkeys.iter_mut() {
            if pk.update_credential(&auth_result).is_some() {
                db::save_credential(&conn, &user_id, pk)?;
                break;
            }
        }
    }

    let token = jwt::create_token(&user_id, &state.jwt_secret)?;
    Ok(Json(AuthToken {
        token,
        user_id: user_id.to_string(),
    }))
}
