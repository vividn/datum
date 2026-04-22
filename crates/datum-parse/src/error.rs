//! Parse-layer error type.
//!
//! `datum-core` (and downstream crates) convert this into `DatumError`
//! transparently via `From`. The variants are intentionally narrow — this
//! crate only produces errors from string-parsing failures.

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("Invalid time: {0}")]
    InvalidTime(String),

    #[error("Invalid argument: {0}")]
    InvalidArg(String),

    /// `handleDataArgs` could not satisfy a `-k key` with no default.
    /// Mirrors datum's `MissingRequiredKeyError(path)`.
    #[error("Missing required key: {0}")]
    MissingRequiredKey(String),

    /// Positional data exceeded the declared keys with no remainder bucket
    /// in scope. Mirrors datum's `ExtraDataError([...remainders])`.
    #[error("Extra data with no matching key: {0:?}")]
    ExtraData(Vec<String>),
}

pub type Result<T> = std::result::Result<T, ParseError>;
