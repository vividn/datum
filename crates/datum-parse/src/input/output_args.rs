//! `outputArgs` — the output-flag family. Port of datum's
//! `src/input/outputArgs.ts`.
//!
//! `-A/--show-all` and `-o/--format-string` are per-command flags, lifted
//! onto whichever intent struct carries an [`OutputArgs`]. `--show` and
//! its `--json` shorthand are NOT: da-qt6z made them one family read by
//! the shared layer for EVERY command, next to `--db`, so [`take_show`]
//! below is the only place either spelling is recognised. `spec`'s old
//! `--json <JSON>` input flag was renamed `--from-json` to clear the way.
//!
//! Step 2b/N lifted the `Show` enum + resolve algorithm out of
//! `datum-commands` so that the bespoke parser (step 3/N) owns the
//! resolution. `datum-commands` keeps a thin bridge from its render-time
//! `OutputArgs` to the parse-crate one; this module is the pure,
//! WASM-safe analogue.
//!
//! See `docs/parity-audit/parsing.md` ("`outputArgs.ts`") for the spec.

use std::str::FromStr;

use crate::error::{ParseError, Result as ParseResult};
use crate::input::flag_registry::{FlagAction, FlagSpec};

/// Canonical flag set for the per-command half of the output concern.
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
    /// The action echo as one JSON document on stdout, nothing else
    /// (da-yc9w). Carries exactly the strings the default show prints,
    /// so a caller places them instead of parsing them.
    Json,
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
            "json" => Ok(Show::Json),
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
    /// `-o` / `--format-string`: `%field%`-templated output. Appended to
    /// the stock render; it does *not* imply `Show::Format` (only an
    /// explicit `--show format` does).
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
    ///
    /// `-o` does **not** influence the `Show` level (da-jose). Supplying a
    /// format string at the default show keeps the header + info lines and
    /// appends the format string in place of the data block; only an
    /// explicit `--show format` collapses to format-string-only output.
    /// This mirrors datumv2's `sanitizeOutputArgs`, where the CLI always
    /// sets `show = Default` and the `formatString → Format` fallback only
    /// fires for an unset show.
    pub fn resolve(&self) -> OutputResolved {
        let parsed = self
            .show
            .as_deref()
            .map(|s| Show::from_str(s).unwrap_or_default());

        let show = if self.show_all {
            Show::All
        } else {
            parsed.unwrap_or_default()
        };

        OutputResolved {
            show,
            format_string: self.format_string.clone(),
        }
    }
}

// ─── The shared half: `--show` / `--json`, read for every command ────────

/// Every `--show` value, in the order `--help` lists them.
pub const ALL_SHOWS: &[Show] = &[
    Show::Default,
    Show::None,
    Show::Minimal,
    Show::Standard,
    Show::Format,
    Show::All,
    Show::Json,
];

impl Show {
    /// The spelling `--show` accepts, and `--help` prints.
    pub fn name(self) -> &'static str {
        match self {
            Show::Default => "default",
            Show::None => "none",
            Show::Minimal => "minimal",
            Show::Standard => "standard",
            Show::Format => "format",
            Show::All => "all",
            Show::Json => "json",
        }
    }
}

