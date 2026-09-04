//! Wire types — exact JSON shapes on the HTTP boundary. The relay has no
//! schema knowledge beyond this contract; see `docs/sync-protocol.md`.

pub mod auth;
pub mod sync;

/// Keeps the doc's "each wire type names the spec" claim true (da-yvu5).
#[cfg(test)]
mod pointers;
