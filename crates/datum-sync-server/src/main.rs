use std::sync::Arc;

use rusqlite::Connection;
use url::Url;

use datum_sync_server::{build_app, build_webauthn, config::Config, db, jwt, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Fail-closed: refuses to boot without a real DATUM_SYNC_JWT_SECRET
    // (da-xsrd) — the server binds all interfaces below, so a fallback
    // secret would be network-exposed and forgeable.
    let config = Config::from_env()?;

    // Admin bootstrap: `datum-sync-server mint-token <username>` prints a
    // bearer for <username> (creating the user if absent) and exits, instead
    // of starting the server. Run on the relay host — it requires the JWT
    // secret that already lives there, so it is NOT a network-reachable auth
    // bypass (unlike the gated device-code `authorize`, da-1am4). This is the
    // supported way to hand a client a credential while the browser
    // verification flow is unbuilt: mint here, then `export
    // DATUM_SYNC_TOKEN=<token>` on the client before `datum sync`.
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("mint-token") {
        let Some(username) = args.next() else {
            anyhow::bail!("usage: datum-sync-server mint-token <username>");
        };
        let conn = Connection::open(&config.db_path)?;
        db::init_db(&conn)?;
        let user_id = match db::get_user_by_username(&conn, &username)? {
            Some(id) => id,
            None => db::create_user(&conn, &username)?,
        };
        let token = jwt::create_token(&user_id, &config.jwt_secret)
            .map_err(|e| anyhow::anyhow!("mint token: {e}"))?;
        println!("# datum sync bearer for {username} (user_id {user_id}, valid 30 days)");
        println!("# on each client, before running `datum sync`:");
        println!("export DATUM_SYNC_TOKEN={token}");
        return Ok(());
    }

    let origin_url = Url::parse(&config.origin)?;
    let webauthn = build_webauthn(&origin_url, &config.rp_name)?;

    let conn = Connection::open(&config.db_path)?;
    db::init_db(&conn)?;

    // da-1am4: say it out loud before the listener comes up. The gate is
    // enforced in code; this is the half that was missing — an operator who
    // enables the bypass to restore device-code login gets told what they just
    // mounted, instead of a silent "listening on ...".
    if let Some(warning) = config.device_authorize_warning() {
        tracing::warn!("{warning}");
    }

    let state = Arc::new(
        AppState::new(conn, webauthn, config.jwt_secret)
            .with_device_authorize(config.device_authorize_enabled)
            .with_rate_limit(config.auth_ratelimit_per_min),
    );
    let app = build_app(state);

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!("datum-sync-server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    // da-7vc3: `into_make_service_with_connect_info` exposes the peer
    // `SocketAddr` to the per-IP rate limiter. Without it every request
    // would fall back to the unspecified address and share one bucket.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
