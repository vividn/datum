//! Typed HTTP errors for the sync client.
//!
//! `Unauthorized` is split out as its own variant so the caller's
//! retry-on-401 path (see [`crate::http::session::Session`]) can match
//! on it without string-sniffing an opaque error message. Every other
//! failure mode collapses into `Other(anyhow::Error)` because the retry
//! logic doesn't care about the difference.

use thiserror::Error;

/// Errors returned by every authenticated HTTP helper in this crate.
#[derive(Debug, Error)]
pub enum HttpError {
    /// The relay returned 401. The Session retry loop catches this and
    /// trades the refresh token for a fresh bearer; CLI/agent callers
    /// see it bubbled up only when the refresh itself fails.
    #[error("unauthorized — bearer token rejected by relay")]
    Unauthorized,
    /// Anything else: network failure, non-401 status, decode error.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl HttpError {
    /// Build an `Other` from anything that converts into anyhow.
    pub fn other<E: Into<anyhow::Error>>(e: E) -> Self {
        HttpError::Other(e.into())
    }
}
