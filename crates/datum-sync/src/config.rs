pub struct Config {
    pub port: u16,
    pub db_path: String,
    pub origin: String,
    pub rp_name: String,
    pub jwt_secret: String,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            port: std::env::var("DATUM_SYNC_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            db_path: std::env::var("DATUM_SYNC_DB")
                .unwrap_or_else(|_| "./datum-sync.db".into()),
            origin: std::env::var("DATUM_SYNC_ORIGIN")
                .unwrap_or_else(|_| "http://localhost:3000".into()),
            rp_name: std::env::var("DATUM_SYNC_RP_NAME")
                .unwrap_or_else(|_| "datum sync".into()),
            jwt_secret: std::env::var("DATUM_SYNC_JWT_SECRET")
                .unwrap_or_else(|_| "change-me-in-production".into()),
        }
    }
}