/// What a command can actually draw — the `--show` values its own code
/// branches on.
///
/// The family is parsed for every command now (da-qt6z), so this table is
/// what keeps that from meaning "every command silently ignores six of the
/// seven values". A value missing here is an error naming the command, not
/// a flag that quietly does nothing; it is also what `--help` prints, so
/// the two can't drift.
pub fn shows_for(command: &str) -> &'static [Show] {
    match command {
        // The document renderers (`render::show_action_with_context`)
        // draw every level.
        "occur" | "start" | "stop" | "end" | "switch" | "add" | "note" | "get" | "delete"
        | "del" | "undo" | "redo" | "update" | "merge" | "rekey" | "edit" => ALL_SHOWS,
        // The table renderer honours silence and the format string.
        "grep" => &[Show::Default, Show::None, Show::Format],
        // …and `tail`/`head` have their own JSON rows on top of it.
        "tail" | "head" => &[Show::Default, Show::None, Show::Format, Show::Json],
        // Readers carrying a JSON document beside their stock output.
        // `alias`, `config` and `db` are groups: only `alias list`,
        // `config show` and `db restore` render the document, and the
        // sibling subcommands ignore the flag.
        "now" | "nowview" | "dayview" | "count" | "ids" | "history" | "check" | "alias"
        | "config" | "db" => &[Show::Default, Show::None, Show::Json],
        // `fields` draws one more: `--show all` is the engineering
        // spelling of its `--include-ignored` (da-1lnf). `stats --show
        // all` is its text and then its JSON line, from one run (da-cddng).
        "fields" | "stats" => &[Show::Default, Show::None, Show::Json, Show::All],
        // `spec show` prints its document by default, so `--show json` is
        // the explicit form of that; it has no silent mode to offer.
        "spec" => &[Show::Default, Show::Json],
        // `help --json` is the command table itself (da-r13j).
        "help" => &[Show::Default, Show::Json],
        // A group: `sync status`, `sync chain list`, `sync admin list`,
        // `sync chain purge` and `sync pipe apply` render the document, and
        // the parser refuses a typed `json` on the rest (da-8un1i).
        "sync" => &[Show::Default, Show::Json],
        // `conflicts list` prints each entry's column as stored.
        "conflicts" => &[Show::Default, Show::Json],
        // Everything else prints one thing, one way.
        _ => &[Show::Default],
    }
}

/// Pull the output family out of one command's tokens, returning the
/// resolved `Show` (when either flag appeared) and the survivors.
///
/// * `--show <V>`, `--show=<V>` — the named value.
/// * `--json` — exactly `--show json`, and it takes no value.
/// * The LAST one wins, for every pair: `--show none --show json` prints
///   the document, `--show json --show none` prints nothing. Silence used
///   to latch (da-yc9w); vividn asked for last-wins, so it no longer does.
///
/// Whether the named command can DRAW the value is [`check_show`], which
/// the caller runs once the subcommand has been recognised: a typo'd verb
/// deserves "unknown subcommand", not a lecture about its renderings.
pub fn take_show<'a>(tokens: &[&'a str]) -> ParseResult<(Option<Show>, Vec<&'a str>)> {
    let mut show: Option<Show> = None;
    let mut rest: Vec<&'a str> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        let value = match tok.strip_prefix("--show") {
            Some("") => {
                let v = tokens
                    .get(i + 1)
                    .ok_or_else(|| ParseError::InvalidArg("--show requires a value".into()))?;
                i += 1;
                Some((*v).to_string())
            }
            Some(eq) if eq.starts_with('=') => Some(eq[1..].to_string()),
            _ if tok == "--json" => Some("json".to_string()),
            _ if tok.starts_with("--json=") => {
                return Err(ParseError::InvalidArg(
                    "--json takes no value (it is `--show json`)".into(),
                ))
            }
            _ => None,
        };
        match value {
            Some(v) => show = Some(v.parse::<Show>().map_err(ParseError::InvalidArg)?),
            None => rest.push(tok),
        }
        i += 1;
    }

    Ok((show, rest))
}

/// Refuse a `--show` value the named command has no rendering for.
///
/// Loud beats silently ignored: the flag is read for every command now, so
/// without this `fields --show minimal` would parse and then draw the
/// default, which is the failure mode the family was meant to remove.
pub fn check_show(show: Show, command: &str) -> ParseResult<()> {
    check_show_in(show, command, shows_for(command))
}

