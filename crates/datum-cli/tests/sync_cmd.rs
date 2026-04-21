//! Acceptance tests for `datum sync` (bead da-ndd.13).

mod sync_support;

use datum_cli::config::{
    auth_token_path, load_raw_config_or_default, master_key_path, resolve_sync_server,
    server_host_from_url,
};
use datum_cli::sync::{login, AuthToken, TestPrompter};
use reqwest::blocking::Client;
use std::sync::Mutex;
use tempfile::TempDir;

use sync_support::{StubRelay, StubRelayOptions};

/// Serialize tests that mutate process-wide env (DATUM_HOME, DATUM_CONFIG_HOME, DATUM_DB, DATUM_CONFIG).
/// These are global state; running in parallel would cause false failures.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _tmp: TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
    _orig_home: EnvRestore,
    _orig_cfg_home: EnvRestore,
    _orig_db: EnvRestore,
    _orig_cfg: EnvRestore,
}

struct EnvRestore {
    key: &'static str,
    prev: Option<String>,
}

impl EnvRestore {
    fn capture(key: &'static str) -> Self {
        Self {
            key,
            prev: std::env::var(key).ok(),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

/// Isolate filesystem + env so tests never touch the real user config.
fn scoped_env() -> EnvGuard {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = TempDir::new().expect("mk tmp");
    let orig_home = EnvRestore::capture("DATUM_HOME");
    let orig_cfg = EnvRestore::capture("DATUM_CONFIG_HOME");
    let orig_db = EnvRestore::capture("DATUM_DB");
    let orig_cfg_file = EnvRestore::capture("DATUM_CONFIG");

    std::env::set_var("DATUM_HOME", tmp.path().join("data"));
    std::env::set_var("DATUM_CONFIG_HOME", tmp.path().join("config"));
    std::env::remove_var("DATUM_DB");
    std::env::remove_var("DATUM_CONFIG");

    EnvGuard {
        _tmp: tmp,
        _guard: guard,
        _orig_home: orig_home,
        _orig_cfg_home: orig_cfg,
        _orig_db: orig_db,
        _orig_cfg: orig_cfg_file,
    }
}

fn write_token_for(server: &str, token: &str) {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host).unwrap();
    let t = AuthToken {
        token: token.to_string(),
        refreshed_at: chrono::Utc::now().to_rfc3339(),
        server: server.to_string(),
        user: Some("vividn@vividn.dev".to_string()),
    };
    std::fs::write(path, serde_json::to_vec_pretty(&t).unwrap()).unwrap();
}

// ──────────────────────────────────────────────────────────────────────────
// sync login
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn sync_login_polls_until_authorized() {
    let _env = scoped_env();
    let relay = StubRelay::start();
    let client = Client::builder().build().unwrap();
    let mut prompter = TestPrompter::new::<Vec<String>, String>(vec![]);

    let token = login::run_with_client(&relay.base_url, &client, &mut prompter)
        .expect("login succeeds");

    assert!(token.token.starts_with("bearer-for-"));
    assert_eq!(token.user.as_deref(), Some("vividn@vividn.dev"));
    assert_eq!(token.server, relay.base_url);

    // Prompter captured the verification URL + user code.
    assert!(
        prompter
            .events
            .iter()
            .any(|e| e.contains("login:https://sync.example.com/activate:ABCD-1234")),
        "expected login instructions, got: {:?}",
        prompter.events
    );
    // And an authorization confirmation status line.
    assert!(
        prompter.events.iter().any(|e| e.contains("Authorized")),
        "expected authorization status, got: {:?}",
        prompter.events
    );
}

#[test]
fn sync_login_handles_slow_down_hint() {
    let _env = scoped_env();
    // First poll returns slow_down; subsequent polls proceed normally.
    let relay = StubRelay::start_with_options(StubRelayOptions {
        slow_down_polls: 1,
        ..Default::default()
    });
    let client = Client::builder().build().unwrap();
    let mut prompter = TestPrompter::new::<Vec<String>, String>(vec![]);

    let token = login::run_with_client(&relay.base_url, &client, &mut prompter)
        .expect("login succeeds through slow_down");

    assert!(
        prompter
            .events
            .iter()
            .any(|e| e.contains("status:slow_down")),
        "expected slow_down status, got: {:?}",
        prompter.events
    );
    assert!(!token.token.is_empty());
}

#[test]
fn sync_login_stores_token_at_xdg_path() {
    let _env = scoped_env();
    let relay = StubRelay::start();
    let client = Client::builder().build().unwrap();
    let mut prompter = TestPrompter::new::<Vec<String>, String>(vec![]);

    login::run_with_client(&relay.base_url, &client, &mut prompter)
        .expect("login succeeds");

    // The token must be persisted under $XDG_CONFIG_HOME/datum/auth/<host>.json.
    let host = server_host_from_url(&relay.base_url);
    let path = auth_token_path(&host).unwrap();
    assert!(path.exists(), "token file missing at {:?}", path);
    let bytes = std::fs::read(&path).unwrap();
    let stored: AuthToken = serde_json::from_slice(&bytes).unwrap();
    assert!(stored.token.starts_with("bearer-for-"));
    assert_eq!(stored.server, relay.base_url);

    // On unix the file must be mode 0600.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&path).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }
}

// ──────────────────────────────────────────────────────────────────────────
// sync config (yaml layering)
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn sync_config_set_server_per_db() {
    let _env = scoped_env();
    std::env::set_var("DATUM_DB", "fitness"); // named, not path-like

    datum_cli::sync::config::set_server("https://sync.example.com".into(), false)
        .expect("set per-db");

    let cfg = load_raw_config_or_default().unwrap();
    let section = cfg.databases.get("fitness").expect("db section present");
    assert_eq!(section.sync_server.as_deref(), Some("https://sync.example.com"));

    // Global default stays unset.
    assert!(cfg.sync.as_ref().and_then(|s| s.default_server.as_ref()).is_none());

    // And resolve_sync_server picks up the per-db value.
    assert_eq!(
        resolve_sync_server(&cfg, Some("fitness")).as_deref(),
        Some("https://sync.example.com"),
    );
}

#[test]
fn sync_config_set_server_global() {
    let _env = scoped_env();

    datum_cli::sync::config::set_server("https://sync.example.com".into(), true)
        .expect("set global");

    let cfg = load_raw_config_or_default().unwrap();
    assert_eq!(
        cfg.sync.as_ref().and_then(|s| s.default_server.as_deref()),
        Some("https://sync.example.com"),
    );
    assert!(cfg.databases.is_empty());

    // resolve_sync_server with no db name falls through to the global.
    assert_eq!(
        resolve_sync_server(&cfg, None).as_deref(),
        Some("https://sync.example.com"),
    );

    // Per-db value overrides global:
    let mut cfg = cfg;
    cfg.databases.entry("work".into()).or_default().sync_server =
        Some("https://other.example.com".into());
    assert_eq!(
        resolve_sync_server(&cfg, Some("work")).as_deref(),
        Some("https://other.example.com"),
    );
    // While a different db still gets the global default.
    assert_eq!(
        resolve_sync_server(&cfg, Some("unknown")).as_deref(),
        Some("https://sync.example.com"),
    );
}

#[test]
fn sync_config_disable_writes_none() {
    let _env = scoped_env();

    // No active named DB → global disable.
    datum_cli::sync::config::disable().expect("disable global");
    let cfg = load_raw_config_or_default().unwrap();
    assert_eq!(
        cfg.sync.as_ref().and_then(|s| s.default_server.as_deref()),
        Some("none"),
    );
    // resolve filters "none" → None.
    assert!(resolve_sync_server(&cfg, None).is_none());
}

// ──────────────────────────────────────────────────────────────────────────
// sync key
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn sync_key_init_creates_wrapped_upload() {
    let _env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, "bearer-XYZ");
    let client = Client::builder().build().unwrap();
    let mut prompter = TestPrompter::new(["secret-passphrase", "secret-passphrase"]);

    let master = datum_cli::sync::key::init_with_client(
        &relay.base_url,
        &client,
        false,
        &mut prompter,
    )
    .expect("init ok");

    // Local plaintext key written (mode 0600).
    let host = server_host_from_url(&relay.base_url);
    let key_path = master_key_path(&host).unwrap();
    assert!(key_path.exists());
    let disk_key = std::fs::read(&key_path).unwrap();
    assert_eq!(disk_key.as_slice(), master.as_slice());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&key_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    // Relay received a wrapped-key payload shaped per docs/sync-protocol.md.
    let payload = relay.wrapped_key().expect("wrapped key uploaded");
    assert_eq!(payload["kdf"], "argon2id");
    assert_eq!(payload["wrap_alg"], "aes-256-gcm");
    assert!(payload["wrapped_key_b64"].as_str().unwrap().len() > 0);
    assert!(payload["nonce_b64"].as_str().unwrap().len() > 0);

    // Existing key without --force must error.
    let mut prompter2 = TestPrompter::new::<Vec<String>, String>(vec![]);
    let err = datum_cli::sync::key::init_with_client(
        &relay.base_url,
        &client,
        false,
        &mut prompter2,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("already exists"),
        "expected already-exists error, got: {err}"
    );
}

#[test]
fn sync_key_unlock_downloads_and_unwraps() {
    let _env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, "bearer-XYZ");
    let client = Client::builder().build().unwrap();

