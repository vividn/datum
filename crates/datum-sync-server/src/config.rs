//! Single source of truth for relay configuration (da-xsrd).
//!
//! Every env read the binary needs lives here so the fail-closed JWT
//! secret check has exactly one audited home. The server binds
//! `0.0.0.0` — network-exposed by default, not loopback-only — so a
//! missing or weak `DATUM_SYNC_JWT_SECRET` must abort startup. The old
//! behavior (silently falling back to a string published in this very
//! repo) let an unconfigured relay boot fully functional while anyone
//! who read the source could forge a 30-day bearer for any user_id.

use anyhow::{bail, Context};

/// Minimum JWT secret length in bytes. HS256 keys shorter than the
/// SHA-256 output size weaken the MAC, and the floor also rejects the
/// historical fallback strings (`changeme-not-for-production`,
/// `change-me-in-production`) by construction — both are under 32.
pub const MIN_JWT_SECRET_LEN: usize = 32;

#[derive(Debug)]
pub struct Config {
    pub port: u16,
    pub db_path: String,
    pub origin: String,
    pub rp_name: String,
    pub jwt_secret: String,
    /// da-1am4: opt-in for the unauthenticated `/device-code/authorize`
    /// endpoint. Default false — leaving it off keeps a deployed relay from
    /// exposing the account-takeover bypass. Enable only behind a real
    /// verification step. Set `DATUM_SYNC_ENABLE_DEVICE_AUTHORIZE=true`.
    pub device_authorize_enabled: bool,
    /// da-7vc3: per-IP request budget for the unauthenticated auth surface
    /// (passkey ceremonies, refresh, device-code start), requests per
    /// minute. Default 30 (burst ~10); 0 disables the limiter. The
    /// device-code poll endpoint is excluded regardless. Set
    /// `DATUM_SYNC_AUTH_RATELIMIT_PER_MIN`.
    pub auth_ratelimit_per_min: u32,
}

impl Config {
    /// Read configuration from the process environment. Fails closed:
    /// no real `DATUM_SYNC_JWT_SECRET`, no config, no server.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Build from an env-shaped lookup. Injectable so tests can pass a
    /// map instead of mutating process env (which races under parallel
    /// test execution).
    pub fn from_lookup(var: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let jwt_secret = var("DATUM_SYNC_JWT_SECRET").ok_or_else(|| {
            anyhow::anyhow!(
                "DATUM_SYNC_JWT_SECRET is not set. Refusing to start: the relay \
                 binds 0.0.0.0 and a guessable secret lets anyone forge bearer \
                 tokens for any user. Generate one with `openssl rand -hex 32`."
            )
        })?;
        if jwt_secret.len() < MIN_JWT_SECRET_LEN {
            bail!(
                "DATUM_SYNC_JWT_SECRET is too short ({} bytes, need at least {}). \
                 Generate one with `openssl rand -hex 32`.",
                jwt_secret.len(),
                MIN_JWT_SECRET_LEN
            );
        }

        let port: u16 = match var("DATUM_SYNC_PORT") {
            Some(p) => p
                .parse()
                .with_context(|| format!("DATUM_SYNC_PORT is not a valid port: {p:?}"))?,
            None => 3000,
        };

        // da-1am4: explicit opt-in only. Anything other than "true"/"1"
        // (incl. unset) leaves the bypass route unmounted.
        let device_authorize_enabled = matches!(
            var("DATUM_SYNC_ENABLE_DEVICE_AUTHORIZE").as_deref(),
            Some("true") | Some("1")
        );

        // da-7vc3: default 30/min/IP. A malformed value is a hard error —
        // same fail-closed posture as the port, so a typo can't silently
        // disable the limiter.
        let auth_ratelimit_per_min: u32 = match var("DATUM_SYNC_AUTH_RATELIMIT_PER_MIN") {
            Some(v) => v.parse().with_context(|| {
                format!("DATUM_SYNC_AUTH_RATELIMIT_PER_MIN is not a valid count: {v:?}")
            })?,
            None => 30,
        };

        Ok(Config {
            db_path: var("DATUM_SYNC_DB").unwrap_or_else(|| "./datum-sync-server.db".into()),
            origin: var("DATUM_SYNC_ORIGIN").unwrap_or_else(|| format!("http://localhost:{port}")),
            rp_name: var("DATUM_SYNC_RP_NAME").unwrap_or_else(|| "datum sync".into()),
            jwt_secret,
            port,
            device_authorize_enabled,
            auth_ratelimit_per_min,
        })
    }

