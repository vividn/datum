//! `durationArgs` — positional `duration` + `-m/--moment`. Port of datum's
//! `src/input/durationArgs.ts`.
//!
//! The TS module wraps argparse; we carry the same two fields on a struct
//! and expose [`DurationArgs::build`] which collapses the flags into a
//! single [`DurationIntent`] value. Unlike the TS version we don't parse
//! the duration here — that lives in
//! [`crate::time::parse_duration`] — so the return type is just the
//! argparse shape expressed as an enum.
//!
//! Special values mirrored from datum:
//!
//! | Input                 | Result                        |
//! |-----------------------|-------------------------------|
//! | no `-m`, no positional| [`DurationIntent::Absent`]    |
//! | `duration = "."` or `""` | [`DurationIntent::Absent`] |
//! | `-m` / `--moment`     | [`DurationIntent::Moment`]    |
//! | anything else         | [`DurationIntent::Value(s)`]  |
//!
//! See `docs/parity-audit/parsing.md` ("`durationArgs.ts`") for the spec.

use crate::error::ParseError;
use crate::input::flag_registry::{FlagAction, FlagSpec};

/// Rejection text for the retired `--dur` flag. Names both surviving
/// spellings and the `-t` anchoring that the flag got wrong, so the
/// message teaches the replacement instead of just refusing.
pub const DUR_FLAG_REMOVED: &str = "`--dur` was removed. Write the duration as a trailing positional (`datum end work 12m`) or as `dur=12m`. On end/stop those anchor the block AT `-t` and run forward, where `--dur` ran backward from it.";

/// Canonical flag set for the duration concern.
///
/// `-m`/`--moment` is the "no duration" override. The duration itself is
/// spelled as a trailing positional or `dur=<D>` — never as a flag; see
/// [`DUR_FLAG_REMOVED`] for why `--dur` is a tombstone rather than absent.
pub static FLAG_SPECS: &[FlagSpec<DurationArgs>] = &[
    // `--dur` was REMOVED on 2026-09-02 (da-azqq). Do not restore it.
    //
    // Two spellings for the same duration already existed — the trailing
    // positional (`datum end work 12`) and the reserved key (`dur=12`) —
    // and `--dur` did not merely duplicate them: on `stop`/`end` it
    // anchored the block on the OPPOSITE side of `-t`, so `end work
    // --dur 12m -t 14:00` wrote 13:48 where `end work 12 -t 14:00`
    // writes 14:00. Two spellings of "a 12 minute break" landed twelve
    // minutes apart.
    //
    // The spec stays here as a tombstone so the flag fails loudly with
    // the two surviving forms named, instead of falling through to a
    // generic "unknown flag". `crates/datum-cli/tests/dur_flag_removed.rs`
    // is the test that keeps it gone.
    FlagSpec {
        short: None,
        long: Some("dur"),
        action: FlagAction::Value(|_a, _v| Err(ParseError::InvalidArg(DUR_FLAG_REMOVED.into()))),
    },
    FlagSpec {
        short: Some('m'),
        long: Some("moment"),
        action: FlagAction::Switch(|a| {
            a.moment = true;
            Ok(())
        }),
    },
];

/// Parsed shape of the duration flag pair.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DurationArgs {
    /// Positional `duration` argument as typed (e.g. `"5m"`, `"1:30"`,
    /// `"."`, or the empty string).
    pub duration: Option<String>,
    /// `-m` / `--moment`: explicit "no duration" — forces the result to
    /// [`DurationIntent::Moment`] even when a positional is also set.
    pub moment: bool,
}

/// Collapsed semantic of `DurationArgs` once defaults are applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurationIntent {
    /// No duration supplied and no `-m` — leave the field unset so the
    /// caller can apply its own default (e.g. `occur` uses a point-shape
    /// entry).
    Absent,
    /// `-m`/`--moment` — explicit "duration is null". Matches datum's
    /// `dur = null` wire form.
    Moment,
    /// The user supplied a non-placeholder positional. The string is
    /// passed through verbatim so the caller can parse it via
    /// [`crate::time::parse_duration`] at the point where a timezone
    /// context is available.
    Value(String),
}

impl DurationArgs {
    /// Resolve the flag pair into a single intent. `-m`/`--moment` wins
    /// over any positional; `"."` / `""` are treated as "no duration
    /// supplied" per datum's help text.
    pub fn build(&self) -> DurationIntent {
        if self.moment {
            return DurationIntent::Moment;
        }
        match self.duration.as_deref() {
            None | Some("") | Some(".") => DurationIntent::Absent,
            Some(s) => DurationIntent::Value(s.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// da-azqq companion to `datum-cli/tests/dur_flag_removed.rs`: the
    /// `--dur` spec must stay a *rejection*, not quietly become a setter
    /// again. A CLI test alone would still pass if someone re-registered
    /// the flag under a different concern's registry, so guard the spec
    /// itself here.
    #[test]
    fn dur_spec_is_a_rejection_not_a_setter() {
        let dur = FLAG_SPECS
            .iter()
            .find(|s| s.long == Some("dur"))
            .expect("the `--dur` tombstone must stay, so the failure names the replacements");
        let FlagAction::Value(f) = dur.action else {
            panic!("`--dur` must stay a value-consuming rejection, not a switch");
        };
        let mut args = DurationArgs::default();
        let err = f(&mut args, "12m").expect_err("`--dur` must reject its value");
        assert!(
            err.to_string().contains("`--dur` was removed"),
            "got: {err}"
        );
        assert_eq!(
            args.duration, None,
            "a rejected `--dur` must not have written a duration"
        );
    }

    #[test]
    fn empty_duration_args_is_absent() {
        assert_eq!(DurationArgs::default().build(), DurationIntent::Absent);
    }

    #[test]
    fn moment_flag_yields_moment_even_with_positional() {
        // Matches datum's `-m` override: "Removes positional duration
        // argument. Sets dur to be null."
        let a = DurationArgs {
            duration: Some("5".into()),
            moment: true,
        };
        assert_eq!(a.build(), DurationIntent::Moment);
    }

    #[test]
    fn dot_positional_is_absent() {
        let a = DurationArgs {
            duration: Some(".".into()),
            moment: false,
        };
        assert_eq!(a.build(), DurationIntent::Absent);
    }

    #[test]
    fn empty_positional_is_absent() {
        let a = DurationArgs {
            duration: Some(String::new()),
            moment: false,
        };
        assert_eq!(a.build(), DurationIntent::Absent);
    }

    #[test]
    fn concrete_positional_passes_through_unparsed() {
        let a = DurationArgs {
            duration: Some("1:30".into()),
            moment: false,
        };
        assert_eq!(a.build(), DurationIntent::Value("1:30".to_string()));
    }

    #[test]
    fn iso_duration_passes_through_unparsed() {
        let a = DurationArgs {
            duration: Some("PT5M".into()),
            moment: false,
        };
        assert_eq!(a.build(), DurationIntent::Value("PT5M".to_string()));
    }
}
