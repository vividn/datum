//! `/v0/keys/wrapped` — store and retrieve the passphrase-wrapped master
//! key blob per `docs/sync-protocol.md` §Wrapped key exchange.
//!
//! The relay is content-blind about the inner shape; it just round-trips
//! the JSON body keyed by authenticated user.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};
use std::sync::Arc;

use crate::{db, error::AppError, models::AuthenticatedUser, AppState};

pub async fn upload_wrapped(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let blob = serde_json::to_string(&body)?;
    let conn = state.db.lock().unwrap();
    db::upsert_wrapped_key(&conn, &user.user_id, &blob)?;
    Ok(Json(serde_json::json!({})))
}

pub async fn fetch_wrapped(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<axum::response::Response, AppError> {
    let conn = state.db.lock().unwrap();
    match db::get_wrapped_key(&conn, &user.user_id)? {
        Some(blob) => {
            let value: serde_json::Value = serde_json::from_str(&blob)?;
            Ok((StatusCode::OK, Json(value)).into_response())
        }
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "not_found" })),
        )
            .into_response()),
    }
}
