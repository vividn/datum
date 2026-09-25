//! Id-space utilities — canonical port of datum's `src/ids/`.
//!
//! Today this module owns the quickId resolver (see [`resolve`]). Other
//! id-layer concerns (min-human-id shortening, id-structure assembly) live
//! in their own modules in the original TS tree and will be ported in
//! follow-up beads.

pub mod resolve;
pub mod types;

pub use resolve::{
    contains_wildcard, parse_underscore_ordinal, quick_ordinals, resolve_quickid,
    resolve_quickid_all, resolve_quickid_single,
};
pub use types::{Hid, Slug};

/// How deep the `_N` shorthand counts. Two digits is where it stops being
/// something anyone reaches for — past `_99` the hid sitting in the next
/// column is the handle you'd actually type — so the display surfaces stop
/// offering a short form there.
///
/// Ungated, unlike [`quick_ordinals`], because the browser assigns the same
/// ordinals over the rows it holds rather than over a database (da-6ec2),
/// and one cap has to govern both.
pub const QUICK_ORDINAL_CAP: usize = 99;
