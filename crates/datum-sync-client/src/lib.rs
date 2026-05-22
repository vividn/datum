#![doc = include_str!("../README.md")]

//! `datum-sync-client` — the client-side library behind `datum sync` and the
//! forthcoming `datum-sync-agent` daemon. It owns the wire protocol types,
//! the on-device crypto, and the HTTP client; it does NOT own the relay
//! server (that lives in `datum-sync-server`) or any CLI surface (that lives
//! in `datum-commands`).
//!
//! # WASM split
//!
//! The crate is structured so a future `datum-web` build can depend on it
//! with the default features off and provide its own HTTP shim:
//!
//! | Module          | Features required | WASM-safe |
//! |-----------------|-------------------|-----------|
//! | `protocol`      | (none)            | ✓         |
//! | `crypto`        | (none)            | ✓         |
//! | `config`        | (none)            | ✓         |
//! | `ops`           | `db`              | native only (rusqlite) |
//! | `sync_state`    | `db`              | native only (rusqlite) |
//! | `http`          | `http`            | no (reqwest blocking) |
//!
//! Build the WASM-safe surface with
//! `cargo build --target wasm32-unknown-unknown -p datum-sync-client \
//!     --no-default-features --features wasm`.

pub mod auth;
pub mod config;
pub mod crypto;
pub mod protocol;

#[cfg(feature = "db")]
pub mod ops;

#[cfg(feature = "db")]
pub mod sync_state;

#[cfg(feature = "http")]
pub mod http;

#[cfg(all(feature = "http", feature = "db"))]
pub mod sync_once;

// Convenience top-level re-exports of the most commonly used types so
// downstream crates don't need to chase module paths. Keeping the list
// narrow on purpose — anything deeper should import explicitly.
pub use auth::PersistedAuthToken;
pub use crypto::changeset::{decrypt_changeset, encrypt_changeset, Changeset, EncryptedBlob};
pub use crypto::key_wrap::{
    key_fingerprint, unwrap_master_key, wrap_master_key, wrap_master_key_with_params, KdfParams,
    WrappedKey,
};
pub use protocol::messages::AuthToken;

// Re-export wire-error types so downstream crates (CLI surface in
// datum-commands) can match on them without adding a direct dep on the
// protocol crate. da-ws1q sub-5: the CLI rewrites
// SessionError::GenerationMismatch into a CLI-friendly instruction.
pub use datum_sync_protocol::SyncProtocolError;
