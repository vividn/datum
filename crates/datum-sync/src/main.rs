use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use url::Url;
use webauthn_rs::prelude::*;

mod auth;
mod config;
mod db;
mod error;
mod jwt;
mod sync;

pub use error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user_id: String,
}

pub struct AppState {
    pub db: db::Db,
    pub webauthn: Arc<Webauthn>,
    pub jwt_secret: String,
    pub reg_states: Mutex<HashMap<String, PasskeyRegistration>>,
    pub auth_states: Mutex<HashMap<String, (String, PasskeyAuthentication)>>,
}

pub async fn jwt_auth_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = {
        let headers = request.headers();
        match headers.get("Authorization") {
            Some(value) => {
                let s = value.to_str().unwrap_or("");
                match s.strip_prefix("Bearer ") {
                    Some(t) => t.to_owned(),
                    None => return AppError::Unauthorized.into_response(),
                }
            }
            None => return AppError::Unauthorized.into_response(),
        }
    };

    match jwt::verify_jwt(&token, &state.jwt_secret) {
        Ok(user_id) => {
            request.extensions_mut().insert(AuthUser { user_id });
            next.run(request).await
        }
        Err(_) => AppError::Unauthorized.into_response(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cfg = config::Config::from_env();

    let db = db::init_db(&cfg.db_path)?;

    let rp_origin = Url::parse(&cfg.origin)?;
    let rp_id = rp_origin.host_str().unwrap_or("localhost").to_string();

    let webauthn = Arc::new(
        WebauthnBuilder::new(&rp_id, &rp_origin)?
            .rp_name(&cfg.rp_name)
            .build()?,
    );

    let state = Arc::new(AppState {
        db,
        webauthn,
        jwt_secret: cfg.jwt_secret,
        reg_states: Mutex::new(HashMap::new()),
        auth_states: Mutex::new(HashMap::new()),
    });

    let public_routes = Router::new()
        .route("/auth/register/begin", post(auth::register_begin))
        .route("/auth/register/finish", post(auth::register_finish))
        .route("/auth/login/begin", post(auth::login_begin))
        .route("/auth/login/finish", post(auth::login_finish));

    let protected_routes = Router::new()
        .route("/devices", get(sync::list_devices).post(sync::add_device))
        .route("/sync/push", post(sync::push))
        .route("/sync/pull", get(sync::pull))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            jwt_auth_middleware,
        ));

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    tracing::info!("datum-sync listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
