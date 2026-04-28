use std::sync::Arc;

use rusqlite::Connection;
use url::Url;

use datum_sync_server::{build_app, build_webauthn, db, AppState};

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
    let webauthn = build_webauthn(&origin_url, &rp_name)?;

    let conn = Connection::open(&db_path)?;
    db::init_db(&conn)?;

    let state = Arc::new(AppState::new(conn, webauthn, jwt_secret));
    let app = build_app(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("datum-sync-server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
