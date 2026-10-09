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
//! * `runtime` (feature `parity-runtime`) — the `DatumRuntime` wrapper
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
pub const DATUM_PINNED_SHA: &str = "f5e7c1b82da18fb6f02d21efea67d40f9cf26f1d";

/// The IANA zone the text goldens were captured in, forced into every
/// process that renders one — v2 under `parity-runtime`, daydumb in the
/// normal suite. This is half of what makes the goldens reproducible, the
/// other half being the `-d`/`-t` flags every fixture passes (see
/// `runtime`'s "Time pinning").
pub const GOLDEN_TZ: &str = "Europe/Berlin";

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

#[cfg(test)]
mod tests {
    /// The pin lives in two files, and `setup_datum.sh`'s own header
    /// promises a mismatch is "rejected loudly". Nothing rejected
    /// anything: the two were free to drift, and a drifted pin means the
    /// goldens are checked against a revision nobody recorded.
    ///
    /// This runs on the default feature set, so it is one of the few
    /// things about the gated path that a plain `cargo test` still
    /// guards.
    #[test]
    fn pinned_sha_agrees_with_setup_script() {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scripts")
            .join("setup_datum.sh");
        let body = std::fs::read_to_string(&script)
            .unwrap_or_else(|e| panic!("read {}: {e}", script.display()));
        let line = body
            .lines()
            .find(|l| l.starts_with("PINNED_SHA="))
            .unwrap_or_else(|| panic!("no PINNED_SHA= line in {}", script.display()));
        let sha = line
            .trim_start_matches("PINNED_SHA=")
            .trim_matches('"')
            .trim();
        assert_eq!(
            sha,
            super::DATUM_PINNED_SHA,
            "setup_datum.sh pins {sha} but DATUM_PINNED_SHA is {}; \
             bump both together (see README § Pinned SHA)",
            super::DATUM_PINNED_SHA
        );
    }
}
