use axum::{
    extract::{Extension, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    db::{self, Changeset, Device},
    error::{AppError, AppResult},
    AuthUser, AppState,
};

// GET /devices
pub async fn list_devices(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthUser>,
) -> AppResult<Json<Vec<Device>>> {
    let devices = db::get_devices_for_user(&state.db, &auth.user_id)?;
    Ok(Json(devices))
}

// POST /devices
#[derive(Deserialize)]
pub struct AddDeviceReq {
    pub device_id: String,
    pub label: Option<String>,
}

pub async fn add_device(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<AddDeviceReq>,
) -> AppResult<Json<serde_json::Value>> {
    let label = req.label.unwrap_or_default();
    db::register_device(&state.db, &req.device_id, &auth.user_id, &label)?;
    Ok(Json(serde_json::json!({ "status": "ok", "device_id": req.device_id })))
}

// POST /sync/push
#[derive(Deserialize)]
pub struct PushReq {
    pub device_id: String,
    pub payload: String,
}

#[derive(Serialize)]
pub struct PushResp {
    pub seq: i64,
}

pub async fn push(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<PushReq>,
) -> AppResult<Json<PushResp>> {
    if !db::device_belongs_to_user(&state.db, &req.device_id, &auth.user_id)? {
        return Err(AppError::Auth(
            "Device does not belong to this user".into(),
        ));
    }

    let seq = db::next_seq_for_device(&state.db, &req.device_id)?;
    db::push_changeset(&state.db, &req.device_id, seq, &req.payload)?;
    Ok(Json(PushResp { seq }))
}

// GET /sync/pull?since=<seq>&device=<id>
#[derive(Deserialize)]
pub struct PullQuery {
    pub since: Option<i64>,
    pub device: String,
}

pub async fn pull(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<PullQuery>,
) -> AppResult<Json<Vec<Changeset>>> {
    if !db::device_belongs_to_user(&state.db, &query.device, &auth.user_id)? {
        return Err(AppError::Auth(
            "Device does not belong to this user".into(),
        ));
    }

    let since = query.since.unwrap_or(0);
    let changesets = db::pull_changesets(&state.db, &auth.user_id, &query.device, since)?;
    Ok(Json(changesets))
}
