//! Id-space utilities — canonical port of datum's `src/ids/`.
//!
//! Today this module owns the quickId resolver (see [`resolve`]). Other
//! id-layer concerns (min-human-id shortening, id-structure assembly) live
//! in their own modules in the original TS tree and will be ported in
//! follow-up beads.

pub mod resolve;
pub mod types;

pub use resolve::{
    contains_wildcard, parse_underscore_ordinal, resolve_quickid, resolve_quickid_all,
    resolve_quickid_single,
};
pub use types::{Hid, Slug};
