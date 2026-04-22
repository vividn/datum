//! `CommandIntent` — the intermediate representation produced by the
//! bespoke argv parser and consumed by the per-command dispatch layer in
//! `datum-commands`.
//!
//! Populated in R2 step 3/N. The per-variant shape mirrors what the
//! existing clap-derived arg structs carry, minus the clap attributes,
//! so the migration is mostly mechanical. See
//! `docs/parity-audit/parsing.md` for the behavior inventory.
