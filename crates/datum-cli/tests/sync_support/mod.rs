//! Tiny axum-based stub relay used by sync integration tests.
//!
//! Exposes the subset of the wire protocol that this bead's CLI flows talk
//! to: `/v1/auth/device-code`, `/v1/auth/device-code/poll`, and the wrapped
//! key round-trip. State is held in memory; each test owns its own instance.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Process-wide lock for tests that mutate `DATUM_HOME` / `DATUM_CONFIG_HOME`
/// / `DATUM_DB`. All integration tests across files MUST serialise through
/// this lock — otherwise one test file's `EnvRestore::drop` can unset the
/// env var that another file's test is mid-flight relying on.
#[allow(dead_code)]
pub static ENV_LOCK: Mutex<()> = Mutex::new(());

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value as JsonValue;

pub struct StubRelay {
    pub base_url: String,
    pub state: Arc<RelayState>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct RelayState {
    pub inner: Mutex<RelayInner>,
}

#[derive(Default)]
pub struct RelayInner {
    pub device_codes: HashMap<String, DeviceCodeEntry>,
    pub wrapped: Option<JsonValue>,
    pub poll_counts: HashMap<String, usize>,
    pub slow_down_left: usize,
    pub user_id: String,
    /// Stored changesets keyed by relay-assigned global id (simulates
    /// `changesets.id` AUTOINCREMENT in the production server).
    pub changesets: Vec<StoredChangeset>,
}

#[derive(Debug, Clone)]
pub struct StoredChangeset {
    pub id: i64,
    pub device_id: String,
    pub seq: i64,
    pub payload_b64: String,
}

#[derive(Debug, Clone)]
pub struct DeviceCodeEntry {
    #[allow(dead_code)]
    pub user_code: String,
    pub authorized: bool,
    pub authorize_after_polls: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct PollParams {
    pub device_code: String,
}

#[derive(Debug, Clone)]
pub struct StubRelayOptions {
    pub user_id: String,
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
        let state = Arc::new(RelayState::default());
        {
            let mut inner = state.inner.lock().unwrap();
            inner.user_id = opts.user_id.clone();
            inner.slow_down_left = opts.slow_down_polls;
        }

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<String>();
        let state_clone = state.clone();

        let handle = std::thread::Builder::new()
            .name("stub-relay".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build runtime");
                rt.block_on(async move {
                    let app = build_app(state_clone);
                    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                        .await
                        .expect("bind");
                    let addr = listener.local_addr().unwrap();
                    ready_tx.send(format!("http://{addr}")).unwrap();
                    axum::serve(listener, app)
                        .with_graceful_shutdown(async {
                            let _ = shutdown_rx.await;
                        })
                        .await
                        .expect("serve");
                });
            })
            .expect("spawn relay thread");

        let base_url = ready_rx.recv().expect("relay ready");
        Self {
            base_url,
            state,
            shutdown: Some(shutdown_tx),
            handle: Some(handle),
        }
    }

    #[allow(dead_code)]
    pub fn wrapped_key(&self) -> Option<JsonValue> {
        self.state.inner.lock().unwrap().wrapped.clone()
    }

    #[allow(dead_code)]
    pub fn set_wrapped_key(&self, wrapped: JsonValue) {
        self.state.inner.lock().unwrap().wrapped = Some(wrapped);
    }

    /// Inject an already-encrypted blob as if a peer device had pushed
    /// it. Used by tests that exercise the pull path without going
    /// through `push_blob` first. `seq` is the per-device seq the AAD was
    /// sealed with.
    #[allow(dead_code)]
    pub fn inject_changeset(&self, device_id: &str, seq: i64, payload: &[u8]) {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        let mut inner = self.state.inner.lock().unwrap();
        let next_id = inner.changesets.len() as i64 + 1;
        inner.changesets.push(StoredChangeset {
            id: next_id,
            device_id: device_id.to_string(),
            seq,
            payload_b64: URL_SAFE_NO_PAD.encode(payload),
        });
    }

    /// Count of encrypted changesets the relay currently holds.
    #[allow(dead_code)]
    pub fn changeset_count(&self) -> usize {
        self.state.inner.lock().unwrap().changesets.len()
    }
}

