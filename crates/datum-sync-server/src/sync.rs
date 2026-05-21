use axum::{
    extract::{Query, State},
    Extension, Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    db,
    error::AppError,
    models::{AuthenticatedUser, Changeset, PushRequest, PushResponse},
    AppState,
};

pub async fn push(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>, AppError> {
    let conn = state.db.lock().unwrap();

    db::ensure_device(&conn, &req.device_id, &user.user_id)?;

    // Server is content-blind: stores payload_b64 string opaquely (per spec §Goals).
    let (_, seq) = db::insert_changeset(&conn, &req.device_id, &req.payload_b64)?;
    Ok(Json(PushResponse {
        seq,
        cluster_id: None,
        generation: None,
    }))
}

#[derive(Deserialize)]
pub struct PullQuery {
    pub since: i64,
    pub device: String,
}

pub async fn pull(
    State(state): State<Arc<AppState>>,
    Extension(_user): Extension<AuthenticatedUser>,
    Query(q): Query<PullQuery>,
) -> Result<Json<Vec<Changeset>>, AppError> {
    let conn = state.db.lock().unwrap();
    let rows = db::pull_changesets(&conn, q.since, &q.device)?;

    let changesets = rows
        .into_iter()
        .map(|(id, device_id, seq, payload_b64, uploaded_at)| Changeset {
            id,
            device_id,
            seq,
            payload_b64,
            uploaded_at,
            cluster_id: None,
            generation: None,
        })
        .collect();

    Ok(Json(changesets))
}
