use axum::{
    extract::{Query, State},
    Extension, Json,
};
use datum_sync_protocol::SyncProtocolError;
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    cluster, db,
    error::AppError,
    models::{AuthenticatedUser, Changeset, PullResponse, PushRequest, PushResponse},
    AppState,
};

pub async fn push(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>, AppError> {
    let conn = state.db.lock().unwrap();

    // da-h4te: reject a push submitted under a device_id owned by another
    // user. Without this guard, `ensure_device`'s INSERT OR IGNORE silently
    // no-ops on a foreign device_id and the changeset lands orphaned; fail
    // closed (403) so a caller can only write under devices it owns.
    if let Some(owner) = db::device_owner(&conn, &req.device_id)? {
        if owner != user.user_id.to_string() {
            return Err(AppError::Forbidden(
                "device_id is registered to another user".to_string(),
            ));
        }
    }

    db::ensure_device(&conn, &req.device_id, &user.user_id)?;

    // da-tat1: resolve the cluster the push targets. The request might be
    // the first time the relay has heard about this cluster — auto-register.
    // `gen_started_at` is the timestamp the current generation was first
    // set; we pass it into `check_generation` so a mismatch error carries
    // it (clients use it to bucket pre/post-reinit local edits, da-74xx).
    let (server_generation, gen_started_at) =
        cluster::ensure_cluster(&conn, &req.cluster_id, &user.user_id)?;

    // Generation match — the wire-required generation must match the
    // server's authoritative value.
    cluster::check_generation(server_generation, req.generation, &gen_started_at)?;

    // Test-only collision twin: while the counter is >0, simulate a second
    // device sharing this device_id pushing just ahead of every attempt —
    // insert a row under the same device_id so the authoritative next seq
    // advances and the prediction check below misses. Production always
    // leaves this at 0 so the branch is never taken.
    {
        let mut remaining = state.collide_pushes_remaining.lock().unwrap();
        if *remaining > 0 {
            *remaining -= 1;
            db::insert_changeset_into_cluster(
                &conn,
                &req.device_id,
                "Y29sbGlzaW9uLXR3aW4", // opaque junk; server is content-blind
                Some(&req.cluster_id),
                server_generation,
            )?;
        }
    }

    // Seq match (da-ca4q) — the client predicts its per-device seq and
    // binds it into the changeset AAD at encrypt time. Validate that
    // prediction against our authoritative next seq BEFORE storing, so a
    // row whose AAD seq disagrees with its stored seq never lands (peers
    // could not decrypt it). A disagreement is recoverable, not fatal:
    // we hand back `expected` and the client re-bases, re-encrypts, and
    // retries. Holding the db lock for the whole handler makes this peek
    // and the insert below observe the same next seq.
    //
    // Scoped per (device_id, cluster_id), matching the client's per-DB
    // `device_seq` counter and the AAD's cluster binding (da-hu5y) —
    // a device-global seq would mismatch on every alternating-DB push
    // from a multi-DB machine.
    let expected_seq = db::peek_next_seq(&conn, &req.device_id, Some(&req.cluster_id))?;
    if req.predicted_seq != expected_seq {
        return Err(AppError::SeqMismatch(SyncProtocolError::SeqMismatch {
            expected: expected_seq,
            predicted: req.predicted_seq,
        }));
    }

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
    /// Max rows in the response (da-5ua6). Absent → [`MAX_PULL_ROWS`], so
    /// a client that omits it still can't ask the relay to serialise a
    /// whole history into one body. Out of range → 400, so an over-ask is
    /// answered rather than silently downgraded (da-blcb).
    pub limit: Option<i64>,
}

/// Ceiling on rows per `GET /v0/pull` response, and the value used when a
/// client sends no `limit`. Defined once in the wire contract — see
/// [`datum_sync_protocol::messages::sync::MAX_PULL_ROWS`] for why both
/// sides must read the same number.
pub const MAX_PULL_ROWS: i64 = datum_sync_protocol::messages::sync::MAX_PULL_ROWS as i64;

pub async fn pull(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(q): Query<PullQuery>,
) -> Result<Json<PullResponse>, AppError> {
    let conn = state.db.lock().unwrap();

    let (server_generation, gen_started_at) =
        cluster::ensure_cluster(&conn, &q.cluster_id, &user.user_id)?;
    cluster::check_generation(server_generation, q.generation, &gen_started_at)?;

    // A clamp that silently substitutes turns a client's over-ask into a
    // false end-of-stream: the client sizes its paging loop by the limit
    // it sent, sees a shorter page, and concludes the history is done
    // (da-blcb). Answering the over-ask instead keeps the disagreement
    // loud and local to the request that made it.
    let limit = match q.limit {
        None => MAX_PULL_ROWS,
        Some(n) if (1..=MAX_PULL_ROWS).contains(&n) => n,
        Some(n) => {
            return Err(AppError::BadRequest(format!(
                "limit must be between 1 and {MAX_PULL_ROWS}, got {n}"
            )))
        }
    };
    // da-1m63: cluster names are per-user, so the pull is scoped to the
    // caller's devices as well as the cluster — two users' same-named
    // clusters never see each other's rows.
    let rows = db::pull_changesets_for_cluster(
        &conn,
        q.since,
        &q.device,
        &q.cluster_id,
        &user.user_id,
        limit,
    )?;

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

    // da-od2t: state what we still hold for the *caller's* device. A pull
    // is the one call every client makes whether or not it has anything to
    // send, so it is where a client with a full push watermark and nothing
    // pending can still learn that this relay has lost its history — a
    // replaced server, or one restored from a backup predating those
    // pushes. `peek_next_seq` is the authoritative next value; the highest
    // held is one below it, and 0 when we hold none.
    let device_high_seq = db::peek_next_seq(&conn, &q.device, Some(&q.cluster_id))? - 1;

    Ok(Json(PullResponse {
        device_high_seq,
        changesets,
    }))
}
