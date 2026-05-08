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
    /// Mirrors datum's `MissingRequiredKeyError(path)`.
    #[error("Missing required key: {0}")]
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
