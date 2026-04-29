//! Crypto primitives — AES-256-GCM changeset seal, Argon2id passphrase key
//! wrap, plus a reserved slot for future X25519 device-pairing material.
//!
//! Pure Rust, no I/O, no datum-core dependency. All three submodules are
//! WASM-safe; enable them unconditionally on every target.
//!
//! The `CryptoError` type and wire-locked constants (`NONCE_LEN`,
//! `CHANGESET_MAGIC_V1`) live in `datum-sync-protocol` so non-Rust clients
//! and a future WASM build can target them without depending on this
//! crate's actual crypto implementation. They are re-exported here so
//! existing `crate::crypto::CryptoError` paths keep working unchanged.

pub mod changeset;
pub mod key_wrap;
pub mod pair;

pub use datum_sync_protocol::crypto::CryptoError;
