use thiserror::Error;

/// One hit in an ambiguous prefix match — both the stored id and slug are
/// surfaced so callers can show users a disambiguating list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixCandidate {
    pub id: String,
    pub slug: String,
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

    #[error("'{prefix}' is ambiguous and may refer to:{}", format_candidates(candidates))]
    AmbiguousPrefix {
        prefix: String,
        candidates: Vec<PrefixCandidate>,
    },

    #[error("{0}")]
    Other(String),
}

fn format_candidates(candidates: &[PrefixCandidate]) -> String {
    let mut s = String::new();
    for c in candidates {
        s.push_str("\n  ");
        s.push_str(&c.id);
        s.push('\t');
        s.push_str(&c.slug);
    }
    s
}

pub type Result<T> = std::result::Result<T, DatumError>;
