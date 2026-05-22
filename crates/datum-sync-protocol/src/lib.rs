//! `datum-sync-protocol` — wire-format contract shared by `datum-sync-server`
//! and every sync client (CLI, agent, web, mobile). Pure data: serde structs,
//! constants, opaque error type. No HTTP, no DB, no crypto implementation.
//!
//! This crate exists to eliminate wire-type drift. Before it, server and
//! client defined parallel structs for the same JSON shapes (see da-6hn);
//! the recent da-6x9 fix was a direct symptom. Single source of truth
//! prevents silent divergence.
//!
//! # Module map
//!
//! | Module | Contents |
//! |--------|----------|
//! | [`messages::auth`] | `AuthToken`, device-code request/response/poll |
//! | [`messages::sync`] | `PushRequest`, `PushResponse`, `Changeset`, `WrappedKey`, `KdfParams` |
//! | [`crypto`]         | `CryptoError`, wire-locked constants (`NONCE_LEN`, `CHANGESET_MAGIC_V1`) |
//! | [`errors`]         | `SyncProtocolError` — control-plane wire errors (`GenerationMismatch`) |
//! | [`version`]        | Protocol version constants (`PROTOCOL_VERSION`) |
//!
//! WebAuthn (register/login) types remain in `datum-sync-server::models`
//! because they are server-only per the da-6hn duplication map and pulling
//! `webauthn-rs` into a WASM-safe crate would defeat its purpose.

pub mod crypto;
pub mod errors;
pub mod messages;
pub mod version;

pub use crypto::CryptoError;
pub use errors::SyncProtocolError;
pub use messages::auth::{
    AuthToken, DeviceCodeAuthorizeRequest, DeviceCodePollQuery, DeviceCodeRequest,
    DeviceCodeResponse, PollResponse, RefreshTokenRequest,
};
pub use messages::sync::{Changeset, KdfParams, PushRequest, PushResponse, WrappedKey};
pub use version::{PROTOCOL_VERSION, PROTOCOL_VERSION_PATH};
