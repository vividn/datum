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
#[derive(Default)]
pub enum PrecisionUnit {
    #[default]
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

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[PrecisionUnit] = &[
        PrecisionUnit::Exact,
        PrecisionUnit::Second,
        PrecisionUnit::Minute,
        PrecisionUnit::Hour,
        PrecisionUnit::Date,
        PrecisionUnit::Month,
        PrecisionUnit::Year,
    ];

    #[test]
    fn display_and_fromstr_round_trip() {
        for v in ALL {
            let s = v.to_string();
            let parsed: PrecisionUnit = s.parse().unwrap();
            assert_eq!(*v, parsed, "round-trip failed for {v}");
        }
    }

    #[test]
    fn display_matches_serde_json_string() {
        for v in ALL {
            let displayed = v.to_string();
            let json = serde_json::to_value(v).unwrap();
            assert_eq!(
                serde_json::Value::String(displayed.clone()),
                json,
                "Display vs serde drift for {v}",
            );
        }
    }

    #[test]
    fn serde_round_trips_through_json() {
        for v in ALL {
            let json = serde_json::to_value(v).unwrap();
            let back: PrecisionUnit = serde_json::from_value(json).unwrap();
            assert_eq!(*v, back, "serde round-trip failed for {v}");
        }
    }

    #[test]
    fn fromstr_is_case_insensitive() {
        assert_eq!(
            "HOUR".parse::<PrecisionUnit>().unwrap(),
            PrecisionUnit::Hour
        );
        assert_eq!(
            "Hour".parse::<PrecisionUnit>().unwrap(),
            PrecisionUnit::Hour
        );
    }

    #[test]
    fn fromstr_rejects_unknown() {
        assert!("fortnight".parse::<PrecisionUnit>().is_err());
    }
}
