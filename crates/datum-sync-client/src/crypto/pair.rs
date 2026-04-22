//! X25519 device-pairing material (reserved for future work).
//!
//! The original plan (see `docs/sync-protocol.md`) calls for a short-lived
//! X25519 key agreement during add-a-device flows — one device generates a
//! pair, prints a fingerprint to scan, and the peer uses the shared secret
//! to wrap the master key for transport. Nothing here yet: the module is
//! a scaffold so `crypto/` has the shape the R4 refactor locked in.
//!
//! When this module grows real code, keep it WASM-safe: no I/O, no reqwest,
//! no filesystem. The only heavy dependency allowed is the `x25519-dalek`
//! crate (or similar) gated off a feature flag if the compile-size cost is
//! material.
