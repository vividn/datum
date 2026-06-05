use std::sync::Arc;

use rusqlite::Connection;
use url::Url;

use datum_sync_server::{build_app, build_webauthn, config::Config, db, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Fail-closed: refuses to boot without a real DATUM_SYNC_JWT_SECRET
    // (da-xsrd) — the server binds all interfaces below, so a fallback
    // secret would be network-exposed and forgeable.
    let config = Config::from_env()?;

    let origin_url = Url::parse(&config.origin)?;
    let webauthn = build_webauthn(&origin_url, &config.rp_name)?;

    let conn = Connection::open(&config.db_path)?;
    db::init_db(&conn)?;

    let state = Arc::new(AppState::new(conn, webauthn, config.jwt_secret));
    let app = build_app(state);

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!("datum-sync-server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
