#![doc = include_str!("../README.md")]

//! datum-parse — pure-Rust parsing primitives for the datum CLI.
//!
//! This crate owns:
//! * Time parsing ([`time`]) — date/time/duration string parsers,
//!   uncertainty, `DatumTime`, `PrecisionUnit`.
//! * Argv parsing primitives ([`input`]) — port of datum's
//!   `src/input/*.ts` (flexiblePositional, consolidateKeys, dataArgs, …).
//! * The bespoke CLI parser ([`parse`]) and its intent IR ([`cli`]):
//!   owns argv parsing for the `datum` binary.
//! * Alias expansion ([`alias`]) — pre-pass that resolves DB-backed and
//!   spec-generated aliases before the parser runs. Callers plug in a
//!   [`alias::Resolver`] implementation.
//! * Hand-authored help rendering ([`help`]).
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

pub mod alias;
pub mod cli;
pub mod error;
pub mod help;
pub mod input;
pub mod intent;
pub mod parse;
pub mod time;

pub use cli::{Cli, Command};
pub use error::{ParseError, Result};
pub use input::{consolidate_keys, flexible_positional, FlexibleTarget};
pub use parse::{parse_argv, parse_argv_string, shell_split};
pub use time::{
    DatumTime, DateArg, PrecisionUnit, TimeArg, TimeBuilder, Uncertainty,
};
