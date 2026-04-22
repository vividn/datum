//! Alias expansion pre-pass — resolve the first argv token against an
//! alias table and record each expansion step on an `IntentContext`.
//!
//! Populated in R2 step 3/N (migration from
//! `datum-commands/src/args.rs::try_expand_alias`). The IO-free core of
//! the lookup (split by shell tokens, detect cycles, chain) lives here;
//! the DB-backed resolver stays in `datum-commands`.
