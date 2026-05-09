//! Parse `--dur` durations, plus the ISO-8601 duration helper and the
//! `split_unit` lexer used by both duration and uncertainty parsing.

use chrono::Duration;

use crate::error::{ParseError, Result};
use crate::time::precision::PrecisionUnit;
use crate::time::uncertainty::Uncertainty;

/// Parse a duration string into a `chrono::Duration`.
/// Supports:
///   "5"        → 5 minutes (default unit)
///   "5m"/"5min" → 5 minutes
///   "2h"       → 2 hours
///   "1d"       → 1 day
///   "1:30"     → 1 hour 30 min
///   "1:15:30"  → 1h 15m 30s
///   "PT5M"     → ISO 8601
pub fn parse_duration(s: &str) -> Result<Duration> {
    // ISO 8601 duration
    if s.starts_with("PT") || s.starts_with("P") {
        return parse_iso_duration(s);
    }

    // Colon-separated: MM, HH:MM, or HH:MM:SS or DD:HH:MM:SS
    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        return match parts.len() {
            2 => {
                let h: i64 = parts[0]
                    .parse()
                    .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
                let m: i64 = parts[1]
                    .parse()
                    .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
                Ok(Duration::minutes(h * 60 + m))
            }
            3 => {
                let h: i64 = parts[0]
                    .parse()
                    .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
                let m: i64 = parts[1]
                    .parse()
                    .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
                let sec: i64 = parts[2]
                    .parse()
                    .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
                Ok(Duration::seconds(h * 3600 + m * 60 + sec))
            }
            4 => {
                let d: i64 = parts[0]
                    .parse()
                    .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
                let h: i64 = parts[1]
                    .parse()
                    .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
                let m: i64 = parts[2]
                    .parse()
                    .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
                let sec: i64 = parts[3]
                    .parse()
                    .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
                Ok(Duration::seconds(d * 86400 + h * 3600 + m * 60 + sec))
            }
            _ => Err(ParseError::InvalidTime(s.to_string())),
        };
    }

    // Named unit suffixes
    let (num_str, unit) = split_unit(s);
    let n: f64 = num_str
        .parse()
        .map_err(|_| ParseError::InvalidTime(s.to_string()))?;
    let secs = match unit {
        "s" | "sec" | "second" | "seconds" => (n) as i64,
        "m" | "min" | "minute" | "minutes" => (n * 60.0) as i64,
        "h" | "hr" | "hour" | "hours" => (n * 3600.0) as i64,
        "d" | "day" | "days" => (n * 86400.0) as i64,
        "" => (n * 60.0) as i64, // default: minutes
        _ => {
            return Err(ParseError::InvalidTime(format!(
                "Unknown duration unit: '{unit}'"
            )))
        }
    };
    Ok(Duration::seconds(secs))
}

/// Precision produced by `n` stacked `-q` flags. The offset scaling is
/// linear (5 min per flag), but precision is capped at 150s (±2.5m) for the
/// first four and steps up to 300s (±5m) once the user stacks 5+ `-q`s —
/// at that point the stacked offset is coarse enough that 5-min-resolution
/// is more honest than claiming 2.5-min precision.
pub fn quick_precision_for_count(n: u32) -> Uncertainty {
    if n == 0 {
        return Uncertainty::exact();
    }
    let seconds = if n >= 5 { 300 } else { 150 };
    Uncertainty {
        seconds,
        unit: PrecisionUnit::Minute,
    }
}

pub(crate) fn split_unit(s: &str) -> (&str, &str) {
    let idx = s
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphabetic())
        .last()
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    (&s[..idx], &s[idx..])
}

pub(crate) fn parse_iso_duration(s: &str) -> Result<Duration> {
    // Minimal ISO 8601 duration parser: PT5M, PT1H30M, P1DT5H, etc.
    let s = if s.starts_with('P') { &s[1..] } else { s };
    let mut secs: i64 = 0;
    let (date_part, time_part) = if let Some(idx) = s.find('T') {
        (&s[..idx], &s[idx + 1..])
    } else {
        (s, "")
    };

    for (part, multiplier) in [
        (
            date_part,
            vec![
                ('Y', 365 * 86400),
                ('M', 30 * 86400),
                ('W', 7 * 86400),
                ('D', 86400),
            ],
        ),
        (time_part, vec![('H', 3600), ('M', 60), ('S', 1)]),
    ] {
        let mut cur = part;
        for (ch, mult) in multiplier {
            if let Some(idx) = cur.find(ch) {
                let n: f64 = cur[..idx].parse().unwrap_or(0.0);
                secs += (n * mult as f64) as i64;
                cur = &cur[idx + 1..];
            }
        }
    }
    Ok(Duration::seconds(secs))
}
