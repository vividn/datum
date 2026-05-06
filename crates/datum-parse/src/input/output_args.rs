//! `outputArgs` — `--show-all/-A`, `--show`, `--format-string/-o`. Port of
//! datum's `src/input/outputArgs.ts`.
//!
//! Step 2b/N lifted the `Show` enum + resolve algorithm out of
//! `datum-commands` so that the bespoke parser (step 3/N) owns the
//! resolution. `datum-commands` keeps a thin bridge from its render-time
//! `OutputArgs` to the parse-crate one; this module is the pure,
//! WASM-safe analogue.
//!
//! See `docs/parity-audit/parsing.md` ("`outputArgs.ts`") for the spec.

use std::str::FromStr;

use crate::input::flag_registry::{FlagAction, FlagSpec};

/// Canonical flag set for the output concern.
pub static FLAG_SPECS: &[FlagSpec<OutputArgs>] = &[
    FlagSpec {
        short: Some('A'),
        long: Some("show-all"),
        action: FlagAction::Switch(|a| {
            a.show_all = true;
            Ok(())
        }),
    },
    FlagSpec {
        short: None,
        long: Some("show"),
        action: FlagAction::Value(|a, v| {
            a.show = Some(v.to_string());
            Ok(())
        }),
    },
    FlagSpec {
        short: Some('o'),
        long: Some("format-string"),
        action: FlagAction::Value(|a, v| {
            a.format_string = Some(v.to_string());
            Ok(())
        }),
    },
];

/// Output verbosity — mirrors datum's `Show` enum. Default is `Default`
/// so constructing [`OutputArgs::default`] matches an unsupplied `--show`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Show {
    /// Header + info + non-redundant extra_data. Matches datum's
    /// implicit default when nothing is supplied.
    #[default]
    Default,
    /// Suppress all output.
    None,
    /// Header line only.
    Minimal,
    /// Default plus always-include extra_data, even with a format string.
    Standard,
    /// Render `format_string` only.
    Format,
    /// Default + full pretty-printed doc.
    All,
}

impl FromStr for Show {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "default" => Ok(Show::Default),
            "none" => Ok(Show::None),
            "minimal" => Ok(Show::Minimal),
            "standard" => Ok(Show::Standard),
            "format" => Ok(Show::Format),
            "all" => Ok(Show::All),
            other => Err(format!("unknown --show value: {other}")),
        }
    }
}

/// Parsed shape of `--show` / `-A` / `-o`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputArgs {
    /// `-A` / `--show-all`: shorthand for `--show all`, wins over `show`.
    pub show_all: bool,
    /// `--show <mode>` — exact string the user supplied. `None` means the
    /// flag wasn't passed; the empty string is never produced by argparse.
    pub show: Option<String>,
    /// `-o` / `--format-string`: `%field%`-templated output. When set and
    /// nothing else is explicit, `Show::Format` is implied.
    pub format_string: Option<String>,
}

/// Resolved output configuration — the semantic form consumed by the
/// render layer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputResolved {
    pub show: Show,
    pub format_string: Option<String>,
}

impl OutputArgs {
    /// Resolve the flag triple. Rules (in precedence order):
    ///
    /// 1. `-A` / `--show-all` → `Show::All` regardless of `--show`.
    /// 2. `--show <mode>` parses with [`Show::from_str`] (default on
    ///    parse-failure, matching the pre-R2 code).
    /// 3. If `-o` is supplied with no other explicit override, `Show::Format`.
    pub fn resolve(&self) -> OutputResolved {
        let parsed = self
            .show
            .as_deref()
            .map(|s| Show::from_str(s).unwrap_or_default());

        let base = if self.show_all {
            Show::All
        } else {
            parsed.unwrap_or_default()
        };

        // `-o` implies `Show::Format` only when the user didn't explicitly
        // pick something else. `--show default` (or omitted) counts as
        // "not explicit" per the pre-R2 behaviour.
        let show = if self.format_string.is_some()
            && !self.show_all
            && matches!(self.show.as_deref(), None | Some("default"))
        {
            Show::Format
        } else {
            base
        };

        OutputResolved {
            show,
            format_string: self.format_string.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_resolve_to_default_show_without_format() {
        let r = OutputArgs::default().resolve();
        assert_eq!(r.show, Show::Default);
        assert_eq!(r.format_string, None);
    }

    #[test]
    fn show_all_overrides_named_show() {
        let a = OutputArgs {
            show_all: true,
            show: Some("minimal".into()),
            format_string: None,
        };
        assert_eq!(a.resolve().show, Show::All);
    }

    #[test]
    fn show_all_overrides_format_string() {
        let a = OutputArgs {
            show_all: true,
            show: None,
            format_string: Some("%slug%".into()),
        };
        assert_eq!(a.resolve().show, Show::All);
    }

    #[test]
    fn named_show_wins_when_explicit() {
        let a = OutputArgs {
            show_all: false,
            show: Some("minimal".into()),
            format_string: None,
        };
        assert_eq!(a.resolve().show, Show::Minimal);
    }

    #[test]
    fn format_string_alone_implies_format_show() {
        let a = OutputArgs {
            show_all: false,
            show: None,
            format_string: Some("%slug% %field%".into()),
        };
        let r = a.resolve();
        assert_eq!(r.show, Show::Format);
        assert_eq!(r.format_string.as_deref(), Some("%slug% %field%"));
    }

    #[test]
    fn format_string_does_not_override_explicit_non_default_show() {
        let a = OutputArgs {
            show_all: false,
            show: Some("all".into()),
            format_string: Some("%slug%".into()),
        };
        assert_eq!(a.resolve().show, Show::All);
    }

    #[test]
    fn format_string_applies_even_when_show_is_default_literal() {
        // Matches the pre-R2 behaviour: an explicit `--show default` with
        // `-o foo` still counts as "not explicit" and flips to Format.
        let a = OutputArgs {
            show_all: false,
            show: Some("default".into()),
            format_string: Some("%slug%".into()),
        };
        assert_eq!(a.resolve().show, Show::Format);
    }

    #[test]
    fn unknown_show_value_falls_back_to_default() {
        // `.parse().unwrap_or_default()` in the resolve path keeps the
        // pre-R2 behaviour of silently degrading garbage.
        let a = OutputArgs {
            show_all: false,
            show: Some("not-a-show-mode".into()),
            format_string: None,
        };
        assert_eq!(a.resolve().show, Show::Default);
    }

    #[test]
    fn show_from_str_accepts_mixed_case() {
        assert_eq!("ALL".parse::<Show>().unwrap(), Show::All);
        assert_eq!("Minimal".parse::<Show>().unwrap(), Show::Minimal);
    }

    #[test]
    fn show_from_str_rejects_unknown_value() {
        assert!("banana".parse::<Show>().is_err());
    }
}
