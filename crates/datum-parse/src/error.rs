//! Parse-layer error type.
//!
//! `datum-core` (and downstream crates) convert this into `DatumError`
//! transparently via `From`. The variants are intentionally narrow — this
//! crate only produces errors from string-parsing failures.

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("Invalid time: {0}")]
    InvalidTime(String),

    #[error("Invalid argument: {0}")]
    InvalidArg(String),

    /// `handleDataArgs` could not satisfy a `-k key` with no default.
    /// Mirrors datum's `MissingRequiredKeyError(path)`. For the very
    /// common case where the user supplied a `KEY=VALUE` token as the
    /// first positional (and that token therefore claimed an unrelated
    /// data key instead of filling the field slot), the message
    /// includes a hint pointing back at the field-name requirement so
    /// the error speaks the user's language instead of the parser's
    /// (da-nazk).
    #[error("Missing required key: {path}{hint}", path = .0, hint = missing_key_hint(.0))]
    MissingRequiredKey(String),

    /// Positional data exceeded the declared keys with no remainder bucket
    /// in scope. Mirrors datum's `ExtraDataError([...remainders])`.
    #[error("Extra data with no matching key: {0:?}")]
    ExtraData(Vec<String>),

    /// Alias expansion revisited a name in the chain (e.g. `a → b → a`).
    /// The payload is the visited path with the repeated name appended at
    /// the end, so callers can render the full cycle.
    #[error("Alias cycle: {}", .0.join(" → "))]
    AliasCycle(Vec<String>),

    /// `--help` or `-h` was requested. Optional payload: the subcommand
    /// name (if `datum help <cmd>` or `datum <cmd> --help`). Callers
    /// render the help and exit with status 0.
    #[error("help requested{}", .0.as_deref().map(|s| format!(" for '{s}'")).unwrap_or_default())]
    HelpRequested(Option<String>),

    /// `--version` or `-V` was requested. Callers render the version
    /// banner and exit with status 0.
    #[error("version requested")]
    VersionRequested,

    /// `--man [SUBCOMMAND]` was requested. Optional payload: the
    /// subcommand whose man page should be rendered. `None` means the
    /// top-level `datum(1)` page. Callers render the roff and exit 0.
    #[error("man requested{}", .0.as_deref().map(|s| format!(" for '{s}'")).unwrap_or_default())]
    ManRequested(Option<String>),
}

pub type Result<T> = std::result::Result<T, ParseError>;

/// Append a hint to `Missing required key: {path}` when the missing key
/// is the field-slot itself. `add foo=bar` / `occur foo=bar` etc. land
/// here: the parser interpreted `foo=bar` as a `KEY=VALUE` token and
/// bound it to a data key called `foo`, leaving the field-slot
/// unfilled. The bare error doesn't tell the user *that* — and the
/// usual fix ("supply a field name first") isn't obvious without it.
fn missing_key_hint(path: &str) -> &'static str {
    if path == "field" {
        " — bare `<FIELD>` positional required first; `KEY=VALUE` tokens bind to data keys, not the field slot. Try: `datum <verb> <field> KEY=VALUE`."
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_field_hint_includes_user_facing_guidance() {
        let err = ParseError::MissingRequiredKey("field".into());
        let msg = format!("{err}");
        assert!(
            msg.contains("Missing required key: field") && msg.contains("bare `<FIELD>`"),
            "got: {msg}"
        );
    }

    #[test]
    fn missing_other_keys_unchanged_by_hint() {
        // Any non-`field` key keeps the original terse message — the hint
        // is targeted at the very common "user typed key=value as their
        // first positional" trap, not a generic addition.
        let err = ParseError::MissingRequiredKey("weight".into());
        let msg = format!("{err}");
        assert_eq!(msg, "Missing required key: weight");
    }
}
