//! datum-sync-server — relay library + binary.
//!
//! All HTTP routes live under the `/v1` version prefix per
//! `docs/sync-protocol.md`. The relay is content-blind: it never sees
//! plaintext entries or master keys, only opaque base64 blobs and
//! JSON-encoded wrapped key bundles.
//!
//! This crate exposes the full app builder + state so integration tests
//! in sibling crates (e.g. `datum-cli/tests/sync_support`) can spawn the
//! real server in-process instead of stubbing it out.

pub mod auth;
pub mod db;
pub mod device_code;
pub mod error;
pub mod jwt;
pub mod keys;
pub mod models;
pub mod sync;

use axum::{
    http::Method,
    middleware,
    routing::{get, post},
    Router,
};
use rusqlite::Connection;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use url::Url;
use uuid::Uuid;
use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration, Webauthn};

use crate::error::AppError;
use crate::models::AuthenticatedUser;

pub use crate::device_code::DeviceCodeRecord;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub webauthn: Webauthn,
    pub jwt_secret: String,
    /// In-progress passkey registration ceremonies keyed by user_id string.
    pub reg_states: Mutex<HashMap<String, PasskeyRegistration>>,
    /// In-progress passkey authentication ceremonies keyed by user_id string.
    pub auth_states: Mutex<HashMap<String, PasskeyAuthentication>>,
    /// Pending device-code grants keyed by `device_code`.
    pub device_codes: Mutex<HashMap<String, DeviceCodeRecord>>,
    /// Test-only: number of upcoming polls that should respond with
    /// `{"error":"slow_down"}` regardless of authorization. Production
    /// always sets this to 0.
    pub slow_down_polls_remaining: Mutex<usize>,
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

    let claims = jwt::verify_token(token, &state.jwt_secret)?;
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AppError::Unauthorized)?;

    req.extensions_mut().insert(AuthenticatedUser { user_id });
    Ok(next.run(req).await)
}

pub fn build_app(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    // Unauthenticated under /v1/auth/*: passkey ceremonies + RFC 8628 device-code.
    let auth_routes = Router::new()
        .route("/register/begin", post(auth::register_begin))
        .route("/register/finish", post(auth::register_finish))
        .route("/login/begin", post(auth::login_begin))
        .route("/login/finish", post(auth::login_finish))
        .route("/device-code", post(device_code::start))
        .route("/device-code/poll", get(device_code::poll))
        .route("/device-code/authorize", post(device_code::authorize));

    // Bearer-authenticated under /v1: changeset push/pull + wrapped key.
    let authed_routes = Router::new()
        .route("/push", post(sync::push))
        .route("/pull", get(sync::pull))
        .route(
            "/keys/wrapped",
            post(keys::upload_wrapped).get(keys::fetch_wrapped),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let v1 = Router::new()
        .nest("/auth", auth_routes)
        .merge(authed_routes);

    Router::new()
        .nest("/v1", v1)
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
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_unauthorized_without_jwt() {
        let app = build_app(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/pull?since=0&device=test")
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
            "payload_b64": "SGVsbG8gV29ybGQ"
        });

        let push_resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/push")
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
                    .uri("/v1/pull?since=0&device=device-2")
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
    async fn test_multi_device_integration() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        for i in 1..=3 {
            let body = serde_json::json!({
                "device_id": "device-1",
                "payload_b64": format!("Y2hhbmdlc2V0LXt9{}", i)
            });
            let resp = build_app(state.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/push")
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
                    .uri("/v1/pull?since=0&device=device-2")
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
                    .uri("/v1/pull?since=0&device=device-1")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let self_view: Vec<serde_json::Value> =
            serde_json::from_slice(&pull_resp2.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(self_view.len(), 0, "device-1 should not see its own changesets");
    }

    #[tokio::test]
    async fn test_incremental_pull_with_since() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        for i in 1..=2 {
            let body = serde_json::json!({
                "device_id": "device-1",
                "payload_b64": format!("p{}", i)
            });
            build_app(state.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/v1/push")
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
                    .uri("/v1/pull?since=0&device=device-2")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let batch1: Vec<serde_json::Value> =
            serde_json::from_slice(&resp1.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(batch1.len(), 2);

        let body = serde_json::json!({"device_id": "device-1", "payload_b64": "p3"});
        build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/push")
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
                    .uri("/v1/pull?since=2&device=device-2")
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

    #[tokio::test]
    async fn test_device_code_flow_end_to_end() {
        let state = test_state();

        // 1. start
        let start_resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/auth/device-code")
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
                    .uri(format!("/v1/auth/device-code/poll?device_code={}", device_code))
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
                    .uri("/v1/auth/device-code/authorize")
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
                    .uri(format!("/v1/auth/device-code/poll?device_code={}", device_code))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(success.status(), StatusCode::OK);
        let tok_json = json_body(success).await;
        let token = tok_json["token"].as_str().unwrap().to_string();
        assert!(!token.is_empty());

        // 5. token works against /v1/pull
        let pull = build_app(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/pull?since=0&device=alice-dev")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pull.status(), StatusCode::OK);
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
                    .uri("/v1/keys/wrapped")
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
                    .uri("/v1/keys/wrapped")
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
                    .uri("/v1/keys/wrapped")
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
}
