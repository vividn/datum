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
}

pub type Result<T> = std::result::Result<T, ParseError>;
