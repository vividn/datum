//! `quickIdArg` — positional `quickId`. Port of datum's
//! `src/input/quickIdArg.ts`.
//!
//! The actual resolution (prefix matching, `_`, wildcards) lives in
//! [`datum_core::ids::resolve`][resolve]; this module just owns the
//! parsed argv shape so step 3/N can assemble it into a `CommandIntent`.
//!
//! ## Wiring status (2026-06-03)
//!
//! Datum's `--on-ambiguous` enum was *not* carried over. The read-only
//! `get` command renders every match for an ambiguous quick-id; the
//! mutating commands (`del`, `update`, `edit`, `retime`) keep their
//! fail-with-candidates error as the disambiguation UX. The D5 conflict
//! flow will grow its own infrastructure rather than reuse a parse-side
//! enum, so no centralised `OnAmbiguous` lives here. `QuickIdArg` remains
//! the shared positional quick-id surface, and `FLAG_SPECS` is kept (empty)
//! so a future quick-id flag slots in without re-establishing the export.
//!
//! [resolve]: https://docs.rs/datum-core/latest/datum_core/ids/resolve/index.html

use crate::input::flag_registry::FlagSpec;

/// Canonical flag set for the quick-id concern.
///
/// Empty today: the positional `quick_id` is captured by the parser's
/// positional path (not a flag), and the quick-id concern declares no
/// flags of its own. Kept as an export so a future flag (e.g.
/// `--include-tombstoned`) slots in here without re-establishing it.
pub static FLAG_SPECS: &[FlagSpec<QuickIdArg>] = &[];

/// Parsed shape of `quickId`.
///
/// Per datum's help text: `quickId` may be prefix of `hid`, a `slug`, a
/// comma-prefixed/suffixed list (`",id1,id2"`), or one of the underscore
/// shortcuts (`_`, `__`, `_alcohol`, …). This crate doesn't validate the
/// pattern — that's the resolver's job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickIdArg {
    /// `quickId` positional. `None` means "use last-touched".
    pub quick_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_no_id() {
        let a = QuickIdArg::default();
        assert_eq!(a.quick_id, None);
    }

    #[test]
    fn compound_comma_patterns_pass_through_unparsed() {
        // The resolver decides what ",a,b," means; we just carry the
        // string verbatim here.
        let a = QuickIdArg {
            quick_id: Some(",id1,id2,".into()),
        };
        assert_eq!(a.quick_id.as_deref(), Some(",id1,id2,"));
    }
}
