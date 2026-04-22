//! Alias expansion (DB-backed + spec-driven).
//!
//! In R2 step 3/N the alias pre-pass currently living in
//! `datum-commands/src/args.rs` (`try_expand_alias`, `expand_argv`) moves
//! here so the bespoke parser can run it before producing a
//! [`CommandIntent`]. The lookup is polymorphic over the alias source so
//! the WASM build can plug a non-SQLite resolver (e.g. an in-memory map
//! populated by the web app).

pub mod autogen;
pub mod expand;
