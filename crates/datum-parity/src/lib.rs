//! Cross-runtime parity tests for daydumb against the original datum
//! (vividn/datum, TypeScript).
//!
//! The class of bug we keep shipping (md5 colors `da-438y`, switch parser
//! `da-ffp6`, `_` SQL `da-luc8`, nowview drift `da-aw2o`, `-tq` cumulative
//! time `da-jp0b`) is exactly the class that golden tests against the
//! *real* datum reference would catch. This crate exists to be that
//! reference.
//!
//! ## What's here
//!
//! * [`colors`] — the md5 / contrast-text color algorithms reimplemented
//!   from datum's `src/utils/md5Color.ts` and `src/utils/colorUtils.ts`,
//!   used to validate daydumb's renderer (`crates/datum-commands/src/render/colors.rs`)
//!   against datum's source. **No runtime needed**: the algorithms are
//!   small enough that comparing implementations is more reliable than
//!   shelling out to node.
//! * [`myspec`] — a port of datum's `src/field/mySpecs.ts` to daydumb's
//!   kit format (`specs/myspec.toml`), so parity tests can install it
//!   into a daydumb DB and have spec overrides line up.
//! * [`runtime`] (feature `parity-runtime`) — the `DatumRuntime` wrapper
//!   that spawns the TS process. Off by default; CI must opt in.
//!
//! ## Layout
//!
//! ```text
//! crates/datum-parity/
//!   Cargo.toml          # workspace member, feature `parity-runtime`
//!   README.md           # setup, run, update goldens
//!   specs/
//!     myspec.toml       # ported from datum's src/field/mySpecs.ts
//!   goldens/
//!     colors/           # field/state name → expected hex (md5-derived)
//!     text/             # CLI output snapshots
//!     regressions/      # da-438y, da-ffp6, da-luc8, da-aw2o, da-jp0b
//!   scripts/
//!     setup_datum.sh    # clone + build datum at the pinned SHA
//!   src/
//!     lib.rs            # this file
//!     colors.rs         # md5 color algebra
//!     myspec.rs         # ported FIELD_SPECS
//!     runtime.rs        # DatumRuntime (feature-gated)
//! ```
//!
//! ## SHA pin
//!
//! Goldens are reproducible only against a specific datum revision. The
//! pinned SHA is [`DATUM_PINNED_SHA`]; `scripts/setup_datum.sh` checks
//! the clone out at that revision before running tests.

#![deny(missing_docs)]

pub mod colors;
pub mod myspec;

#[cfg(feature = "parity-runtime")]
pub mod runtime;

/// The vividn/datum commit goldens were captured against. Bump in lockstep
/// with `scripts/setup_datum.sh` and regenerate goldens via
/// `PARITY_REGEN=1 cargo test --features parity-runtime -p datum-parity`.
pub const DATUM_PINNED_SHA: &str = "c8599d84baa6d26a633ce375f0a28bb1f2a9294a";

/// Resolve the absolute path of the datum clone managed by
/// `scripts/setup_datum.sh`. Tests under `parity-runtime` use this to
/// locate the TS toolchain; pure-algorithm tests don't need it.
pub fn datum_clone_path() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("..")
        .join("..")
        .join("target")
        .join("parity-cache")
        .join("datum")
}
