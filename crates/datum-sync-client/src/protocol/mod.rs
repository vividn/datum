//! Wire protocol — serde types that travel between clients and the
//! `datum-sync-server` relay. Pure: no HTTP, no I/O, no datum-core deps.
//!
//! The JSON shape is the contract; see `docs/sync-protocol.md`. Keep this
//! module dependency-free so it compiles on every target the project cares
//! about (native, wasm32, mobile FFI).

pub mod messages;
