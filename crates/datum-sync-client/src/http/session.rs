//! Per-call HTTP session with automatic bearer-token refresh.
//!
//! Background: every authenticated relay call previously took a borrowed
//! `bearer: &str` and bubbled a 401 as an opaque `anyhow::bail!`. When a
//! token expired, the next sync would fail with `push failed: 401 ...`
//! and never recover — the user had to re-run `datum sync login`. See
//! `docs/sync-protocol.md` §Authentication: "A 401 response means
//! 'reauthenticate'; the client should kick off the device-code flow
//! and retry once." This is the missing retry path.
//!
//! The `Session` owns the current `(bearer, refresh_token)` pair plus a
//! caller-provided `on_refresh` callback so the CLI / agent can persist
//! the rotated values to disk atomically. Each authenticated HTTP call
//! is wrapped with [`Session::run`]:
//!
//! 1. Run the operation with the current bearer.
//! 2. If it succeeds → return the value.
//! 3. If it returns [`HttpError::Unauthorized`] → call
//!    [`super::auth::refresh`], persist the new pair via `on_refresh`,
//!    retry the operation once.
//! 4. If the refresh itself returns `Unauthorized` → surface a typed
//!    error so the caller can prompt the user to re-login.
//! 5. If retry STILL returns `Unauthorized` → same prompt; the bearer
//!    we just minted was rejected which is unrecoverable without user
//!    action.

use std::cell::RefCell;

use reqwest::blocking::Client;

use crate::http::auth;
use crate::http::error::HttpError;
use crate::protocol::messages::AuthToken;

/// Outcome of a refresh attempt — exposed primarily for tests that
/// want to assert how many rotations a given run performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// Operation succeeded on the first try; no refresh attempted.
    NotNeeded,
    /// Refresh fired and the retried op succeeded.
    Rotated,
}

/// The "we tried to refresh and failed for good" error shape. Surfaced
/// by [`Session::run`] when neither the original 401 nor the post-
/// refresh 401 can be recovered.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(
        "session expired — run `datum sync login` again (refresh token rejected)"
    )]
    SessionExpired,
    #[error(
        "session expired — run `datum sync login` again (no refresh token persisted)"
    )]
    NoRefreshToken,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Callback invoked after a successful refresh so the caller can persist
/// the rotated `(bearer, refresh_token)` pair. The closure receives the
/// full new [`AuthToken`] (so persistence layers that store extra
/// metadata — `refreshed_at`, `server` — can do so in one shot).
pub type OnRefresh = Box<dyn Fn(&AuthToken) -> anyhow::Result<()> + Send + Sync + 'static>;

/// Owns the (bearer, refresh_token) pair for the lifetime of one sync
/// cycle and rotates them through `/v1/auth/refresh` when the relay
/// returns 401.
///
/// `RefCell` is used internally so [`Session::run`] can mutate the
/// stored token through a `&self` borrow — the closure passed to `run`
/// re-borrows the bearer string for each attempt.
pub struct Session {
    server: String,
    inner: RefCell<SessionInner>,
    on_refresh: Option<OnRefresh>,
    /// Tracks how many refreshes happened over the Session's lifetime.
    /// Tests assert against this; production callers ignore it.
    refresh_count: RefCell<u32>,
}

struct SessionInner {
    bearer: String,
    refresh_token: Option<String>,
}

impl Session {
    /// Build a new session from a freshly-loaded [`AuthToken`]. Callers
    /// that want auto-persistence on rotation should pass an `on_refresh`
    /// closure that re-writes the token file (mode 0600) atomically;
    /// passing `None` leaves persistence to the caller.
    pub fn new(server: &str, token: &AuthToken, on_refresh: Option<OnRefresh>) -> Self {
        Self {
            server: server.to_string(),
            inner: RefCell::new(SessionInner {
                bearer: token.token.clone(),
                refresh_token: token.refresh_token.clone(),
            }),
            on_refresh,
            refresh_count: RefCell::new(0),
        }
    }

    /// Number of times this Session has rotated tokens. Mostly for tests.
    pub fn refresh_count(&self) -> u32 {
        *self.refresh_count.borrow()
    }

    /// Snapshot the current bearer. Used by tests that want to assert
    /// the in-memory bearer matches what landed on disk after a rotate.
    pub fn current_bearer(&self) -> String {
        self.inner.borrow().bearer.clone()
    }

    /// Run an authenticated HTTP operation, refreshing the bearer
    /// automatically if the relay returns 401.
    ///
    /// `op` is given the current bearer for each attempt so it can
    /// rebuild the request after a rotation. It runs at most twice: once
    /// with the original bearer, and (if 401) once with a freshly-
    /// minted one. Anything other than `Unauthorized` short-circuits.
    pub fn run<F, T>(&self, client: &Client, mut op: F) -> Result<T, SessionError>
    where
        F: FnMut(&str) -> Result<T, HttpError>,
    {
        // Attempt 1 — current bearer.
        let bearer_snapshot = self.inner.borrow().bearer.clone();
        match op(&bearer_snapshot) {
            Ok(v) => return Ok(v),
            Err(HttpError::Unauthorized) => { /* fall through to refresh */ }
            Err(HttpError::Other(e)) => return Err(SessionError::Other(e)),
        }

        // Attempt 2 — refresh, then retry once.
        self.rotate_bearer(client)?;
        let new_bearer = self.inner.borrow().bearer.clone();
        match op(&new_bearer) {
            Ok(v) => Ok(v),
            // A second 401 right after a rotation almost certainly means
            // the user revoked the credential server-side. Treat it as a
            // session-expired prompt rather than a transient network
            // issue.
            Err(HttpError::Unauthorized) => Err(SessionError::SessionExpired),
            Err(HttpError::Other(e)) => Err(SessionError::Other(e)),
        }
    }

    /// Trade the stored refresh token for a fresh AuthToken. Updates the
    /// in-memory session and notifies `on_refresh` so the caller can
    /// persist the rotation.
    fn rotate_bearer(&self, client: &Client) -> Result<(), SessionError> {
        let refresh_token = self
            .inner
            .borrow()
            .refresh_token
            .clone()
            .ok_or(SessionError::NoRefreshToken)?;

        let new = match auth::refresh(client, &self.server, &refresh_token) {
            Ok(t) => t,
            Err(HttpError::Unauthorized) => return Err(SessionError::SessionExpired),
            Err(HttpError::Other(e)) => return Err(SessionError::Other(e)),
        };

        {
            let mut guard = self.inner.borrow_mut();
            guard.bearer = new.token.clone();
            guard.refresh_token = new.refresh_token.clone();
        }
        *self.refresh_count.borrow_mut() += 1;

        if let Some(cb) = &self.on_refresh {
            cb(&new).map_err(SessionError::Other)?;
        }
        Ok(())
    }
}
