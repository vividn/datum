#![doc = include_str!("../README.md")]

//! datum-sync-server — relay library + binary.
//!
//! All HTTP routes live under the `/v0` version prefix per
//! `docs/sync-protocol.md`. The relay is content-blind: it never sees
//! plaintext entries or master keys, only opaque base64 blobs and
//! JSON-encoded wrapped key bundles.
//!
//! This crate exposes the full app builder + state so integration tests
//! in sibling crates (e.g. `datum-cli/tests/sync_support`) can spawn the
//! real server in-process instead of stubbing it out.

pub mod auth;
pub mod cluster;
pub mod config;
pub mod db;
pub mod device_code;
pub mod error;
pub mod jwt;
pub mod keys;
pub mod models;
pub mod refresh;
pub mod sync;

use axum::{
    http::Method,
    middleware,
    routing::{get, post},
    Router,
};
use datum_sync_protocol::PROTOCOL_VERSION_PATH;
use rusqlite::Connection;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use url::Url;
use uuid::Uuid;
use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration, Webauthn};

use crate::error::AppError;
use crate::models::AuthenticatedUser;

pub use crate::device_code::DeviceCodeRecord;

/// Server-side TTL for in-progress passkey ceremonies. Generous bound: the
/// browser-side challenge times out in ~60s; this only caps how long an
/// abandoned ceremony may occupy memory before a sweep reclaims it.
pub const CEREMONY_TTL: Duration = Duration::from_secs(300);

/// Sweep expired entries, then insert the new one with a fresh deadline.
/// Amortizes cleanup onto the same (unauthenticated) insert path that causes
/// growth, so abandoned ceremonies can't accumulate unboundedly (da-3huo).
fn sweep_and_insert<V>(map: &Mutex<HashMap<String, (Instant, V)>>, key: String, value: V) {
    let now = Instant::now();
    let mut map = map.lock().unwrap();
    map.retain(|_, (deadline, _)| *deadline > now);
    map.insert(key, (now + CEREMONY_TTL, value));
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub webauthn: Webauthn,
    pub jwt_secret: String,
    /// In-progress passkey registration ceremonies keyed by user_id string,
    /// each paired with its sweep deadline.
    pub reg_states: Mutex<HashMap<String, (Instant, PasskeyRegistration)>>,
    /// In-progress passkey authentication ceremonies keyed by user_id string,
    /// each paired with its sweep deadline.
    pub auth_states: Mutex<HashMap<String, (Instant, PasskeyAuthentication)>>,
    /// Pending device-code grants keyed by `device_code`.
    pub device_codes: Mutex<HashMap<String, DeviceCodeRecord>>,
    /// Test-only: number of upcoming polls that should respond with
    /// `{"error":"slow_down"}` regardless of authorization. Production
    /// always sets this to 0.
    pub slow_down_polls_remaining: Mutex<usize>,
    /// Test-only: number of upcoming pushes that should be preceded by a
    /// "twin device" insert under the same device_id, advancing the
    /// authoritative next seq so the caller's prediction misses. Lets
    /// tests deterministically exercise the second-mismatch =
    /// device_id-collision path (da-ca4q). Production always sets this
    /// to 0.
    pub collide_pushes_remaining: Mutex<usize>,
}

impl AppState {
    pub fn new(conn: Connection, webauthn: Webauthn, jwt_secret: String) -> Self {
        Self {
            db: Mutex::new(conn),
            webauthn,
            jwt_secret,
            reg_states: Mutex::new(HashMap::new()),
            auth_states: Mutex::new(HashMap::new()),
            device_codes: Mutex::new(HashMap::new()),
            slow_down_polls_remaining: Mutex::new(0),
            collide_pushes_remaining: Mutex::new(0),
        }
    }
}

