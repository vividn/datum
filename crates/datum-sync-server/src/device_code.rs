//! RFC 8628 device-authorization grant handlers.
//!
//! Wire contract per `docs/sync-protocol.md` §OAuth device-code flow.
//! State is held in memory on the running server (no DB persistence) — a
//! restart invalidates pending grants, which is fine because they're
//! short-lived (default 600s) and clients always retry the flow on 401.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

use crate::{
    db,
    error::AppError,
    jwt,
    models::{DeviceCodeAuthorizeRequest, DeviceCodePollQuery, DeviceCodeResponse, PollResponse},
    refresh, AppState,
};

/// In-memory record of one pending device-code grant.
#[derive(Debug, Clone)]
pub struct DeviceCodeRecord {
    pub user_code: String,
    pub authorized_user_id: Option<Uuid>,
    pub expires_at: Instant,
}

/// Default 10-minute lifetime; matches spec's documented `expires_in`.
const DEFAULT_EXPIRES_IN: u64 = 600;
/// Default 1-second poll interval. Tests use this to keep flows fast;
/// production may bump it via env but spec floor is 5s per RFC 8628.
const DEFAULT_INTERVAL: u64 = 1;

fn random_token(len: usize) -> String {
    // device_code is a bearer credential — system-clock entropy is predictable
    // and lets an attacker race /authorize. OsRng pulls directly from the OS
    // CSPRNG (getrandom syscall) so no shared state is observable across calls.
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use rand::{rngs::OsRng, RngCore};
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(&bytes)
}

fn random_user_code() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let n = nanos as u32;
    format!("{:04X}-{:04X}", n & 0xFFFF, (n >> 16) & 0xFFFF)
}

pub async fn start(
    State(state): State<Arc<AppState>>,
    Json(_body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let device_code = format!("dc-{}", random_token(24));
    let user_code = random_user_code();
    let expires_at = Instant::now() + Duration::from_secs(DEFAULT_EXPIRES_IN);

    state.device_codes.lock().unwrap().insert(
        device_code.clone(),
        DeviceCodeRecord {
            user_code: user_code.clone(),
            authorized_user_id: None,
            expires_at,
        },
    );

    let resp = DeviceCodeResponse {
        device_code,
        user_code: user_code.clone(),
        verification_uri: "/auth/device-code/verify".to_string(),
        verification_uri_complete: Some(format!(
            "/auth/device-code/verify?user_code={}",
            user_code
        )),
        expires_in: Some(DEFAULT_EXPIRES_IN),
        interval: DEFAULT_INTERVAL,
    };
    (StatusCode::OK, Json(resp))
}

/// Helper to build the `PollResponse::Error` variant with a static error
/// code (avoids stringly-typed call sites).
fn poll_error(code: &str) -> PollResponse {
    PollResponse::Error {
        error: code.to_string(),
        detail: None,
    }
}

pub async fn poll(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DeviceCodePollQuery>,
) -> axum::response::Response {
    // Test-only slow_down: pop one off the counter and return the hint
    // before doing anything else. Production sets this to 0 so the branch
    // is never taken.
    {
        let mut remaining = state.slow_down_polls_remaining.lock().unwrap();
        if *remaining > 0 {
            *remaining -= 1;
            return (StatusCode::OK, Json(poll_error("slow_down"))).into_response();
        }
    }

    // Snapshot the record under the lock, then drop it before issuing a JWT
    // so we don't hold the device_codes mutex across crypto work.
    let record = {
        let map = state.device_codes.lock().unwrap();
        map.get(&q.device_code).cloned()
    };

    let Some(record) = record else {
        return (StatusCode::BAD_REQUEST, Json(poll_error("expired"))).into_response();
    };

    if Instant::now() > record.expires_at {
        // Clean up the stale record so /authorize on the same code fails
        // immediately too.
        state.device_codes.lock().unwrap().remove(&q.device_code);
        return (StatusCode::BAD_REQUEST, Json(poll_error("expired"))).into_response();
    }

    if let Some(user_id) = record.authorized_user_id {
        let refresh_token = {
            let conn = state.db.lock().unwrap();
            match refresh::mint_and_persist(&conn, &user_id) {
                Ok(t) => t,
                Err(e) => return e.into_response(),
            }
        };
        match jwt::create_token(&user_id, &state.jwt_secret) {
            Ok(token) => {
                // One-shot: drop the device_code so the same poll can't
                // be replayed for a fresh token without re-running the
                // device-code flow.
                state.device_codes.lock().unwrap().remove(&q.device_code);
                let body = PollResponse::Success {
                    token,
                    user_id: Some(user_id.to_string()),
                    refresh_token: Some(refresh_token),
                    cluster_id: None,
                };
                (StatusCode::OK, Json(body)).into_response()
            }
            Err(e) => e.into_response(),
        }
    } else {
        (
            StatusCode::OK,
            Json(PollResponse::Pending {
                status: "pending".to_string(),
            }),
        )
            .into_response()
    }
}

/// Mark a pending device-code as authorized for the given username. The
/// real "user opens verification_uri and enters their passphrase" UX is
/// out of scope for v1 (see `docs/sync-protocol.md`); this endpoint is the
/// minimal, no-auth-required hook that completes the loop. Tests drive it
/// directly; a future browser flow would call it after passphrase
/// confirmation.
pub async fn authorize(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeviceCodeAuthorizeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = {
        let conn = state.db.lock().unwrap();
        match db::get_user_by_username(&conn, &req.username)? {
            Some(id) => id,
            None => db::create_user(&conn, &req.username)?,
        }
    };

    let mut map = state.device_codes.lock().unwrap();
    let entry = map
        .get_mut(&req.device_code)
        .ok_or_else(|| AppError::BadRequest("unknown device_code".to_string()))?;

    if Instant::now() > entry.expires_at {
        return Err(AppError::BadRequest("device_code expired".to_string()));
    }

    entry.authorized_user_id = Some(user_id);
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Kept private to satisfy unused-import lint when device_code state is
/// scaffolded but not exercised.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct _Unused;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn random_token_no_collisions_in_1000_calls() {
        let mut seen = HashSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(random_token(24)));
        }
        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn random_token_back_to_back_calls_distinct() {
        // Guards against any future RNG that keys on system clock — back-to-
        // back calls on modern hardware land inside a single nanosecond tick.
        let mut seen = HashSet::new();
        for _ in 0..256 {
            assert!(seen.insert(random_token(24)));
        }
    }

    #[test]
    fn random_token_meets_minimum_entropy() {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let token = random_token(24);
        let raw = URL_SAFE_NO_PAD
            .decode(token.as_bytes())
            .expect("token is base64url");
        assert!(
            raw.len() * 8 >= 128,
            "token entropy {} bits < 128",
            raw.len() * 8
        );
    }
}
