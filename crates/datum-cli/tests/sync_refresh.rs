//! Acceptance tests for bearer-token refresh on 401 (bead da-1i5x).
//!
//! Verifies the end-to-end recovery contract spelled out in
//! `docs/sync-protocol.md` §Authentication: "A 401 response means
//! 'reauthenticate'; the client should kick off the device-code flow
//! and retry once." Pre-da-1i5x, the client did not implement that
//! retry — every 401 surfaced as `push failed: 401 ...` and forced the
//! user to re-run `datum sync login`. The Session struct in
//! `datum-sync-client::http::session` closes that loop.

mod sync_support;

use chrono::Utc;
use datum_cli::config::{auth_token_path, master_key_path, server_host_from_url};
use datum_cli::sync::{run_sync_with_client, AuthToken, Direction};
use datum_core::db::Db;
use datum_core::entry::{Entry, Verb};
use datum_core::time::DatumTime;
use datum_sync_client::http::Session;
use datum_sync_server::refresh as server_refresh;
use tempfile::TempDir;

use sync_support::{StubRelay, ENV_LOCK};

// ──────────────────────────────────────────────────────────────────────────
// Scoped-env scaffolding (mirrors sync_cmd / sync_e2e patterns)
// ──────────────────────────────────────────────────────────────────────────

struct EnvGuard {
    _tmp: TempDir,
    _orig_home: EnvRestore,
    _orig_cfg_home: EnvRestore,
    _orig_db: EnvRestore,
    _orig_cfg: EnvRestore,
    _guard: std::sync::MutexGuard<'static, ()>,
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
        _orig_home: orig_home,
        _orig_cfg_home: orig_cfg,
        _orig_db: orig_db,
        _orig_cfg: orig_cfg_file,
        _guard: guard,
    }
}

fn write_token_for(server: &str, token: AuthToken) {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host).unwrap();
    std::fs::write(path, serde_json::to_vec_pretty(&token).unwrap()).unwrap();
}

fn read_token(server: &str) -> AuthToken {
    let host = server_host_from_url(server);
    let path = auth_token_path(&host).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn write_master_key(server: &str, key: &[u8; 32]) {
    let host = server_host_from_url(server);
    let path = master_key_path(&host).unwrap();
    std::fs::write(path, key).unwrap();
}

fn make_entry(field: &str, offset_secs: i64, device: &str) -> Entry {
    use chrono::{Duration, TimeZone};
    let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
    let at = base + Duration::seconds(offset_secs);
    let t = DatumTime {
        utc: at,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    };
    let mut e = Entry::new_legacy(
        field,
        Verb::Point,
        Some(serde_json::json!(1)),
        None,
        Some(t),
        None,
        device,
    );
    e.create_time = at;
    e.modify_time = at;
    e
}

fn fresh_db(env: &EnvGuard) -> Db {
    let _ = env;
    let path = std::env::var("DATUM_HOME").unwrap();
    std::fs::create_dir_all(&path).unwrap();
    let db_path = std::path::Path::new(&path).join("datum.db");
    Db::open(db_path.to_str().unwrap()).unwrap()
}

/// Mint a refresh token for the relay's test user, plumbed through the
/// real server `refresh::mint_and_persist` so the resulting token works
/// against `/v1/auth/refresh` at the wire level.
fn mint_refresh_for_test_user(relay: &StubRelay) -> String {
    let conn = relay.state.db.lock().unwrap();
    server_refresh::mint_and_persist(&conn, &relay.user_id).unwrap()
}

// ──────────────────────────────────────────────────────────────────────────
// Refresh-path acceptance tests
// ──────────────────────────────────────────────────────────────────────────

/// Bogus bearer + valid refresh token → sync succeeds after auto-refresh.
/// The on-disk token file is rewritten with the new bearer + refresh, so
/// the next session starts with fresh credentials without prompting the
/// user.
#[test]
fn sync_auto_refreshes_on_401_and_persists_new_token() {
    let env = scoped_env();
    let relay = StubRelay::start();
    let valid_refresh = mint_refresh_for_test_user(&relay);

    // Deliberately bogus bearer — the relay will 401 the first push.
    write_token_for(
        &relay.base_url,
        AuthToken {
            token: "bogus-bearer-not-a-jwt".to_string(),
            refresh_token: Some(valid_refresh.clone()),
            refreshed_at: Some(Utc::now().to_rfc3339()),
            server: Some(relay.base_url.clone()),
            user_id: Some(relay.user_id.to_string()),
            cluster_id: None,
        },
    );
    write_master_key(&relay.base_url, &[0xA1u8; 32]);

    let db = fresh_db(&env);
    db.insert_entry(&make_entry("coffee", 0, "dev-X")).unwrap();

    let outcome = run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db)
        .expect("sync recovers via refresh");
    assert_eq!(outcome.pushed, 1, "push must land after refresh");

    // The refreshed bearer + a NEW refresh token must have been
    // persisted back to disk so the next sync uses the fresh pair.
    let on_disk = read_token(&relay.base_url);
    assert_ne!(
        on_disk.token, "bogus-bearer-not-a-jwt",
        "bearer must be rotated on disk"
    );
    assert!(on_disk.refresh_token.is_some());
    assert_ne!(
        on_disk.refresh_token.as_deref(),
        Some(valid_refresh.as_str()),
        "refresh token rotates on every successful refresh (single-use)"
    );
}

