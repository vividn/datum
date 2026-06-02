//! Alias expansion (DB-backed + spec-driven).
//!
//! In R2 step 3/N the alias pre-pass currently living in
//! `datum-commands/src/args.rs` (`try_expand_alias`, `expand_argv`) moves
//! here so the bespoke parser can run it before producing a
//! [`CommandIntent`]. The lookup is polymorphic over the alias source so
//! the WASM build can plug a non-SQLite resolver (e.g. an in-memory map
//! populated by the web app).
//!
//! Note: spec-derived alias autogen lives on `FieldSpec::
//! regenerate_derived_aliases` in `datum-core::field_spec::spec`, not in
//! this module. The originally-planned `alias::autogen` placeholder was
//! removed once it became clear the canonical home was the spec itself.

pub mod body;
pub mod expand;
pub mod scope;

pub use body::{
    build_slot_list, parse_alias_body, walk_slots, AliasBody, FieldDecl, GlobalSlot, KeyDecl,
    KeyForm, SideBinding, SlotKind,
};
pub use expand::{
    apply_terminal_verb_override, expand_argv, expand_argv_no_alias, expand_stacked_flags,
    maybe_collapse_switch_chain, maybe_swap_field_verb, shell_join, shell_split,
    split_argv_at_commas, try_expand_alias, AliasStep, MapResolver, NoResolver, ResolvedAlias,
    Resolver, KNOWN_SUBCOMMANDS, MAX_ALIAS_HOPS, MULTI_COMMA, TERMINAL_VERBS,
};
pub use scope::{expand_into_sides, verb_is_time_relevant, ExpandedSides, TimeFlags};
