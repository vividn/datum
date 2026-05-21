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

    // da-tat1: resolve the cluster the push targets. Wire-absent
    // cluster_id maps to the user's default cluster (transitional 1:1
    // mapping; the da-og24 backfill rule). The request might be the
    // first time the relay has heard about this cluster — auto-register.
    let user_id_str = user.user_id.to_string();
    let cluster_id = req.cluster_id.as_deref().unwrap_or(&user_id_str);
    let server_generation = cluster::ensure_cluster(&conn, cluster_id, &user.user_id)?;

    // Generation match — clients that send a generation must match the
    // server's authoritative value. Absent generation means a v1 client;
    // we still let it through (see `check_generation` doc).
    cluster::check_generation(server_generation, req.generation)?;

    // Server is content-blind: stores payload_b64 string opaquely (per spec §Goals).
    let (_, seq) = db::insert_changeset_into_cluster(
        &conn,
        &req.device_id,
        &req.payload_b64,
        Some(cluster_id),
        server_generation,
    )?;
    Ok(Json(PushResponse {
        seq,
        cluster_id: Some(cluster_id.to_string()),
        generation: Some(server_generation as u64),
    }))
}

#[derive(Deserialize)]
pub struct PullQuery {
    pub since: i64,
    pub device: String,
    /// da-tat1: client may pin the pull to a specific cluster. Absent
    /// means the user's default cluster (transitional). `generation`
    /// is checked against the server's authoritative value before
    /// returning rows, so a client sitting on a stale generation gets
    /// a 409 before it consumes peers' post-bump changesets.
    #[serde(default)]
    pub cluster_id: Option<String>,
    #[serde(default)]
    pub generation: Option<u64>,
}

pub async fn pull(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(q): Query<PullQuery>,
) -> Result<Json<Vec<Changeset>>, AppError> {
    let conn = state.db.lock().unwrap();

    let user_id_str = user.user_id.to_string();
    let cluster_id = q.cluster_id.as_deref().unwrap_or(&user_id_str);
    let server_generation = cluster::ensure_cluster(&conn, cluster_id, &user.user_id)?;
    cluster::check_generation(server_generation, q.generation)?;

    let rows = db::pull_changesets_for_cluster(&conn, q.since, &q.device, cluster_id)?;

    let changesets = rows
        .into_iter()
        .map(
            |(id, device_id, seq, payload_b64, uploaded_at, cluster_id, generation)| Changeset {
                id,
                device_id,
                seq,
                payload_b64,
                uploaded_at,
                cluster_id: Some(cluster_id),
                generation: Some(generation as u64),
            },
        )
        .collect();

    Ok(Json(changesets))
}
