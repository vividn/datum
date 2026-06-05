//! Server-side cluster ops (da-tat1, sub-3 of multi-DB epic da-u6lm).
//!
//! Three pieces sit here:
//!   1. `ensure_cluster` — auto-register a cluster row on first sync, owner-
//!      pinned to the calling user.
//!   2. `bump_generation` — increment the cluster's generation counter (the
//!      `POST /v0/clusters/{id}/bump-generation` handler calls this when a
//!      client signals purge / restore / rewind).
//!   3. `check_generation` — gate a sync request against the server's
//!      authoritative value; mismatches surface as
//!      [`SyncProtocolError::GenerationMismatch`].
//!
//! Sub-2 (da-og24) created the `clusters` table; this module is where the
//! row first gets written for a brand-new cluster_id.

use anyhow::{Context, Result};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use datum_sync_protocol::SyncProtocolError;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::{error::AppError, models::AuthenticatedUser, AppState};

/// Look up a cluster's current generation. Cluster names are
/// namespaced per user (composite PK, da-1m63), so the `user_id`
/// filter selects the caller's own row; another user's same-named
/// cluster is a different row and never visible here. Returns `None`
/// when the caller has no cluster by that name.
pub fn get_generation_for_owner(
    conn: &Connection,
    cluster_id: &str,
    user_id: &Uuid,
) -> Result<Option<(i64, String)>> {
    let mut stmt = conn.prepare(
        "SELECT generation, generation_started_at FROM clusters \
         WHERE cluster_id = ?1 AND user_id = ?2",
    )?;
    let mut rows = stmt.query(params![cluster_id, user_id.to_string()])?;
    if let Some(row) = rows.next()? {
        Ok(Some((row.get(0)?, row.get(1)?)))
    } else {
        Ok(None)
    }
}

/// Idempotent cluster registration. `INSERT OR IGNORE` means a
/// concurrent first-push from two devices on the same cluster lands
/// exactly one row at generation 0. Returns the generation that was in
/// effect after the call (existing value if the row was already there,
/// 0 for a fresh insert).
///
/// Cluster names are namespaced per user (composite PK, da-1m63): two
/// users registering the same name get independent rows, so default
/// names derived from DB filenames ('notes', 'main', …) never collide
/// across users and name-squatting is impossible. The `NotFound` arm
/// below is therefore unreachable in practice — kept as a defensive
/// guard against the re-read racing a row deletion.
pub fn ensure_cluster(
    conn: &Connection,
    cluster_id: &str,
    user_id: &Uuid,
) -> Result<(i64, String), AppError> {
    conn.execute(
        "INSERT OR IGNORE INTO clusters (cluster_id, user_id, generation) VALUES (?1, ?2, 0)",
        params![cluster_id, user_id.to_string()],
    )?;
    // Re-read because INSERT OR IGNORE doesn't tell us whether the row
    // is fresh or pre-existing.
    // `generation_started_at` is stamped with RFC 3339 UTC at insert via
    // the table's DEFAULT (see `db::migrate_to_clusters`).
    get_generation_for_owner(conn, cluster_id, user_id)?.ok_or(AppError::NotFound)
}

/// Increment the cluster's generation by one and return the new value.
/// Scoped to `user_id`: another user's same-named cluster is a
/// different row (per-user namespace) and stays untouched. Fresh
/// clusters (no row for this user) return `None` — the bump endpoint
/// never auto-registers because doing so would invent a cluster with
/// non-zero generation, which has no meaningful semantics.
pub fn bump_generation(
    conn: &Connection,
    cluster_id: &str,
    user_id: &Uuid,
) -> Result<Option<(i64, String)>> {
    // Stamp `generation_started_at` in RFC 3339 UTC so the wire form
    // (carried in the GenerationMismatch error) parses cleanly client-side
    // — see the long comment on the CREATE TABLE in `db::migrate_to_clusters`.
    let rows = conn
        .execute(
            "UPDATE clusters
             SET generation = generation + 1,
                 generation_started_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE cluster_id = ?1 AND user_id = ?2",
            params![cluster_id, user_id.to_string()],
        )
        .context("bump_generation")?;
    if rows == 0 {
        return Ok(None);
    }
    get_generation_for_owner(conn, cluster_id, user_id)
}

/// Compare a client-supplied generation against the server's
/// authoritative value. Returns `Err(AppError::GenerationMismatch { ... })`
/// on a mismatch so the caller can `?` it straight out of the handler.
pub fn check_generation(server: i64, client: u64, started_at: &str) -> Result<(), AppError> {
    if (server as u64) == client {
        Ok(())
    } else {
        Err(AppError::GenerationMismatch(
            SyncProtocolError::GenerationMismatch {
                expected: server as u64,
                found: client,
                generation_started_at: started_at.to_string(),
            },
        ))
    }
}

// ─── HTTP handler ────────────────────────────────────────────────────────────

/// Response body for `POST /v0/clusters/{cluster_id}/bump-generation`.
/// Echoes the cluster_id back so a client that fired against a
/// templated path still sees the canonical value.
#[derive(Debug, Serialize)]
pub struct BumpResponse {
    pub cluster_id: String,
    pub generation: i64,
}

