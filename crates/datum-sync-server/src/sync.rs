use axum::{
    extract::{Query, State},
    Extension, Json,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    cluster, db,
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

    // da-tat1: resolve the cluster the push targets. The request might be
    // the first time the relay has heard about this cluster — auto-register.
    let server_generation = cluster::ensure_cluster(&conn, &req.cluster_id, &user.user_id)?;

    // Generation match — the wire-required generation must match the
    // server's authoritative value.
    cluster::check_generation(server_generation, req.generation)?;

    // Server is content-blind: stores payload_b64 string opaquely (per spec §Goals).
    let (_, seq) = db::insert_changeset_into_cluster(
        &conn,
        &req.device_id,
        &req.payload_b64,
        Some(&req.cluster_id),
        server_generation,
    )?;
    Ok(Json(PushResponse {
        seq,
        cluster_id: req.cluster_id,
        generation: server_generation as u64,
    }))
}

#[derive(Deserialize)]
pub struct PullQuery {
    pub since: i64,
    pub device: String,
    /// Cluster the pull is scoped to. `generation` is checked against
    /// the server's authoritative value before returning rows, so a
    /// client sitting on a stale generation gets a 409 before it
    /// consumes peers' post-bump changesets.
    pub cluster_id: String,
    pub generation: u64,
}

pub async fn pull(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(q): Query<PullQuery>,
) -> Result<Json<Vec<Changeset>>, AppError> {
    let conn = state.db.lock().unwrap();

    let server_generation = cluster::ensure_cluster(&conn, &q.cluster_id, &user.user_id)?;
    cluster::check_generation(server_generation, q.generation)?;

    let rows = db::pull_changesets_for_cluster(&conn, q.since, &q.device, &q.cluster_id)?;

    let changesets = rows
        .into_iter()
        .map(
            |(id, device_id, seq, payload_b64, uploaded_at, cluster_id, generation)| Changeset {
                id,
                device_id,
                seq,
                payload_b64,
                uploaded_at,
                cluster_id,
                generation: generation as u64,
            },
        )
        .collect();

    Ok(Json(changesets))
}
