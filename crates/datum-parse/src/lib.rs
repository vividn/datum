//! datum-parse — pure-Rust parsing primitives for the datum CLI.
//!
//! This crate owns:
//! * Time parsing ([`time`]) — date/time/duration string parsers,
//!   uncertainty, `DatumTime`, `PrecisionUnit`.
//! * Argv parsing primitives ([`input`]) — port of datum's
//!   `src/input/*.ts`. In R2 step 3/N this will drive the bespoke parser
//!   that replaces clap.
//! * Alias expansion ([`alias`]) — pre-pass that resolves DB-backed and
//!   spec-generated aliases before the parser runs.
//! * [`CommandIntent`][intent] — the IR produced by the parser and
//!   consumed by `datum-commands` (step 3/N).
//! * Bespoke help rendering ([`help`]) — step 3/N.
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
pub mod error;
pub mod help;
pub mod input;
pub mod intent;
pub mod time;

pub use error::{ParseError, Result};
pub use input::{consolidate_keys, flexible_positional, FlexibleTarget};
pub use time::{
    DatumTime, DateArg, PrecisionUnit, TimeArg, TimeBuilder, Uncertainty,
};
