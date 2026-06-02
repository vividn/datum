use thiserror::Error;

use crate::ids::{Hid, Slug};

/// One hit in an ambiguous prefix match — both the slug and hid are surfaced
/// so callers can show users a disambiguating list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixCandidate {
    pub slug: Slug,
    pub hid: Hid,
}

#[derive(Error, Debug)]
pub enum DatumError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Invalid time: {0}")]
    InvalidTime(String),

    #[error("Invalid field spec: {0}")]
    InvalidFieldSpec(String),

    #[error("Duplicate entry: {0}")]
    Duplicate(String),

    #[error("Entry not found: {0}")]
    NotFound(String),

    #[error("Entry is tombstoned: {0}")]
    Tombstoned(String),

    #[error("Missing required field: {0}")]
    MissingRequired(String),

    #[error(
        "'{prefix}' is ambiguous and may refer to:{}",
        format_candidates(candidates)
    )]
    AmbiguousPrefix {
        prefix: String,
        candidates: Vec<PrefixCandidate>,
    },

    #[error("Incompatible database schema: {0}")]
    IncompatibleSchema(String),

    /// A migration's `down` step refused to silently destroy data. Surfaced
    /// when rolling a database back across a step that the forward
    /// migration documented as data-destructive (e.g. v2→v3 entries
    /// rewrite). Callers must back up first or accept the loss explicitly.
    #[error("Migration is not reversible: {0}")]
    NotReversible(String),

    #[error("Cryptographic error: {0}")]
    Crypto(String),

    /// `_LAST_WITH_PROTECTION` resolved against a stale pointer. The
    /// caller can re-issue with `_LAST` to bypass the time guard, or
    /// touch a document first to refresh the pointer.
    #[error(
        "Last docs were last touched more than {minutes} minutes ago. \
         Use '_LAST' to bypass the protection window, or touch a document first."
    )]
    LastDocsTooOld { minutes: i64 },

    /// `_LAST` / `_LAST_WITH_PROTECTION` was used but no command has
    /// stamped the pointer yet.
    #[error("No previous documents to refer to — touch a document first")]
    NoLastDocsRef,

    #[error("{0}")]
    Other(String),
}

fn format_candidates(candidates: &[PrefixCandidate]) -> String {
    let mut s = String::new();
    for c in candidates {
        s.push_str("\n  ");
        s.push_str(&c.hid);
        s.push('\t');
        s.push_str(&c.slug);
    }
    s
}

pub type Result<T> = std::result::Result<T, DatumError>;

impl From<datum_parse::ParseError> for DatumError {
    fn from(e: datum_parse::ParseError) -> Self {
        match e {
            datum_parse::ParseError::InvalidTime(s) => DatumError::InvalidTime(s),
            datum_parse::ParseError::InvalidArg(s) => DatumError::Other(s),
            datum_parse::ParseError::MissingRequiredKey(path) => {
                DatumError::Other(datum_parse::ParseError::MissingRequiredKey(path).to_string())
            }
            datum_parse::ParseError::ExtraData(rem) => {
                DatumError::Other(format!("Extra data with no matching key: {rem:?}"))
            }
            datum_parse::ParseError::AliasCycle(path) => {
                DatumError::Other(format!("Alias cycle: {}", path.join(" → ")))
            }
            datum_parse::ParseError::HelpRequested(Some(cmd)) => {
                DatumError::Other(format!("help requested for '{cmd}'"))
            }
            datum_parse::ParseError::HelpRequested(None) => {
                DatumError::Other("help requested".to_string())
            }
            datum_parse::ParseError::VersionRequested => {
                DatumError::Other("version requested".to_string())
            }
            datum_parse::ParseError::ManRequested(Some(cmd)) => {
                DatumError::Other(format!("man requested for '{cmd}'"))
            }
            datum_parse::ParseError::ManRequested(None) => {
                DatumError::Other("man requested".to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DatumError;

    #[test]
    fn parse_missing_required_field_preserves_hint_in_conversion() {
        let err = DatumError::from(datum_parse::ParseError::MissingRequiredKey("field".into()));

        assert_eq!(
            err.to_string(),
            "Missing required key: field — bare `<FIELD>` positional required first; `KEY=VALUE` tokens bind to data keys, not the field slot. Try: `datum <verb> <field> KEY=VALUE`."
        );
    }
}
