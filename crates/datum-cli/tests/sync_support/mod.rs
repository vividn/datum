//! Test harness that spawns the real `datum-sync-server` in-process and
//! exposes the pre-existing `StubRelay` interface to tests.
//!
//! Why a real server? The CLI integration suite previously talked to a
//! tiny axum stub that only loosely matched the wire protocol. That was
//! enough to compile but masked divergences (route prefix, payload field
//! name, missing endpoints) between the client and server crates.
//!
//! We keep the public type names (`StubRelay`, `StubRelayOptions`) so
//! existing tests don't have to be rewritten — only the seams that
//! depend on real auth need updating.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use datum_sync_server::{build_app, build_webauthn, db, jwt, AppState};
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use url::Url;
use uuid::Uuid;

/// Process-wide lock for tests that mutate `DATUM_HOME` / `DATUM_CONFIG_HOME`
/// / `DATUM_DB`. All integration tests across files MUST serialise through
/// this lock — otherwise one test file's `EnvRestore::drop` can unset the
/// env var that another file's test is mid-flight relying on.
#[allow(dead_code)]
pub static ENV_LOCK: Mutex<()> = Mutex::new(());

pub struct StubRelay {
    pub base_url: String,
    pub state: Arc<AppState>,
    /// User the relay auto-authorizes pending device-codes for, and the
    /// subject of the JWT returned by [`StubRelay::mint_token`].
    pub user_id: Uuid,
    #[allow(dead_code)]
    pub username: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
    auth_stop: Arc<AtomicBool>,
    auth_handle: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub struct StubRelayOptions {
    /// Username (display string) the relay authorizes device-codes for and
    /// echoes back as `user_id` on `/v1/auth/device-code/poll`. Kept named
    /// `user_id` for backwards-compat with existing tests.
    pub user_id: String,
    /// First N polls of any device_code return `slow_down` before normal
    /// poll behaviour resumes. Mirrors the prior stub's knob.
    pub slow_down_polls: usize,
}

impl Default for StubRelayOptions {
    fn default() -> Self {
        Self {
            user_id: "vividn@vividn.dev".to_string(),
            slow_down_polls: 0,
        }
    }
}

impl StubRelay {
    pub fn start() -> Self {
        Self::start_with_options(StubRelayOptions::default())
    }

    pub fn start_with_options(opts: StubRelayOptions) -> Self {
        // Real server state, but with the test-only `slow_down_polls_remaining`
        // pre-loaded so the first N polls of any device-code return slow_down.
        let conn = Connection::open_in_memory().expect("open sqlite");
        db::init_db(&conn).expect("init db");
        let origin = Url::parse("http://localhost:0").expect("parse origin");
        let webauthn = build_webauthn(&origin, "datum-test").expect("webauthn");
        let state = Arc::new(AppState::new(
            conn,
            webauthn,
            "test-secret-not-for-prod".to_string(),
        ));
        *state.slow_down_polls_remaining.lock().unwrap() = opts.slow_down_polls;

        // Pre-create the test user; the JWT we mint references this row.
        let user_id = {
            let conn = state.db.lock().unwrap();
            db::create_user(&conn, &opts.user_id).expect("create user")
        };

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<String>();
        let state_for_server = state.clone();

        let handle = std::thread::Builder::new()
            .name("datum-sync-test-server".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build runtime");
                rt.block_on(async move {
                    let app = build_app(state_for_server);
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                        .await
                        .expect("bind");
                    let addr = listener.local_addr().expect("local addr");
                    ready_tx.send(format!("http://{addr}")).expect("ready");
                    axum::serve(listener, app)
                        .with_graceful_shutdown(async {
                            let _ = shutdown_rx.await;
                        })
                        .await
                        .expect("serve");
                });
            })
            .expect("spawn server thread");

        let base_url = ready_rx.recv().expect("server ready");

        // Auto-authorizer. The CLI's device-code flow polls on a loop;
        // the original stub authorized on the second poll. We replicate
        // that pacing so existing assertions (which exercise the
        // pending → authorized transition) still pass: bump a per-code
        // counter on each scan and authorize once it reaches 2.
        let auth_stop = Arc::new(AtomicBool::new(false));
        let auth_state = state.clone();
        let auth_stop_clone = auth_stop.clone();
        let mut seen_codes: HashMap<String, u32> = HashMap::new();
        let auth_user_id = user_id;
        let auth_handle = std::thread::Builder::new()
            .name("datum-sync-test-authorizer".into())
            .spawn(move || {
                while !auth_stop_clone.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                    let mut codes = auth_state.device_codes.lock().unwrap();
                    for (code, record) in codes.iter_mut() {
                        if record.authorized_user_id.is_some() {
                            continue;
                        }
                        // Per-code counter so we mimic the prior stub's
                        // `authorize_after_polls = 2` pacing — auth lands
                        // on the second observation so the device-flow
                        // test exercises the pending → authorized path.
                        let n = seen_codes.entry(code.clone()).or_insert(0);
                        *n += 1;
                        if *n >= 2 {
                            record.authorized_user_id = Some(auth_user_id);
                        }
                    }
                }
            })
            .expect("spawn authorizer");