impl Drop for StubRelay {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn build_app(state: Arc<RelayState>) -> Router {
    Router::new()
        .route("/v1/auth/device-code", post(device_code_start))
        .route("/v1/auth/device-code/poll", get(device_code_poll))
        .route("/v1/keys/wrapped", post(put_wrapped).get(get_wrapped))
        .route("/v1/push", post(push_changeset))
        .route("/v1/pull", get(pull_changesets))
        .with_state(state)
}

async fn device_code_start(
    State(state): State<Arc<RelayState>>,
    Json(_body): Json<JsonValue>,
) -> impl IntoResponse {
    // Deterministic-ish device_code per request.
    let code = format!(
        "dev-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let user_code = "ABCD-1234".to_string();
    {
        let mut inner = state.inner.lock().unwrap();
        inner.device_codes.insert(
            code.clone(),
            DeviceCodeEntry {
                user_code: user_code.clone(),
                authorized: false,
                // Default: authorize on the 2nd poll.
                authorize_after_polls: Some(2),
            },
        );
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "device_code": code,
            "user_code": user_code,
            "verification_uri": "https://sync.example.com/activate",
            "verification_uri_complete": "https://sync.example.com/activate?code=ABCD-1234",
            "expires_in": 600,
            "interval": 0,
        })),
    )
}

async fn device_code_poll(
    State(state): State<Arc<RelayState>>,
    Query(q): Query<PollParams>,
) -> impl IntoResponse {
    let mut inner = state.inner.lock().unwrap();

    if inner.slow_down_left > 0 {
        inner.slow_down_left -= 1;
        return (
            StatusCode::OK,
            Json(serde_json::json!({ "error": "slow_down" })),
        );
    }

    let count = inner.poll_counts.entry(q.device_code.clone()).or_insert(0);
    *count += 1;
    let count = *count;

    let user_id = inner.user_id.clone();
    let Some(entry) = inner.device_codes.get_mut(&q.device_code) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "expired" })),
        );
    };

    if !entry.authorized {
        if let Some(after) = entry.authorize_after_polls {
            if count >= after {
                entry.authorized = true;
            }
        }
    }

    if entry.authorized {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "token": format!("bearer-for-{}", q.device_code),
                "user_id": user_id,
            })),
        )
    } else {
        (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "pending" })),
        )
    }
}

async fn put_wrapped(
    State(state): State<Arc<RelayState>>,
    Json(body): Json<JsonValue>,
) -> impl IntoResponse {
    state.inner.lock().unwrap().wrapped = Some(body);
    (StatusCode::OK, Json(serde_json::json!({})))
}

async fn get_wrapped(State(state): State<Arc<RelayState>>) -> impl IntoResponse {
    let guard = state.inner.lock().unwrap();
    match &guard.wrapped {
        Some(v) => (StatusCode::OK, Json(v.clone())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "not_found" })),
        )
            .into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct PushBody {
    pub device_id: String,
    pub payload_b64: String,
}

#[derive(Debug, Deserialize)]
pub struct PullParams {
    pub since: i64,
    pub device: String,
}

async fn push_changeset(
    State(state): State<Arc<RelayState>>,
    Json(body): Json<PushBody>,
) -> impl IntoResponse {
    let mut inner = state.inner.lock().unwrap();
    let next_seq = inner
        .changesets
        .iter()
        .filter(|c| c.device_id == body.device_id)
        .map(|c| c.seq)
        .max()
        .unwrap_or(0)
        + 1;
    let next_id = inner.changesets.len() as i64 + 1;
    inner.changesets.push(StoredChangeset {
        id: next_id,
        device_id: body.device_id,
        seq: next_seq,
        payload_b64: body.payload_b64,
    });
    (StatusCode::OK, Json(serde_json::json!({ "seq": next_seq })))
}

async fn pull_changesets(
    State(state): State<Arc<RelayState>>,
    Query(q): Query<PullParams>,
) -> impl IntoResponse {
    let inner = state.inner.lock().unwrap();
    let rows: Vec<JsonValue> = inner
        .changesets
        .iter()
        .filter(|c| c.id > q.since && c.device_id != q.device)
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "device_id": c.device_id,
                "seq": c.seq,
                "payload_b64": c.payload_b64,
                "uploaded_at": "2026-04-22T12:00:00Z",
            })
        })
        .collect();
    (StatusCode::OK, Json(rows))
}
