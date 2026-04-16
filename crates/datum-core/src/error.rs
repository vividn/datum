use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatumError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid time: {0}")]
    InvalidTime(String),

    #[error("Invalid field spec: {0}")]
    InvalidFieldSpec(String),

    #[error("Duplicate entry: {0}")]
    Duplicate(String),

    #[error("Entry not found: {0}")]
    NotFound(String),

    #[error("Missing required field: {0}")]
    MissingRequired(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, DatumError>;