    // Init on one device (simulating a prior session on the same host).
    let mut prompter = TestPrompter::new(["pass1", "pass1"]);
    let master = datum_cli::sync::key::init_with_client(
        &relay.base_url,
        &client,
        false,
        &mut prompter,
    )
    .expect("init ok");

    // Delete the local key file — simulating a fresh device.
    let host = server_host_from_url(&relay.base_url);
    let key_path = master_key_path(&host).unwrap();
    std::fs::remove_file(&key_path).unwrap();

    // Unlock with the right passphrase recovers the same key.
    let mut prompter2 = TestPrompter::new(["pass1"]);
    let recovered = datum_cli::sync::key::unlock_with_client(
        &relay.base_url,
        &client,
        &mut prompter2,
    )
    .expect("unlock ok");
    assert_eq!(recovered, master);
    assert!(key_path.exists());
}

#[test]
fn sync_key_wrong_passphrase_fails() {
    let _env = scoped_env();
    let relay = StubRelay::start();
    write_token_for(&relay.base_url, "bearer-XYZ");
    let client = Client::builder().build().unwrap();

    let mut prompter = TestPrompter::new(["right-pass", "right-pass"]);
    datum_cli::sync::key::init_with_client(&relay.base_url, &client, false, &mut prompter)
        .expect("init ok");

    // Different-passphrase unlock path on a fresh-looking local device.
    let host = server_host_from_url(&relay.base_url);
    let key_path = master_key_path(&host).unwrap();
    std::fs::remove_file(&key_path).unwrap();

    let mut bad_prompter = TestPrompter::new(["wrong-pass"]);
    let err = datum_cli::sync::key::unlock_with_client(
        &relay.base_url,
        &client,
        &mut bad_prompter,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unlock failed"),
        "expected unlock failure, got: {msg}"
    );
    // Local key file must NOT be written when unlock fails.
    assert!(!key_path.exists(), "key file was written despite failure");
}

// ──────────────────────────────────────────────────────────────────────────
// End-to-end integration
// ──────────────────────────────────────────────────────────────────────────

#[test]
fn end_to_end_login_and_key_init() {
    let _env = scoped_env();
    let relay = StubRelay::start();
    let client = Client::builder().build().unwrap();

    // 1. Login
    let mut login_prompter = TestPrompter::new::<Vec<String>, String>(vec![]);
    let token = login::run_with_client(&relay.base_url, &client, &mut login_prompter)
        .expect("login");
    assert!(!token.token.is_empty());

    // 2. Key init using the newly stored token
    let mut key_prompter = TestPrompter::new(["the-pass", "the-pass"]);
    let master = datum_cli::sync::key::init_with_client(
        &relay.base_url,
        &client,
        false,
        &mut key_prompter,
    )
    .expect("key init");

    // 3. Wrapped key lives on the relay and unwraps back to `master`.
    let wrapped_json = relay.wrapped_key().expect("relay has wrapped key");
    let wrapped: datum_core::sync::WrappedKey =
        serde_json::from_value(wrapped_json).expect("parse wrapped");
    let recovered =
        datum_core::sync::unwrap_master_key("the-pass", &wrapped).expect("unwrap");
    assert_eq!(recovered, master);
}
