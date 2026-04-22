//! Crypto primitives — AES-256-GCM changeset seal, Argon2id passphrase key
//! wrap, plus a reserved slot for future X25519 device-pairing material.
//!
//! Pure Rust, no I/O, no datum-core dependency. All three submodules are
//! WASM-safe; enable them unconditionally on every target.

pub mod changeset;
pub mod key_wrap;
pub mod pair;

/// Crate-internal error type for every crypto failure. All variants collapse
/// to a single string because callers MUST NOT branch on the reason —
/// distinguishing wrong-key from tampered-ciphertext leaks information
/// useful to attackers (padding-oracle family).
#[derive(Debug, Clone, thiserror::Error)]
#[error("crypto: {0}")]
pub struct CryptoError(pub String);
