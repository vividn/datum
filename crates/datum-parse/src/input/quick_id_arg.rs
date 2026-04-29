//! `quickIdArg` — positional `quickId` + `--on-ambiguous`. Port of datum's
//! `src/input/quickIdArg.ts`.
//!
//! The actual resolution (prefix matching, `_`, wildcards) lives in
//! [`datum_core::ids::resolve`][resolve]; this module just owns the
//! parsed argv shape so step 3/N can assemble it into a `CommandIntent`.
//!
//! `OnAmbiguous` exists in `datum-commands` today only as a per-caller
//! ad-hoc decision; lifting the enum here (WASM-safe, no DB dep) is the
//! first half of the centralisation the parity audit calls for — see
//! "`onAmbiguous` unification" under "Improvement ideas (post-R2)" in
//! `docs/parity-audit/parsing.md`.
//!
//! [resolve]: https://docs.rs/datum-core/latest/datum_core/ids/resolve/index.html

use std::str::FromStr;

use crate::error::ParseError;
use crate::input::flag_registry::{FlagAction, FlagSpec};

/// Canonical flag set for the quick-id concern.
///
/// The positional `quick_id` is captured by the parser's positional path
/// (not a flag); this only declares `--on-ambiguous`.
pub static FLAG_SPECS: &[FlagSpec<QuickIdArg>] = &[
    FlagSpec {
        short: None,
        long: Some("on-ambiguous"),
        action: FlagAction::Value(|a, v| {
            a.on_ambiguous = v
                .parse::<OnAmbiguous>()
                .map_err(ParseError::InvalidArg)?;
            Ok(())
        }),
    },
];

/// What to do when a quickId resolves to more than one live entry.
/// Mirrors datum's `ON_AMBIGUOUS_QUICK_ID` tuple, default `Fail`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OnAmbiguous {
    /// Act on every match.
    All,
    /// Act on the lowest-sorted id only.
    First,
    /// Act on the highest-sorted id only.
    Last,
    /// Raise an error listing the candidates. Datum's default.
    #[default]
    Fail,
    /// Prompt the user to pick a match. CLI-only — non-CLI callers should
    /// treat this as equivalent to `Fail`.
    Ask,
}

impl FromStr for OnAmbiguous {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "all" => Ok(OnAmbiguous::All),
            "first" => Ok(OnAmbiguous::First),
            "last" => Ok(OnAmbiguous::Last),
            "fail" => Ok(OnAmbiguous::Fail),
            "ask" => Ok(OnAmbiguous::Ask),
            other => Err(format!("unknown --on-ambiguous value: {other}")),
        }
    }
}

impl OnAmbiguous {
    /// The `--on-ambiguous` lowercase wire form. Useful for round-tripping
    /// a parsed intent back into a canonicalised string command.
    pub fn as_str(self) -> &'static str {
        match self {
            OnAmbiguous::All => "all",
            OnAmbiguous::First => "first",
            OnAmbiguous::Last => "last",
            OnAmbiguous::Fail => "fail",
            OnAmbiguous::Ask => "ask",
        }
    }
}

/// Parsed shape of `quickId` + `--on-ambiguous`.
///
/// Per datum's help text: `quickId` may be prefix of `hid`, a `slug`, a
/// comma-prefixed/suffixed list (`",id1,id2"`), or one of the underscore
/// shortcuts (`_`, `__`, `_alcohol`, …). This crate doesn't validate the
/// pattern — that's the resolver's job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickIdArg {
    /// `quickId` positional. `None` means "use last-touched".
    pub quick_id: Option<String>,
    /// `--on-ambiguous` disposition. Defaults to [`OnAmbiguous::Fail`].
    pub on_ambiguous: OnAmbiguous,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_no_id_and_fail_on_ambiguous() {
        let a = QuickIdArg::default();
        assert_eq!(a.quick_id, None);
        assert_eq!(a.on_ambiguous, OnAmbiguous::Fail);
    }

    #[test]
    fn on_ambiguous_parses_all_variants() {
        assert_eq!("all".parse::<OnAmbiguous>().unwrap(), OnAmbiguous::All);
        assert_eq!("first".parse::<OnAmbiguous>().unwrap(), OnAmbiguous::First);
        assert_eq!("last".parse::<OnAmbiguous>().unwrap(), OnAmbiguous::Last);
        assert_eq!("fail".parse::<OnAmbiguous>().unwrap(), OnAmbiguous::Fail);
        assert_eq!("ask".parse::<OnAmbiguous>().unwrap(), OnAmbiguous::Ask);
    }

    #[test]
    fn on_ambiguous_is_case_insensitive() {
        assert_eq!("ALL".parse::<OnAmbiguous>().unwrap(), OnAmbiguous::All);
        assert_eq!("Ask".parse::<OnAmbiguous>().unwrap(), OnAmbiguous::Ask);
    }

    #[test]
    fn on_ambiguous_rejects_unknown() {
        assert!("banana".parse::<OnAmbiguous>().is_err());
    }

    #[test]
    fn on_ambiguous_round_trips_through_as_str() {
        for mode in [
            OnAmbiguous::All,
            OnAmbiguous::First,
            OnAmbiguous::Last,
            OnAmbiguous::Fail,
            OnAmbiguous::Ask,
        ] {
            assert_eq!(mode.as_str().parse::<OnAmbiguous>().unwrap(), mode);
        }
    }

    #[test]
    fn compound_comma_patterns_pass_through_unparsed() {
        // The resolver decides what ",a,b," means; we just carry the
        // string verbatim here.
        let a = QuickIdArg {
            quick_id: Some(",id1,id2,".into()),
            on_ambiguous: OnAmbiguous::First,
        };
        assert_eq!(a.quick_id.as_deref(), Some(",id1,id2,"));
        assert_eq!(a.on_ambiguous, OnAmbiguous::First);
    }
}
