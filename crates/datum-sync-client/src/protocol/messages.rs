//! Wire types — re-exports from `datum-sync-protocol`.
//!
//! Before da-6hn PR1, this module defined parallel structs to the server's
//! `models.rs`; the recent da-6x9 fix was a direct symptom of that drift.
//! The structs now live exactly once, in `datum_sync_protocol::messages`.

pub use datum_sync_protocol::messages::auth::{AuthToken, DeviceCodeResponse, PollResponse};
