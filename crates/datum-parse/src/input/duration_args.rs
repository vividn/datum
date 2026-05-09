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

use crate::input::flag_registry::{FlagAction, FlagSpec};

/// Canonical flag set for the duration concern.
///
/// `--dur` carries the positional duration value (datum's TS shape uses a
/// proper positional, but the bespoke parser exposes it as `--dur` so it
/// composes with the rest of the flag stream); `-m`/`--moment` is the
/// "no duration" override.
pub static FLAG_SPECS: &[FlagSpec<DurationArgs>] = &[
    FlagSpec {
        short: None,
        long: Some("dur"),
        action: FlagAction::Value(|a, v| {
            a.duration = Some(v.to_string());
            Ok(())
        }),
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