/// `POST /v0/clusters/{cluster_id}/bump-generation`. Client signals a
/// purge / restore / rewind; the relay increments the cluster's
/// generation so subsequent pushes carrying the old generation are
/// rejected with [`SyncProtocolError::GenerationMismatch`].
///
/// Returns 404 when the caller has no cluster by that name. Cluster
/// names are namespaced per user (da-1m63), so another user's
/// same-named cluster is invisible here.
pub async fn bump_generation_endpoint(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(cluster_id): Path<String>,
) -> Result<Json<BumpResponse>, AppError> {
    let conn = state.db.lock().unwrap();
    match bump_generation(&conn, &cluster_id, &user.user_id)? {
        Some((generation, _started_at)) => Ok(Json(BumpResponse {
            cluster_id,
            generation,
        })),
        None => Err(AppError::NotFound),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn ensure_cluster_inserts_at_generation_zero_then_is_idempotent() {
        let conn = setup();
        let user = Uuid::new_v4();
        let cid = "cluster-A";
        let (gen, started_at) = ensure_cluster(&conn, cid, &user).unwrap();
        assert_eq!(gen, 0);
        // RFC 3339 UTC: `YYYY-MM-DDTHH:MM:SSZ`. SQLite's `datetime('now')`
        // returns a non-RFC-3339 form (`YYYY-MM-DD HH:MM:SS`) and would
        // fail this check — surfacing the bug Copilot caught on the
        // initial PR pass.
        assert!(
            started_at.contains('T') && started_at.ends_with('Z'),
            "generation_started_at must be RFC 3339 (`...T...Z`), got: {started_at}"
        );
        // Second call must NOT bump — registration is a no-op when the
        // row already exists.
        let (gen2, _) = ensure_cluster(&conn, cid, &user).unwrap();
        assert_eq!(gen2, 0);
    }

    #[test]
    fn bump_generation_increments_and_returns_new_value() {
        let conn = setup();
        let user = Uuid::new_v4();
        let cid = "cluster-B";
        ensure_cluster(&conn, cid, &user).unwrap();
        assert_eq!(
            bump_generation(&conn, cid, &user).unwrap().map(|(g, _)| g),
            Some(1)
        );
        assert_eq!(
            bump_generation(&conn, cid, &user).unwrap().map(|(g, _)| g),
            Some(2)
        );
        assert_eq!(
            get_generation_for_owner(&conn, cid, &user)
                .unwrap()
                .map(|(g, _)| g),
            Some(2)
        );
    }

    #[test]
    fn bump_generation_on_missing_cluster_returns_none() {
        let conn = setup();
        let user = Uuid::new_v4();
        assert!(bump_generation(&conn, "never-existed", &user)
            .unwrap()
            .is_none());
    }

    /// da-1m63: cluster names are namespaced per user. Two users
    /// registering the same name (the default-name case — 'notes',
    /// 'main' come from DB filenames) get independent rows with
    /// independent generations; neither errors, neither squats.
    #[test]
    fn ensure_cluster_same_name_is_independent_per_user() {
        let conn = setup();
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();
        let cid = "notes";
        let (alice_gen, _) = ensure_cluster(&conn, cid, &alice).unwrap();
        let (bob_gen, _) = ensure_cluster(&conn, cid, &bob).unwrap();
        assert_eq!(alice_gen, 0);
        assert_eq!(bob_gen, 0);

        // Bumping Alice's 'notes' leaves Bob's 'notes' untouched.
        assert_eq!(
            bump_generation(&conn, cid, &alice).unwrap().map(|(g, _)| g),
            Some(1)
        );
        assert_eq!(
            get_generation_for_owner(&conn, cid, &bob)
                .unwrap()
                .map(|(g, _)| g),
            Some(0)
        );
    }

    #[test]
    fn bump_generation_isolates_across_owners() {
        let conn = setup();
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();
        let cid = "alice-cluster";
        ensure_cluster(&conn, cid, &alice).unwrap();

        // Bob can neither read nor bump Alice's cluster.
        assert!(get_generation_for_owner(&conn, cid, &bob)
            .unwrap()
            .is_none());
        assert!(bump_generation(&conn, cid, &bob).unwrap().is_none());

        // Alice's view is untouched.
        assert_eq!(
            get_generation_for_owner(&conn, cid, &alice)
                .unwrap()
                .map(|(g, _)| g),
            Some(0)
        );
    }

    #[test]
    fn check_generation_passes_on_match() {
        check_generation(3, 3, "2026-06-01T00:00:00Z").unwrap();
    }

    #[test]
    fn check_generation_rejects_with_typed_mismatch() {
        let err = check_generation(5, 3, "2026-06-01T12:00:00Z").unwrap_err();
        match err {
            AppError::GenerationMismatch(SyncProtocolError::GenerationMismatch {
                expected,
                found,
                generation_started_at,
            }) => {
                assert_eq!(expected, 5);
                assert_eq!(found, 3);
                assert_eq!(generation_started_at, "2026-06-01T12:00:00Z");
            }
            other => panic!("expected GenerationMismatch variant, got {other:?}"),
        }
    }
}
