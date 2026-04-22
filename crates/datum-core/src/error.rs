use thiserror::Error;

/// One hit in an ambiguous prefix match — both the slug and hid are surfaced
/// so callers can show users a disambiguating list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixCandidate {
    pub slug: String,
    pub hid: String,
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

    #[error("Incompatible database schema: {0}")]
    IncompatibleSchema(String),

    #[error("Cryptographic error: {0}")]
    Crypto(String),

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
        }
    }
}
