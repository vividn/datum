//! Client-side authentication shapes that sit on top of the protocol
//! crate's wire [`crate::protocol::messages::AuthToken`].
//!
//! Today this holds the on-disk persistence wrapper (`PersistedAuthToken`,
//! da-ixmh); future additions (per-cluster token caches, token-source
//! provenance) will land here so the wire crate stays minimal.

pub mod persisted;

pub use persisted::PersistedAuthToken;