/// Bogus bearer + bogus refresh → typed error mentions both
/// "session expired" and "datum sync login" so the user gets actionable
/// guidance, not an opaque "401" string.
#[test]
fn sync_with_bogus_refresh_fails_with_actionable_error() {
    let env = scoped_env();
    let relay = StubRelay::start();

    write_token_for(
        &relay.base_url,
        AuthToken {
            token: "bogus-bearer".to_string(),
            refresh_token: Some("bogus-refresh-token".to_string()),
            refreshed_at: Some(Utc::now().to_rfc3339()),
            server: Some(relay.base_url.clone()),
            user_id: Some(relay.user_id.to_string()),
            cluster_id: None,
        },
    );
    write_master_key(&relay.base_url, &[0xA2u8; 32]);

    let db = fresh_db(&env);
    db.insert_entry(&make_entry("mood", 0, "dev-X")).unwrap();

    let err = run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db)
        .expect_err("sync must fail when both bearer and refresh are rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("session expired"),
        "expected 'session expired' in error, got: {msg}"
    );
    assert!(
        msg.contains("datum sync login"),
        "expected 'datum sync login' in error, got: {msg}"
    );
}

/// Missing refresh_token in the persisted file (legacy login or hand-
/// edited file) → the Session bails with the "no refresh token" branch
/// of the session-expired prompt, instead of trying to call /refresh
/// with `None`.
#[test]
fn sync_without_refresh_token_fails_gracefully_on_401() {
    let env = scoped_env();
    let relay = StubRelay::start();

    write_token_for(
        &relay.base_url,
        AuthToken {
            token: "bogus-bearer".to_string(),
            refresh_token: None,
            refreshed_at: Some(Utc::now().to_rfc3339()),
            server: Some(relay.base_url.clone()),
            user_id: Some(relay.user_id.to_string()),
            cluster_id: None,
        },
    );
    write_master_key(&relay.base_url, &[0xA3u8; 32]);

    let db = fresh_db(&env);
    db.insert_entry(&make_entry("focus", 0, "dev-X")).unwrap();

    let err = run_sync_with_client(Some(&relay.base_url), Direction::PushOnly, false, &db)
        .expect_err("sync with no refresh_token must fail on 401");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("session expired"),
        "missing-refresh path must surface as session-expired: {msg}"
    );
    assert!(
        msg.contains("datum sync login"),
        "missing-refresh path must point at `datum sync login`: {msg}"
    );
}

/// Session unit test: a 200 on the first attempt does NOT trigger a
/// refresh round-trip. Catches the regression where every call would
/// pre-emptively rotate.
#[test]
fn session_does_not_refresh_on_success() {
    let _env = scoped_env();
    let relay = StubRelay::start();
    let valid_bearer = relay.mint_token();
    let valid_refresh = mint_refresh_for_test_user(&relay);

    let token = AuthToken {
        token: valid_bearer.clone(),
        refresh_token: Some(valid_refresh),
        refreshed_at: None,
        server: Some(relay.base_url.clone()),
        user_id: Some(relay.user_id.to_string()),
        cluster_id: None,
    };
    let session = Session::new(&relay.base_url, &token, None);
    let client = reqwest::blocking::Client::builder().build().unwrap();

    let value = session
        .run::<_, &'static str>(&client, |bearer| {
            assert_eq!(bearer, valid_bearer.as_str());
            Ok("ok")
        })
        .unwrap();
    assert_eq!(value, "ok");
    assert_eq!(session.refresh_count(), 0, "no refresh on success");
}

/// Session unit test: a 401 → 200 sequence triggers exactly one
/// refresh and exposes the new bearer to the retry attempt.
#[test]
fn session_refreshes_once_on_401_then_retries() {
    let _env = scoped_env();
    let relay = StubRelay::start();
    let valid_refresh = mint_refresh_for_test_user(&relay);

    let token = AuthToken {
        token: "bogus-bearer".to_string(),
        refresh_token: Some(valid_refresh),
        refreshed_at: None,
        server: Some(relay.base_url.clone()),
        user_id: Some(relay.user_id.to_string()),
        cluster_id: None,
    };
    let session = Session::new(&relay.base_url, &token, None);
    let client = reqwest::blocking::Client::builder().build().unwrap();

    let attempts = std::cell::Cell::new(0u32);
    let result: Result<&'static str, _> = session.run(&client, |bearer| {
        attempts.set(attempts.get() + 1);
        if attempts.get() == 1 {
            assert_eq!(bearer, "bogus-bearer");
            Err(datum_sync_client::http::HttpError::Unauthorized)
        } else {
            assert_ne!(bearer, "bogus-bearer", "retry must see fresh bearer");
            Ok("ok")
        }
    });
    let v = result.unwrap();
    assert_eq!(v, "ok");
    assert_eq!(attempts.get(), 2, "exactly one retry");
    assert_eq!(session.refresh_count(), 1, "exactly one refresh round-trip");
}