        Self {
            base_url,
            state,
            user_id,
            username: opts.user_id,
            shutdown: Some(shutdown_tx),
            handle: Some(handle),
            auth_stop,
            auth_handle: Some(auth_handle),
        }
    }

    /// Mint a valid JWT for the relay's default test user. Tests that
    /// bypass the device-code flow seed this into the on-disk auth file
    /// instead of a sentinel string.
    #[allow(dead_code)]
    pub fn mint_token(&self) -> String {
        jwt::create_token(&self.user_id, &self.state.jwt_secret).expect("mint test JWT")
    }

    /// Snapshot the wrapped-key blob currently stored for the test user.
    /// Returns `None` if the user has not yet uploaded one.
    #[allow(dead_code)]
    pub fn wrapped_key(&self) -> Option<JsonValue> {
        let conn = self.state.db.lock().unwrap();
        let blob = db::get_wrapped_key(&conn, &self.user_id).ok().flatten()?;
        serde_json::from_str(&blob).ok()
    }

    /// Replace the wrapped-key blob for the test user; mirrors the prior
    /// stub's setter.
    #[allow(dead_code)]
    pub fn set_wrapped_key(&self, wrapped: JsonValue) {
        let conn = self.state.db.lock().unwrap();
        let blob = serde_json::to_string(&wrapped).expect("serialize wrapped");
        db::upsert_wrapped_key(&conn, &self.user_id, &blob).expect("upsert wrapped");
    }

    /// Inject an already-encrypted blob as if a peer device had pushed
    /// it. The relay assigns a global id; the per-device `seq` is the
    /// value the test specifies (must match the AAD the blob was sealed
    /// with). Tests inject from a fresh device, so seq=1 lines up with
    /// what production push would have computed.
    #[allow(dead_code)]
    pub fn inject_changeset(&self, device_id: &str, seq: i64, payload: &[u8]) {
        let payload_b64 = URL_SAFE_NO_PAD.encode(payload);
        let conn = self.state.db.lock().unwrap();
        // Bypass insert_changeset's auto-seq computation so the test can
        // pin the exact seq used as AAD by the encrypting peer.
        // The production push path stamps cluster_id + generation; pull
        // filters on cluster_id, so the inject helper must mirror that or
        // the row is invisible. Mirror the client's default cluster name
        // (`"default"`) so an injected changeset lands in the cluster the
        // client will be pulling from.
        let user_id_str = self.user_id.to_string();
        let default_cluster = "default";
        conn.execute(
            "INSERT INTO changesets (device_id, seq, payload, cluster_id, generation)
             VALUES (?1, ?2, ?3, ?4, 0)",
            rusqlite::params![device_id, seq, payload_b64, default_cluster],
        )
        .expect("inject changeset");
        // Mirror the production server's auto-device-registration so
        // peer devices appear in the devices table too.
        let _ = conn.execute(
            "INSERT OR IGNORE INTO devices (id, user_id) VALUES (?1, ?2)",
            rusqlite::params![device_id, user_id_str],
        );
        // The default cluster must exist for the pull endpoint's
        // ensure_cluster lookup to pin to the right generation.
        let _ = conn.execute(
            "INSERT OR IGNORE INTO clusters (cluster_id, user_id, generation) VALUES (?1, ?2, 0)",
            rusqlite::params![default_cluster, user_id_str],
        );
    }

    /// Count of stored changesets — same semantics as the prior stub.
    #[allow(dead_code)]
    pub fn changeset_count(&self) -> usize {
        let conn = self.state.db.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM changesets", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0) as usize
    }

    /// How long since the relay started. Lets tests sanity-check that
    /// they aren't fighting a stale auto-authorizer race.
    #[allow(dead_code)]
    pub fn uptime(&self) -> Duration {
        // Marker for parity with future debugging needs; not used yet.
        Instant::now().duration_since(Instant::now())
    }

    /// Stop the HTTP listener, leaving `state` (in-memory SQLite, users,
    /// changesets, wrapped keys) intact. Tests that exercise the
    /// "client points at a dead port" path call this to simulate a
    /// network partition without losing server-side state.
    #[allow(dead_code)]
    pub fn stop_http(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    /// Bring the HTTP listener back up on a fresh port, reusing the same
    /// `AppState`. Mirrors a real relay restart where the on-disk store
    /// survives the process bounce. Updates `base_url` to the new port.
    #[allow(dead_code)]
    pub fn restart_http(&mut self) {
        // Tear down the existing listener if it's still running.
        self.stop_http();

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<String>();
        let state_for_server = self.state.clone();

        let handle = std::thread::Builder::new()
            .name("datum-sync-test-server".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build runtime");
                rt.block_on(async move {
                    let app = build_app(state_for_server);
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                        .await
                        .expect("bind");
                    let addr = listener.local_addr().expect("local addr");
                    ready_tx.send(format!("http://{addr}")).expect("ready");
                    axum::serve(listener, app)
                        .with_graceful_shutdown(async {
                            let _ = shutdown_rx.await;
                        })
                        .await
                        .expect("serve");
                });
            })
            .expect("spawn server thread");

        self.base_url = ready_rx.recv().expect("server ready");
        self.shutdown = Some(shutdown_tx);
        self.handle = Some(handle);
    }

    /// Iterate `(device_id, seq, raw_payload_b64)` for every changeset
    /// the server has stored. Tests use this to assert ciphertext (no
    /// plaintext field names leak) and to round-trip the AES-GCM seal.
    #[allow(dead_code)]
    pub fn dump_raw_changesets(&self) -> Vec<(String, i64, String)> {
        let conn = self.state.db.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT device_id, seq, payload FROM changesets ORDER BY id")
            .expect("prepare");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .expect("query");
        rows.filter_map(|r| r.ok()).collect()
    }
}

impl Drop for StubRelay {
    fn drop(&mut self) {
        self.auth_stop.store(true, Ordering::SeqCst);
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        if let Some(h) = self.auth_handle.take() {
            let _ = h.join();
        }
    }
}