/// [`check_show`] against the renderings the caller knows `command` to
/// have: a group's subcommand, which [`shows_for`] does not list.
pub fn check_show_in(show: Show, command: &str, allowed: &[Show]) -> ParseResult<()> {
    if allowed.contains(&show) {
        return Ok(());
    }
    Err(ParseError::InvalidArg(format!(
        "{command}: --show {} is not a rendering {command} has (it draws: {})",
        show.name(),
        allowed
            .iter()
            .map(|v| v.name())
            .collect::<Vec<_>>()
            .join("|"),
    )))
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

    /// da-qt6z: the LAST `--show` wins, for every pair of values.
    /// `none` used to latch over everything after it (da-yc9w); that came
    /// from vremya's wording of the spec, not from what vividn wanted, and
    /// silence is now as overridable as every other value.
    #[test]
    fn the_last_show_wins_for_every_pair() {
        assert_eq!("json".parse::<Show>().unwrap(), Show::Json);
        assert_eq!("JSON".parse::<Show>().unwrap(), Show::Json);

        let table = [
            (vec!["--show", "none", "--show", "json"], Show::Json),
            (vec!["--show", "json", "--show", "none"], Show::None),
            (vec!["--show", "json", "--show", "json"], Show::Json),
            (vec!["--show", "minimal", "--show", "all"], Show::All),
            (vec!["--show", "json", "--json"], Show::Json),
            (vec!["--json"], Show::Json),
            (vec!["--json", "--show", "none"], Show::None),
            (vec!["--show=none", "--show=json"], Show::Json),
        ];
        for (argv, want) in table {
            let (got, rest) = take_show(&argv).unwrap();
            assert_eq!(got, Some(want), "{argv:?}");
            assert!(rest.is_empty(), "{argv:?} left {rest:?} behind");
        }

        // Nothing asked for → nothing resolved, and every token survives.
        let (got, rest) = take_show(&["coffee", "-t", "9:15"]).unwrap();
        assert_eq!(got, None);
        assert_eq!(rest, vec!["coffee", "-t", "9:15"]);
    }

    /// A value the named command has no rendering for is loud, and the
    /// message names both (da-qt6z).
    #[test]
    fn a_show_the_command_cannot_draw_is_an_error() {
        let err = check_show(Show::Minimal, "fields").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("fields: --show minimal is not a rendering fields has"),
            "{msg}"
        );
        assert!(check_show(Show::Json, "fields").is_ok());
        assert!(check_show(Show::Json, "track").is_err());
        assert!(check_show(Show::Default, "track").is_ok());
        // An unknown value is loud too, rather than degrading to default.
        assert!(take_show(&["--show", "banana"]).is_err());
    }

    /// `--json` carries no value of its own, so a value behind it is left
    /// standing as an argument — which is how `spec set x --json '{…}'`
    /// reaches the parser that points at `--from-json`.
    #[test]
    fn json_takes_no_value_and_leaves_one_standing() {
        let (got, rest) = take_show(&["set", "x", "--json", "{}"]).unwrap();
        assert_eq!(got, Some(Show::Json));
        assert_eq!(rest, vec!["set", "x", "{}"]);
        assert!(take_show(&["--json=x"]).is_err());
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
    fn format_string_alone_keeps_default_show() {
        // da-jose: `-o` no longer collapses the default show to Format —
        // the header + info lines stay and the format string is appended.
        // Only an explicit `--show format` gives format-string-only output.
        let a = OutputArgs {
            show_all: false,
            show: None,
            format_string: Some("%slug% %field%".into()),
        };
        let r = a.resolve();
        assert_eq!(r.show, Show::Default);
        assert_eq!(r.format_string.as_deref(), Some("%slug% %field%"));
    }

    #[test]
    fn explicit_show_format_gives_format_show() {
        let a = OutputArgs {
            show_all: false,
            show: Some("format".into()),
            format_string: Some("%slug%".into()),
        };
        assert_eq!(a.resolve().show, Show::Format);
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
    fn format_string_keeps_show_when_default_literal() {
        // da-jose: an explicit `--show default` with `-o foo` stays Default
        // (header + info + appended format string), not Format.
        let a = OutputArgs {
            show_all: false,
            show: Some("default".into()),
            format_string: Some("%slug%".into()),
        };
        assert_eq!(a.resolve().show, Show::Default);
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
