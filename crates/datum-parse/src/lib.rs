#![doc = include_str!("../README.md")]

//! datum-parse — pure-Rust parsing primitives for the datum CLI.
//!
//! This crate owns:
//! * Time parsing ([`time`]) — date/time/duration string parsers,
//!   uncertainty, `DatumTime`, `UncertaintyUnit`.
//! * Argv parsing primitives ([`input`]) — port of datum's
//!   `src/input/*.ts` (flexiblePositional, consolidateKeys, dataArgs, …).
//! * The bespoke CLI parser ([`parse`]) and its intent IR ([`cli`]):
//!   owns argv parsing for the `datum` binary.
//! * Alias expansion ([`alias`]) — pre-pass that resolves DB-backed and
//!   spec-generated aliases before the parser runs. Callers plug in a
//!   [`alias::Resolver`] implementation.
//! * Hand-authored help rendering ([`help`]).
//! * Public error type ([`error`]) — `ParseError` and the shared
//!   `Result` alias for callers that want to match on the variants.
//!
//! ## WASM-safety
//!
//! The crate builds for `wasm32-unknown-unknown` when default features
//! are disabled:
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown -p datum-parse --no-default-features
//! ```
//!
//! The `native` feature (default-on) enables `DatumTime::now()` via
//! `iana-time-zone` for local-timezone detection. Callers targeting
//! WASM must supply a timezone explicitly.

// Turning `native` off on a host target compiles cleanly and yields a binary
// with dead zone detection — every local time silently resolved to UTC, and
// nothing anywhere said so. `--no-default-features` is for wasm; on any other
// target it is a mistake, so make it one (da-2pgd).
#[cfg(all(not(feature = "native"), not(target_arch = "wasm32")))]
compile_error!(
    "datum-parse: the `native` feature is required off wasm — without it local \
     timezone detection is compiled out and every host time resolves to UTC. \
     Disable default features only together with \
     `--target wasm32-unknown-unknown`."
);

pub mod alias;
pub mod cli;
pub mod error;
pub mod help;
pub mod input;
pub mod parse;
pub mod time;

pub use alias::expand::shell_split;
pub use cli::{Cli, Command};
pub use error::{ParseError, Result};
pub use input::{consolidate_keys, flexible_positional, FlexibleTarget};
pub use parse::{parse_argv, parse_argv_string};
pub use time::{DateArg, DatumTime, TimeArg, TimeBuilder, Uncertainty, UncertaintyUnit};
