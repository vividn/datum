//! `PrecisionUnit` — granularity label for a time's uncertainty.
//!
//! Previously lived in `datum-core::entry`; moved here as part of R2 so the
//! pure time types all live in `datum-parse`. `datum-core::entry` re-exports
//! it so `entry::PrecisionUnit` remains a valid path.
//!
//! The serde representation (`rename_all = "lowercase"`) and case-insensitive
//! `FromStr` are preserved verbatim from the original location — rows stored
//! before this move must continue to deserialize.

use serde::{Deserialize, Serialize};

/// Display granularity of a time value's uncertainty. An input of `±5m` is
/// stored as `seconds = 300` + `unit = Minute` so round-tripping preserves
/// the user-visible unit rather than collapsing to seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrecisionUnit {
    Exact,
    Second,
    Minute,
    Hour,
    Date,
    Month,
    Year,
}

impl std::fmt::Display for PrecisionUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrecisionUnit::Exact => write!(f, "exact"),
            PrecisionUnit::Second => write!(f, "second"),
            PrecisionUnit::Minute => write!(f, "minute"),
            PrecisionUnit::Hour => write!(f, "hour"),
            PrecisionUnit::Date => write!(f, "date"),
            PrecisionUnit::Month => write!(f, "month"),
            PrecisionUnit::Year => write!(f, "year"),
        }
    }
}

impl std::str::FromStr for PrecisionUnit {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "exact" => Ok(PrecisionUnit::Exact),
            "second" => Ok(PrecisionUnit::Second),
            "minute" => Ok(PrecisionUnit::Minute),
            "hour" => Ok(PrecisionUnit::Hour),
            "date" => Ok(PrecisionUnit::Date),
            "month" => Ok(PrecisionUnit::Month),
            "year" => Ok(PrecisionUnit::Year),
            _ => Err(format!("Unknown precision_unit: '{s}'")),
        }
    }
}

impl Default for PrecisionUnit {
    fn default() -> Self {
        PrecisionUnit::Exact
    }
}