async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    mut req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, AppError> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    // A malformed/expired JWT is "you're not authenticated", not a
    // server-side fault; surface it as 401 so the client's refresh-on-
    // 401 retry path (Session) actually fires. Pre-da-1i5x we leaked
    // these as 500 which trapped clients in an opaque error loop.
    let claims = jwt::verify_token(token, &state.jwt_secret).map_err(|_| AppError::Unauthorized)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AppError::Unauthorized)?;

    req.extensions_mut().insert(AuthenticatedUser { user_id });
    Ok(next.run(req).await)
}

pub fn build_app(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    // Unauthenticated under /v0/auth/*: passkey ceremonies + RFC 8628 device-code.
    // /v0/auth/refresh is unauthenticated by design — it accepts only the
    // single-use refresh_token, not a (possibly-expired) bearer.
    let auth_routes = Router::new()
        .route("/register/begin", post(auth::register_begin))
        .route("/register/finish", post(auth::register_finish))
        .route("/login/begin", post(auth::login_begin))
        .route("/login/finish", post(auth::login_finish))
        .route("/refresh", post(auth::refresh))
        .route("/device-code", post(device_code::start))
        .route("/device-code/poll", get(device_code::poll))
        .route("/device-code/authorize", post(device_code::authorize));

    // Bearer-authenticated under /v0: changeset push/pull + wrapped key.
    // `/clusters/{id}/bump-generation` lives here too (da-tat1): a client
    // signals a purge / restore / rewind and the relay increments the
    // cluster's generation counter so old-generation pushes start
    // bouncing with GenerationMismatchError.
    let authed_routes = Router::new()
        .route("/push", post(sync::push))
        .route("/pull", get(sync::pull))
        .route(
            "/keys/wrapped",
            post(keys::upload_wrapped).get(keys::fetch_wrapped),
        )
        .route(
            "/clusters/:cluster_id/bump-generation",
            post(cluster::bump_generation_endpoint),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let v0 = Router::new()
        .nest("/auth", auth_routes)
        .merge(authed_routes);

    Router::new()
        .nest(&format!("/{PROTOCOL_VERSION_PATH}"), v0)
        .route("/health", get(|| async { "ok" }))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Build a default Webauthn instance for the given origin. Extracted so
/// both the binary and tests can share the wiring.
pub fn build_webauthn(origin: &Url, rp_name: &str) -> anyhow::Result<Webauthn> {
    let rp_id = origin
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid origin: no host"))?
        .to_string();
    Ok(webauthn_rs::WebauthnBuilder::new(&rp_id, origin)?
        .rp_name(rp_name)
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        let origin = Url::parse("http://localhost:3000").unwrap();
        let webauthn = build_webauthn(&origin, "datum-test").unwrap();
        Arc::new(AppState::new(conn, webauthn, "test-secret".to_string()))
    }

    fn make_test_jwt(state: &AppState) -> (Uuid, String) {
        let user_id = Uuid::new_v4();
        let conn = state.db.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username) VALUES (?1, ?2)",
            rusqlite::params![user_id.to_string(), format!("user-{}", user_id)],
        )
        .unwrap();
        drop(conn);
        let token = jwt::create_token(&user_id, &state.jwt_secret).unwrap();
        (user_id, token)
    }

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_health_check() {
        let app = build_app(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Guards that routes mount under the prefix derived from
    /// [`PROTOCOL_VERSION_PATH`], not a hardcoded `/v0` literal — a future
    /// protocol bump must move the live mount in lock-step (da-wg02).
    #[tokio::test]
    async fn test_routes_mounted_under_protocol_version_path() {
        let app = build_app(test_state());
        // A route under the derived prefix exists, so it reaches the auth
        // layer (401) rather than the router's fallback (404)...
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/{PROTOCOL_VERSION_PATH}/pull?since=0&device=test&cluster_id=test-cluster&generation=0"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // ...while the same route under a different prefix is unmounted (404),
        // proving the prefix is the constant and not accepted verbatim.
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v999/pull?since=0&device=test&cluster_id=test-cluster&generation=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_unauthorized_without_jwt() {
        let app = build_app(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/pull?since=0&device=test&cluster_id=test-cluster&generation=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_push_pull_roundtrip() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        let push_body = serde_json::json!({
            "device_id": "device-1",
            "payload_b64": "SGVsbG8gV29ybGQ",
            "cluster_id": "test-cluster",
            "generation": 0,
            "predicted_seq": 1
        });

        let push_resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(push_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(push_resp.status(), StatusCode::OK);
        let push_json = json_body(push_resp).await;
        assert_eq!(push_json["seq"], 1);

        let pull_resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/pull?since=0&device=device-2&cluster_id=test-cluster&generation=0")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pull_resp.status(), StatusCode::OK);
        let changesets: Vec<serde_json::Value> =
            serde_json::from_slice(&pull_resp.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(changesets.len(), 1);
        assert_eq!(changesets[0]["device_id"], "device-1");
        assert_eq!(changesets[0]["payload_b64"], "SGVsbG8gV29ybGQ");
        assert_eq!(changesets[0]["id"], 1);
    }

    #[tokio::test]
    async fn push_under_foreign_device_is_forbidden() {
        // da-h4te: a device_id first seen under user A cannot be reused by
        // user B — B's push fails closed with 403 instead of silently
        // writing an orphaned changeset under A's device identity.
        let state = test_state();
        let (_user_a, token_a) = make_test_jwt(&state);
        let (_user_b, token_b) = make_test_jwt(&state);

        let body = serde_json::json!({
            "device_id": "shared-device",
            "payload_b64": "SGVsbG8",
            "cluster_id": "c",
            "generation": 0,
            "predicted_seq": 1
        });

        // A registers `shared-device` by pushing.
        let resp_a = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {token_a}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_a.status(), StatusCode::OK);

        // B tries the same device_id → 403.
        let resp_b = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {token_b}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp_b.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_multi_device_integration() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        for i in 1..=3 {
            let body = serde_json::json!({
                "device_id": "device-1",
                "payload_b64": format!("Y2hhbmdlc2V0LXt9{}", i),
                "cluster_id": "test-cluster",
                "generation": 0,
                "predicted_seq": i
            });
            let resp = build_app(state.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v0/push")
                        .header("Authorization", format!("Bearer {}", token))
                        .header("Content-Type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK, "push {} failed", i);
        }

        let pull_resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/pull?since=0&device=device-2&cluster_id=test-cluster&generation=0")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pull_resp.status(), StatusCode::OK);
        let changesets: Vec<serde_json::Value> =
            serde_json::from_slice(&pull_resp.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(changesets.len(), 3, "device-2 should see 3 changesets");

        let pull_resp2 = build_app(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/pull?since=0&device=device-1&cluster_id=test-cluster&generation=0")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let self_view: Vec<serde_json::Value> =
            serde_json::from_slice(&pull_resp2.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(
            self_view.len(),
            0,
            "device-1 should not see its own changesets"
        );
    }

    #[tokio::test]
    async fn test_incremental_pull_with_since() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        for i in 1..=2 {
            let body = serde_json::json!({
                "device_id": "device-1",
                "payload_b64": format!("p{}", i),
                "cluster_id": "test-cluster",
                "generation": 0,
                "predicted_seq": i
            });
            build_app(state.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v0/push")
                        .header("Authorization", format!("Bearer {}", token))
                        .header("Content-Type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        let resp1 = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/pull?since=0&device=device-2&cluster_id=test-cluster&generation=0")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let batch1: Vec<serde_json::Value> =
            serde_json::from_slice(&resp1.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(batch1.len(), 2);

        let body = serde_json::json!({
            "device_id": "device-1",
            "payload_b64": "p3",
            "cluster_id": "test-cluster",
            "generation": 0,
            "predicted_seq": 3
        });
        build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp2 = build_app(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/pull?since=2&device=device-2&cluster_id=test-cluster&generation=0")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let batch2: Vec<serde_json::Value> =
            serde_json::from_slice(&resp2.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(batch2.len(), 1, "should only get the new changeset");
        assert_eq!(batch2[0]["payload_b64"], "p3");
    }

    /// da-3huo: an expired device-code grant is reclaimed by the next
    /// `start` call, not retained until someone happens to poll it.
    #[tokio::test]
    async fn test_device_code_start_sweeps_expired_grants() {
        let state = test_state();
        state.device_codes.lock().unwrap().insert(
            "dc-stale".to_string(),
            DeviceCodeRecord {
                user_code: "DEAD-BEEF".to_string(),
                authorized_user_id: None,
                expires_at: std::time::Instant::now() - std::time::Duration::from_secs(1),
            },
        );

        let resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/device-code")
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let map = state.device_codes.lock().unwrap();
        assert!(
            !map.contains_key("dc-stale"),
            "expired grant must be swept on the next start"
        );
        assert_eq!(map.len(), 1, "only the freshly minted grant remains");
    }

    /// da-3huo: an abandoned registration ceremony past its TTL is swept
    /// by the next `register_begin`, so distinct-username spam can't grow
    /// `reg_states` unboundedly.
    #[tokio::test]
    async fn test_register_begin_sweeps_expired_ceremonies() {
        let state = test_state();

        let begin = |username: &str| {
            let body = serde_json::json!({ "username": username }).to_string();
            build_app(state.clone()).oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/register/begin")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
        };

        // Alice begins and abandons; backdate her deadline past the TTL.
        assert_eq!(begin("alice").await.unwrap().status(), StatusCode::OK);
        {
            let mut map = state.reg_states.lock().unwrap();
            assert_eq!(map.len(), 1);
            for (deadline, _) in map.values_mut() {
                *deadline = std::time::Instant::now() - std::time::Duration::from_secs(1);
            }
        }

        // Bob's begin sweeps the expired ceremony.
        assert_eq!(begin("bob").await.unwrap().status(), StatusCode::OK);
        let map = state.reg_states.lock().unwrap();
        assert_eq!(map.len(), 1, "expired ceremony must be swept on begin");
    }

    #[tokio::test]
    async fn test_device_code_flow_end_to_end() {
        let state = test_state();

        // 1. start
        let start_resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/device-code")
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(start_resp.status(), StatusCode::OK);
        let start_json = json_body(start_resp).await;
        let device_code = start_json["device_code"].as_str().unwrap().to_string();
        assert!(!start_json["user_code"].as_str().unwrap().is_empty());

        // 2. poll while pending
        let pending = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/v0/auth/device-code/poll?device_code={}",
                        device_code
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pending.status(), StatusCode::OK);
        let pending_json = json_body(pending).await;
        assert_eq!(pending_json["status"], "pending");

        // 3. authorize
        let authz = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/device-code/authorize")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "device_code": device_code,
                            "username": "alice"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authz.status(), StatusCode::OK);

        // 4. poll again — get token
        let success = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/v0/auth/device-code/poll?device_code={}",
                        device_code
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(success.status(), StatusCode::OK);
        let tok_json = json_body(success).await;
        let token = tok_json["token"].as_str().unwrap().to_string();
        assert!(!token.is_empty());

        // The poll success branch must mint a refresh token alongside
        // the JWT so the client can rotate credentials without re-running
        // the device-code flow on first 401 (da-1i5x).
        assert!(
            tok_json["refresh_token"].is_string(),
            "device-code/poll must include refresh_token on success: {tok_json}"
        );
        assert!(
            !tok_json["refresh_token"].as_str().unwrap().is_empty(),
            "refresh_token must not be empty"
        );

        // 5. token works against /v0/pull
        let pull = build_app(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/pull?since=0&device=alice-dev&cluster_id=test-cluster&generation=0")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pull.status(), StatusCode::OK);
    }

    /// `/v0/auth/refresh` happy path: trade the refresh token issued by
    /// the device-code flow for a fresh `(token, refresh_token)` pair,
    /// then exercise the new bearer against an authed endpoint.
    #[tokio::test]
    async fn test_refresh_token_roundtrip() {
        let state = test_state();

        // Drive a device-code flow to get an initial refresh token.
        let start = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/device-code")
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let device_code = json_body(start).await["device_code"]
            .as_str()
            .unwrap()
            .to_string();

        let _ = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/device-code/authorize")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "device_code": device_code,
                            "username": "refresh-user"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let poll = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/v0/auth/device-code/poll?device_code={}",
                        device_code
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let initial = json_body(poll).await;
        let initial_refresh = initial["refresh_token"].as_str().unwrap().to_string();

        // Refresh trades the single-use token for a fresh pair.
        let refresh = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/refresh")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"refresh_token": initial_refresh.clone()}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(refresh.status(), StatusCode::OK);
        let refreshed = json_body(refresh).await;
        let new_bearer = refreshed["token"].as_str().unwrap().to_string();
        let new_refresh = refreshed["refresh_token"].as_str().unwrap().to_string();
        // The bearer is a JWT with second-resolution `exp`; the same
        // user signed within the same second yields a byte-identical
        // string, which is fine — what matters is that the refresh
        // token rotates (random bytes, so collision-free) and that the
        // new bearer authenticates.
        assert!(!new_bearer.is_empty(), "refresh must yield a bearer");
        assert_ne!(
            new_refresh, initial_refresh,
            "refresh must rotate the refresh token (single-use)"
        );

        // The new bearer authenticates an authed route.
        let pull = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(
                        "/v0/pull?since=0&device=refresh-test&cluster_id=test-cluster&generation=0",
                    )
                    .header("Authorization", format!("Bearer {}", new_bearer))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pull.status(), StatusCode::OK);
    }

    /// `/v0/auth/refresh` rejects an unknown refresh token with 401 —
    /// the same status the bearer endpoints use, so the client retry
    /// path can match on status uniformly.
    #[tokio::test]
    async fn test_refresh_token_unknown_returns_401() {
        let state = test_state();
        let resp = build_app(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/refresh")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"refresh_token": "not-a-real-token"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Single-use rotation: the same refresh token must not work twice.
    /// First call rotates and succeeds; second call against the consumed
    /// token returns 401.
    #[tokio::test]
    async fn test_refresh_token_single_use() {
        let state = test_state();

        // Mint a refresh manually so we don't depend on the full
        // device-code flow plumbing in this test.
        let user_id = {
            let conn = state.db.lock().unwrap();
            db::create_user(&conn, "single-use-user").unwrap()
        };
        let initial_refresh = {
            let conn = state.db.lock().unwrap();
            crate::refresh::mint_and_persist(&conn, &user_id).unwrap()
        };

        // First refresh succeeds.
        let resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/refresh")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"refresh_token": initial_refresh.clone()}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Second refresh with the same (now-consumed) token must 401.
        let resp = build_app(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/auth/refresh")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({"refresh_token": initial_refresh}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "consumed refresh token must not be reusable"
        );
    }

    #[tokio::test]
    async fn test_keys_wrapped_roundtrip() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        // 404 before set
        let resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/keys/wrapped")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // POST stores
        let body = serde_json::json!({
            "wrapped_key_b64": "abc",
            "kdf": "argon2id",
            "kdf_params": {"m_cost": 65536, "t_cost": 3, "p_cost": 1, "salt_b64": "s"},
            "wrap_alg": "aes-256-gcm",
            "nonce_b64": "n"
        });
        let resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/keys/wrapped")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // GET returns it
        let resp = build_app(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/keys/wrapped")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let got = json_body(resp).await;
        assert_eq!(got["wrapped_key_b64"], "abc");
        assert_eq!(got["wrap_alg"], "aes-256-gcm");
    }

    // ─── da-tat1: cluster ops + generation gating ───────────────────────────

    /// First push for an unseen cluster auto-registers it at generation
    /// 0 and stamps the response with the resolved cluster_id +
    /// generation so the client can confirm what it landed on.
    #[tokio::test]
    async fn push_auto_registers_cluster() {
        let state = test_state();
        let (user_id, token) = make_test_jwt(&state);

        let body = serde_json::json!({
            "device_id": "dev-A",
            "payload_b64": "AA",
            "cluster_id": "cluster-X",
            "generation": 0,
            "predicted_seq": 1
        });
        let resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json["cluster_id"], "cluster-X");
        assert_eq!(json["generation"], 0);

        // Cluster row exists, scoped to the calling user.
        let conn = state.db.lock().unwrap();
        let gen = cluster::get_generation_for_owner(&conn, "cluster-X", &user_id).unwrap();
        assert_eq!(gen.map(|(g, _)| g), Some(0));
    }

    /// `POST /v0/clusters/{id}/bump-generation` increments the
    /// counter and returns the new value. Locks the response shape so
    /// the client (sub-4) can parse a stable JSON.
    #[tokio::test]
    async fn bump_generation_endpoint_increments() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);
        // First push registers the cluster at gen=0.
        let _ = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "device_id": "dev-A",
                            "payload_b64": "AA",
                            "cluster_id": "cluster-Y",
                            "generation": 0,
                            "predicted_seq": 1
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/clusters/cluster-Y/bump-generation")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = json_body(resp).await;
        assert_eq!(json["cluster_id"], "cluster-Y");
        assert_eq!(json["generation"], 1);

        // A second bump goes to 2 — proves the counter is the cluster
        // row's monotonic field, not a constant.
        let resp = build_app(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/clusters/cluster-Y/bump-generation")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let json = json_body(resp).await;
        assert_eq!(json["generation"], 2);
    }

    /// Bumping a cluster owned by another user 404s. Stays
    /// indistinguishable from "doesn't exist" so the relay doesn't
    /// leak cluster existence across users.
    #[tokio::test]
    async fn bump_generation_isolated_across_users() {
        let state = test_state();
        let (alice_uid, alice_token) = make_test_jwt(&state);
        let (_, bob_token) = make_test_jwt(&state);

        // Alice creates cluster-Z by pushing.
        let _ = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {}", alice_token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "device_id": "alice-dev",
                            "payload_b64": "AA",
                            "cluster_id": "cluster-Z",
                            "generation": 0,
                            "predicted_seq": 1
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Bob can't bump it.
        let resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/clusters/cluster-Z/bump-generation")
                    .header("Authorization", format!("Bearer {}", bob_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // Alice's generation stays at 0 — Bob's attempt was a no-op.
        let conn = state.db.lock().unwrap();
        assert_eq!(
            cluster::get_generation_for_owner(&conn, "cluster-Z", &alice_uid)
                .unwrap()
                .map(|(g, _)| g),
            Some(0)
        );
    }

    /// da-1m63: cluster names are namespaced per user. Two users pushing
    /// to the same cluster name (the default-name case — names come from
    /// DB filenames, so 'notes'/'main' recur constantly) each land in
    /// their own cluster: both pushes succeed, and neither user's pull
    /// sees the other's blobs. This replaces the old global-namespace
    /// behavior where the second user 404'd forever (and, before da-k3re,
    /// 500'd with an ownership-leaking message).
    #[tokio::test]
    async fn same_named_clusters_are_independent_per_user() {
        let state = test_state();
        let (_, alice_token) = make_test_jwt(&state);
        let (_, bob_token) = make_test_jwt(&state);

        let push = |token: String, device: &'static str, payload: &'static str| {
            let state = state.clone();
            async move {
                build_app(state)
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/v0/push")
                            .header("Authorization", format!("Bearer {}", token))
                            .header("Content-Type", "application/json")
                            .body(Body::from(
                                serde_json::json!({
                                    "device_id": device,
                                    "payload_b64": payload,
                                    "cluster_id": "cluster-W",
                                    "generation": 0,
                                    "predicted_seq": 1
                                })
                                .to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap()
            }
        };

        // Alice registers cluster-W; Bob's same-named push also succeeds
        // — his own cluster-W, not a 404, not a squat.
        let resp = push(alice_token.clone(), "alice-dev", "QUxJQ0U").await;
        assert_eq!(resp.status(), StatusCode::OK);
        let resp = push(bob_token.clone(), "bob-dev", "Qk9C").await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Pulls are isolated: each user sees only their own changesets.
        let pull = |token: String, device: &'static str| {
            let state = state.clone();
            async move {
                let resp = build_app(state)
                    .oneshot(
                        Request::builder()
                            .method("GET")
                            .uri(format!(
                                "/v0/pull?since=0&device={device}&cluster_id=cluster-W&generation=0"
                            ))
                            .header("Authorization", format!("Bearer {}", token))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(resp.status(), StatusCode::OK);
                json_body(resp).await
            }
        };

        let alice_rows = pull(alice_token, "alice-dev-2").await;
        assert_eq!(alice_rows.as_array().unwrap().len(), 1);
        assert_eq!(alice_rows[0]["payload_b64"], "QUxJQ0U");

        let bob_rows = pull(bob_token, "bob-dev-2").await;
        assert_eq!(bob_rows.as_array().unwrap().len(), 1);
        assert_eq!(bob_rows[0]["payload_b64"], "Qk9C");
    }

    /// End-to-end mismatch flow per the bead's acceptance: push at
    /// gen=0, bump → server is now at gen=1, push with stale gen=0
    /// must reject with a 409 carrying the tagged
    /// `SyncProtocolError::GenerationMismatch` payload.
    #[tokio::test]
    async fn push_rejects_stale_generation_with_typed_mismatch() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        // Establish the cluster at gen=0 with an initial push.
        let _ = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "device_id": "dev-A",
                            "payload_b64": "AA",
                            "cluster_id": "cluster-G",
                            "generation": 0,
                            "predicted_seq": 1,
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Bump → server's authoritative generation is now 1.
        let _ = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/clusters/cluster-G/bump-generation")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Stale push (generation=0) — must reject 409 with the typed
        // `kind`. Clients route on `kind`, not status text.
        let resp = build_app(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "device_id": "dev-A",
                            "payload_b64": "BB",
                            "cluster_id": "cluster-G",
                            "generation": 0,
                            "predicted_seq": 2,
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let json = json_body(resp).await;
        assert_eq!(json["kind"], "generation_mismatch");
        assert_eq!(json["expected"], 1);
        assert_eq!(json["found"], 0);
    }

    /// da-ca4q: a push whose `predicted_seq` disagrees with the server's
    /// authoritative next seq is rejected with a 409 carrying the tagged
    /// `SyncProtocolError::SeqMismatch` payload — and crucially the blob
    /// is NOT stored (a later peer pull sees only the first push). The
    /// client routes on `kind` to recover in-band.
    #[tokio::test]
    async fn push_rejects_mispredicted_seq_without_storing() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        let push = |seq: i64, payload: &str| {
            let body = serde_json::json!({
                "device_id": "dev-S",
                "payload_b64": payload,
                "cluster_id": "cluster-S",
                "generation": 0,
                "predicted_seq": seq,
            })
            .to_string();
            build_app(state.clone()).oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v0/push")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
        };

        // First push lands at seq 1.
        let ok = push(1, "AA").await.unwrap();
        assert_eq!(ok.status(), StatusCode::OK);

        // Re-predicting 1 (e.g. the client never advanced its watermark
        // after the first push) is a mismatch: the server expects 2.
        let resp = push(1, "BB").await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let json = json_body(resp).await;
        assert_eq!(json["kind"], "seq_mismatch");
        assert_eq!(json["expected"], 2);
        assert_eq!(json["predicted"], 1);

        // The rejected blob must not have been stored — a peer pull sees
        // exactly the one accepted row.
        let pull = build_app(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v0/pull?since=0&device=dev-other&cluster_id=cluster-S&generation=0")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pull.status(), StatusCode::OK);
        let rows: Vec<serde_json::Value> =
            serde_json::from_slice(&pull.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(rows.len(), 1, "rejected push must not be stored");
        assert_eq!(rows[0]["payload_b64"], "AA");
    }
}
