mod auth;
mod db;
mod error;
mod jwt;
mod models;
mod sync;

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

pub struct AppState {
    pub db: Mutex<Connection>,
    pub webauthn: Webauthn,
    pub jwt_secret: String,
    /// In-progress registration ceremonies keyed by user_id string
    pub reg_states: Mutex<HashMap<String, PasskeyRegistration>>,
    /// In-progress authentication ceremonies keyed by user_id string
    pub auth_states: Mutex<HashMap<String, PasskeyAuthentication>>,
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

    let auth_routes = Router::new()
        .route("/register/begin", post(auth::register_begin))
        .route("/register/finish", post(auth::register_finish))
        .route("/login/begin", post(auth::login_begin))
        .route("/login/finish", post(auth::login_finish));

    let sync_routes = Router::new()
        .route("/push", post(sync::push))
        .route("/pull", get(sync::pull))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .nest("/auth", auth_routes)
        .nest("/sync", sync_routes)
        .route("/health", get(|| async { "ok" }))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let port: u16 = std::env::var("DATUM_SYNC_PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()?;

    let db_path = std::env::var("DATUM_SYNC_DB")
        .unwrap_or_else(|_| "./datum-sync-server.db".to_string());

    let origin = std::env::var("DATUM_SYNC_ORIGIN")
        .unwrap_or_else(|_| format!("http://localhost:{}", port));

    let rp_name = std::env::var("DATUM_SYNC_RP_NAME")
        .unwrap_or_else(|_| "datum sync".to_string());

    let jwt_secret = std::env::var("DATUM_SYNC_JWT_SECRET")
        .unwrap_or_else(|_| "changeme-not-for-production".to_string());

    let origin_url = Url::parse(&origin)?;
    let rp_id = origin_url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid origin: no host"))?
        .to_string();

    let webauthn = webauthn_rs::WebauthnBuilder::new(&rp_id, &origin_url)?
        .rp_name(&rp_name)
        .build()?;

    let conn = Connection::open(&db_path)?;
    db::init_db(&conn)?;

    let state = Arc::new(AppState {
        db: Mutex::new(conn),
        webauthn,
        jwt_secret,
        reg_states: Mutex::new(HashMap::new()),
        auth_states: Mutex::new(HashMap::new()),
    });

    let app = build_app(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("datum-sync-server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use base64::Engine;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        let origin = Url::parse("http://localhost:3000").unwrap();
        let webauthn = webauthn_rs::WebauthnBuilder::new("localhost", &origin)
            .unwrap()
            .build()
            .unwrap();

        Arc::new(AppState {
            db: Mutex::new(conn),
            webauthn,
            jwt_secret: "test-secret".to_string(),
            reg_states: Mutex::new(HashMap::new()),
            auth_states: Mutex::new(HashMap::new()),
        })
    }

    /// Create a test user in the DB and return a valid JWT for them.
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
        let bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn test_health_check() {
        let state = test_state();
        let app = build_app(state);

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

    #[tokio::test]
    async fn test_unauthorized_without_jwt() {
        let state = test_state();
        let app = build_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/sync/pull?since=0&device=test")
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
            "payload": "SGVsbG8gV29ybGQ=" // base64("Hello World")
        });

        // Push a changeset
        let push_resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sync/push")
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

        // Pull from another device
        let pull_resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/sync/pull?since=0&device=device-2")
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
        assert_eq!(changesets[0]["payload"], "SGVsbG8gV29ybGQ=");
    }

    #[tokio::test]
    async fn test_multi_device_integration() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        // Push 3 changesets from device-1
        for i in 1..=3 {
            let payload = base64::engine::general_purpose::STANDARD
                .encode(format!("changeset-{}", i));
            let body = serde_json::json!({
                "device_id": "device-1",
                "payload": payload
            });

            let resp = build_app(state.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/sync/push")
                        .header("Authorization", format!("Bearer {}", token))
                        .header("Content-Type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(resp.status(), StatusCode::OK, "push {} failed", i);
        }

        // Pull from device-2 — should receive all 3
        let pull_resp = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/sync/pull?since=0&device=device-2")
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

        // Verify device-1 doesn't see its own changesets
        let pull_resp2 = build_app(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/sync/pull?since=0&device=device-1")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(pull_resp2.status(), StatusCode::OK);
        let self_view: Vec<serde_json::Value> =
            serde_json::from_slice(&pull_resp2.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(self_view.len(), 0, "device-1 should not see its own changesets");
    }

    #[tokio::test]
    async fn test_incremental_pull_with_since() {
        let state = test_state();
        let (_, token) = make_test_jwt(&state);

        // Push 2 changesets from device-1
        for i in 1..=2 {
            let body = serde_json::json!({
                "device_id": "device-1",
                "payload": format!("p{}", i)
            });
            build_app(state.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/sync/push")
                        .header("Authorization", format!("Bearer {}", token))
                        .header("Content-Type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
        }

        // First pull from device-2: gets both
        let resp1 = build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/sync/pull?since=0&device=device-2")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let batch1: Vec<serde_json::Value> =
            serde_json::from_slice(&resp1.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(batch1.len(), 2);

        // Now device-1 pushes one more
        let body = serde_json::json!({"device_id": "device-1", "payload": "p3"});
        build_app(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sync/push")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Incremental pull — find last global id we saw and pull since then
        // The second changeset has id=2 (global AUTOINCREMENT), pull since=2
        let resp2 = build_app(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/sync/pull?since=2&device=device-2")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let batch2: Vec<serde_json::Value> =
            serde_json::from_slice(&resp2.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(batch2.len(), 1, "should only get the new changeset");
        assert_eq!(batch2[0]["payload"], "p3");
    }
}
