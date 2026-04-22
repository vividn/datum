//! datum-parse — pure-Rust parsing primitives for the datum CLI.
//!
//! This crate owns:
//! * Time parsing (`time/`) — date/time/duration string parsers, uncertainty,
//!   `DatumTime`, `PrecisionUnit`.
//! * (R2 step 2+) Bespoke argv parsing that replaces clap, producing a
//!   [`CommandIntent`] consumed by `datum-commands`.
//!
//! ## WASM-safety
//!
//! The crate builds for `wasm32-unknown-unknown` when default features are
//! disabled:
//!
//! ```bash
//! cargo build --target wasm32-unknown-unknown -p datum-parse --no-default-features
//! ```
//!
//! The `native` feature (default-on) enables `DatumTime::now()` via
//! `iana-time-zone` for local-timezone detection. Callers targeting WASM
//! must supply a timezone explicitly.

pub mod error;
pub mod time;

pub use error::{ParseError, Result};
pub use time::{
    DatumTime, DateArg, PrecisionUnit, TimeArg, TimeBuilder, Uncertainty,
};
