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

pub use expand::{
    apply_terminal_verb_override, expand_argv, expand_argv_no_alias, expand_stacked_flags,
    maybe_swap_field_verb, shell_join, shell_split, try_expand_alias, AliasStep, MapResolver,
    NoResolver, ResolvedAlias, Resolver, KNOWN_SUBCOMMANDS, MAX_ALIAS_HOPS, TERMINAL_VERBS,
};
