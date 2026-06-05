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
            // da-k3re: the 500 family must never echo internal error text
            // (SQL fragments, file paths, library messages) to the client.
            // Log the real Display server-side; return a fixed opaque body.
            _ => {
                tracing::error!(error = %self, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// da-k3re: the 500 family must return a fixed opaque body — internal
    /// error text (SQL, paths, ownership details) stays server-side.
    #[tokio::test]
    async fn internal_errors_get_opaque_500_body() {
        for err in [
            AppError::Internal("secret: /var/lib/relay.db".to_string()),
            AppError::from(rusqlite::Error::InvalidQuery),
        ] {
            let resp = err.into_response();
            assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(body_json(resp).await["error"], "internal error");
        }
    }

    /// Caller-actionable 4xx bodies keep their messages.
    #[tokio::test]
    async fn four_xx_bodies_keep_caller_actionable_messages() {
        let resp = AppError::NotFound.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(body_json(resp).await["error"], "not found");

        let resp = AppError::BadRequest("missing field: device_id".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert_eq!(body_json(resp).await["error"], "missing field: device_id");
    }
}
