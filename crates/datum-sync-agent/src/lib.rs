//! `datum-sync-agent` — reserved for C4d (`da-ndd.15`).
//!
//! Scope (planned): a background daemon that wraps `datum-sync-client` to
//! perform periodic push/pull and conflict-queue drain without requiring
//! the CLI to be open. Anchors the `client` + `server` + `agent` split
//! that R4 locked in.
//!
//! Nothing is implemented yet — the directory, Cargo.toml, and this lib.rs
//! exist so the crate name is reserved and downstream refactors can rely
//! on it being in the workspace graph.
