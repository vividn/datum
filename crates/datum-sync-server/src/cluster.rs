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

/// Look up a cluster's current generation, scoped to `user_id` so a
/// caller can't probe a cluster they don't own. Returns `None` when the
/// cluster doesn't exist OR is owned by a different user — the two are
/// indistinguishable to the caller by design.
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
/// A cluster_id already claimed by a different user surfaces as
/// [`AppError::NotFound`] — the same 404 the bump endpoint returns —
/// so the response is indistinguishable from "doesn't exist" and can't
/// be used to enumerate other users' cluster names (da-k3re).
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
    // is fresh or pre-existing — and a row owned by a different user
    // must surface as a 404 to the caller, not as 0.
    // `generation_started_at` is stamped with RFC 3339 UTC at insert via
    // the table's DEFAULT (see `db::migrate_to_clusters`).
    get_generation_for_owner(conn, cluster_id, user_id)?.ok_or(AppError::NotFound)
}

/// Increment the cluster's generation by one and return the new value.
/// Scoped to `user_id`: a cluster owned by someone else stays untouched
/// and the caller gets `None`. Fresh clusters (no row) also return
/// `None` — the bump endpoint never auto-registers because doing so
/// would invent a cluster with non-zero generation, which has no
/// meaningful semantics.
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
/// Returns 404 when the cluster doesn't exist or is owned by another
/// user — the two cases are indistinguishable to the caller, by
/// design (no cross-owner cluster enumeration).
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

    /// da-k3re: registering against a cluster_id another user owns must
    /// surface as the typed NotFound — never an Internal whose message
    /// ("cluster owned by another user") would confirm the cluster exists.
    #[test]
    fn ensure_cluster_foreign_owner_is_typed_not_found() {
        let conn = setup();
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();
        let cid = "contested-cluster";
        ensure_cluster(&conn, cid, &alice).unwrap();
        match ensure_cluster(&conn, cid, &bob) {
            Err(AppError::NotFound) => {}
            other => panic!("expected AppError::NotFound, got {other:?}"),
        }
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
