//! Crypto-related contract: opaque error type + wire-locked constants.
//! No actual crypto implementation — that lives in `datum-sync-client`
//! (and its WASM shim and any future native client). This module exists
//! so every implementation keys off the same nonce length, magic prefix,
//! and error contract.

pub mod constants;

pub use constants::{CHANGESET_MAGIC_V1, NONCE_LEN};

/// Opaque crypto failure. Variants intentionally collapse to a single
/// string — distinguishing wrong-key from tampered-ciphertext leaks
/// information useful to attackers (padding-oracle family).
#[derive(Debug, Clone, thiserror::Error)]
#[error("crypto: {0}")]
pub struct CryptoError(pub String);

impl CryptoError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}
