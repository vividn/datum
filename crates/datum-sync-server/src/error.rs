use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use datum_sync_protocol::SyncProtocolError;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("jwt error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("webauthn error: {0}")]
    WebAuthn(#[from] webauthn_rs::prelude::WebauthnError),

    #[error("unauthorized")]
    Unauthorized,

    #[error("not found")]
    NotFound,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("internal error: {0}")]
    Internal(String),

    /// da-tat1: client's view of the cluster generation disagrees with
    /// the server's. Surfaces as HTTP 409 with the tagged-enum payload
    /// so clients route on `kind` (per [`SyncProtocolError`]'s
    /// serde tag).
    #[error(transparent)]
    GenerationMismatch(SyncProtocolError),

    /// da-ca4q: the client's predicted per-device seq disagrees with the
    /// server's authoritative next seq. Surfaces as HTTP 409 with the
    /// same tagged-enum payload so clients route on `kind`; unlike
    /// GenerationMismatch the client recovers in-band (re-base,
    /// re-encrypt, retry) rather than re-initialising.
    #[error(transparent)]
    SeqMismatch(SyncProtocolError),
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // GenerationMismatch and SeqMismatch emit a structured wire-error
        // body (the protocol crate's SyncProtocolError serialization) as
        // HTTP 409 so clients can route on `kind`. Other variants stay on
        // the legacy `{ "error": "..." }` shape until they grow
        // protocol-crate counterparts.
        if let AppError::GenerationMismatch(ref err) | AppError::SeqMismatch(ref err) = self {
            return (StatusCode::CONFLICT, Json(err)).into_response();
        }
        let (status, message) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::GenerationMismatch(_) | AppError::SeqMismatch(_) => {
                unreachable!("handled above")
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}