    /// da-1am4: the operational half of the gate. The route being off by
    /// default is enforced in code and covered by a test; *turning it on* is a
    /// deploy-time act that nothing objected to, and the pressure to do it is
    /// built in — the gate deliberately breaks device-code login, so an
    /// operator fixing that feature restores an unauthenticated
    /// account-takeover path and the server starts silently.
    ///
    /// Returned rather than logged here so the warning itself is falsifiable:
    /// deleting it fails a test instead of failing quietly in production.
    pub fn device_authorize_warning(&self) -> Option<&'static str> {
        self.device_authorize_enabled.then_some(
            "SECURITY (da-1am4): DATUM_SYNC_ENABLE_DEVICE_AUTHORIZE is set. \
             /v0/auth/device-code/authorize is mounted UNAUTHENTICATED — it binds a \
             pending device code to any username with no proof of control, which is a \
             full account-takeover path. Enable only behind a real verification step.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    const GOOD_SECRET: &str = "0123456789abcdef0123456789abcdef"; // 32 bytes

    #[test]
    fn missing_jwt_secret_fails_closed() {
        let err = Config::from_lookup(lookup(&[])).unwrap_err();
        assert!(err.to_string().contains("DATUM_SYNC_JWT_SECRET"));
    }

    #[test]
    fn short_jwt_secret_is_rejected() {
        let err = Config::from_lookup(lookup(&[("DATUM_SYNC_JWT_SECRET", "hunter2")])).unwrap_err();
        assert!(err.to_string().contains("too short"));
    }

    /// Both historical fallback strings must never validate — a deploy
    /// that explicitly sets one of them is as forgeable as one that
    /// sets nothing.
    #[test]
    fn known_default_secrets_are_rejected() {
        for default in ["changeme-not-for-production", "change-me-in-production"] {
            assert!(
                Config::from_lookup(lookup(&[("DATUM_SYNC_JWT_SECRET", default)])).is_err(),
                "known default {default:?} must not boot"
            );
        }
    }

    #[test]
    fn valid_secret_yields_defaults() {
        let config =
            Config::from_lookup(lookup(&[("DATUM_SYNC_JWT_SECRET", GOOD_SECRET)])).unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.db_path, "./datum-sync-server.db");
        assert_eq!(config.origin, "http://localhost:3000");
        assert_eq!(config.rp_name, "datum sync");
        assert_eq!(config.jwt_secret, GOOD_SECRET);
        assert_eq!(config.auth_ratelimit_per_min, 30);
    }

    /// da-7vc3: the auth rate-limit budget is env-tunable, and `0` is the
    /// explicit off-switch.
    #[test]
    fn auth_ratelimit_is_configurable() {
        let config = Config::from_lookup(lookup(&[
            ("DATUM_SYNC_JWT_SECRET", GOOD_SECRET),
            ("DATUM_SYNC_AUTH_RATELIMIT_PER_MIN", "0"),
        ]))
        .unwrap();
        assert_eq!(config.auth_ratelimit_per_min, 0);

        let config = Config::from_lookup(lookup(&[
            ("DATUM_SYNC_JWT_SECRET", GOOD_SECRET),
            ("DATUM_SYNC_AUTH_RATELIMIT_PER_MIN", "120"),
        ]))
        .unwrap();
        assert_eq!(config.auth_ratelimit_per_min, 120);
    }

    /// A garbage rate-limit value fails closed rather than silently
    /// disabling the limiter.
    #[test]
    fn invalid_auth_ratelimit_is_rejected() {
        let err = Config::from_lookup(lookup(&[
            ("DATUM_SYNC_JWT_SECRET", GOOD_SECRET),
            ("DATUM_SYNC_AUTH_RATELIMIT_PER_MIN", "lots"),
        ]))
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("DATUM_SYNC_AUTH_RATELIMIT_PER_MIN"));
    }

    /// The default origin tracks the configured port, matching what the
    /// binary previously derived inline.
    #[test]
    fn default_origin_follows_port() {
        let config = Config::from_lookup(lookup(&[
            ("DATUM_SYNC_JWT_SECRET", GOOD_SECRET),
            ("DATUM_SYNC_PORT", "8089"),
        ]))
        .unwrap();
        assert_eq!(config.port, 8089);
        assert_eq!(config.origin, "http://localhost:8089");
    }

    /// A garbage port is a hard error, not a silent fallback — same
    /// fail-closed posture as the secret.
    #[test]
    fn invalid_port_is_rejected() {
        let err = Config::from_lookup(lookup(&[
            ("DATUM_SYNC_JWT_SECRET", GOOD_SECRET),
            ("DATUM_SYNC_PORT", "not-a-port"),
        ]))
        .unwrap_err();
        assert!(err.to_string().contains("DATUM_SYNC_PORT"));
    }

    /// da-1am4: the warning must exist exactly when the bypass is mounted.
    /// Its whole job is to be loud at deploy time, so it needs a falsifier of
    /// its own — delete the warning and this fails.
    #[test]
    fn enabling_device_authorize_warns() {
        let off = Config::from_lookup(lookup(&[("DATUM_SYNC_JWT_SECRET", GOOD_SECRET)])).unwrap();
        assert!(!off.device_authorize_enabled);
        assert!(
            off.device_authorize_warning().is_none(),
            "no warning when the bypass is not mounted"
        );

        let on = Config::from_lookup(lookup(&[
            ("DATUM_SYNC_JWT_SECRET", GOOD_SECRET),
            ("DATUM_SYNC_ENABLE_DEVICE_AUTHORIZE", "true"),
        ]))
        .unwrap();
        assert!(on.device_authorize_enabled);
        let warning = on
            .device_authorize_warning()
            .expect("enabling the bypass must warn");
        assert!(warning.contains("da-1am4"), "warning must cite the bead");
        assert!(
            warning.contains("UNAUTHENTICATED"),
            "warning must name the exposure, not just the flag"
        );
    }

    #[test]
    fn explicit_vars_override_defaults() {
        let config = Config::from_lookup(lookup(&[
            ("DATUM_SYNC_JWT_SECRET", GOOD_SECRET),
            ("DATUM_SYNC_DB", "/tmp/relay.db"),
            ("DATUM_SYNC_ORIGIN", "https://sync.example.com"),
            ("DATUM_SYNC_RP_NAME", "example sync"),
        ]))
        .unwrap();
        assert_eq!(config.db_path, "/tmp/relay.db");
        assert_eq!(config.origin, "https://sync.example.com");
        assert_eq!(config.rp_name, "example sync");
    }
}
