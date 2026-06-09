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
///   "10m30s"   → 10 minutes 30 seconds (compound)
///   "1h30m"    → 1 hour 30 minutes (compound)
///   "1:30"     → 1 hour 30 min
///   "1:15:30"  → 1h 15m 30s
///   "PT5M"     → ISO 8601
///
/// ## Colon-form contract (read carefully — the rightmost field shifts)
///
/// The colon form is anchored by part count, and the 2-part case is
/// deliberately read as `HH:MM`, *not* `MM:SS`:
///
/// ```text
///   2 parts → HH:MM        1:30       = 1h30m,  0:30 = 30 min
///   3 parts → HH:MM:SS     1:15:30    = 1h15m30s
///   4 parts → DD:HH:MM:SS  1:00:00:00 = 1 day
/// ```
///
/// So the rightmost field is *minutes* for the 2-part form but *seconds* for
/// the 3- and 4-part forms. This is intentional: people read a bare `1:30`
/// stopwatch-style as one-thirty (an hour and a half), which is the common
/// `--dur` case, while the longer forms want second-precision.
///
/// This is a deliberate divergence from datum-v2, whose colon form was
/// *seconds*-anchored across all part counts
/// (`[[[[years:]days:]hours:]minutes:]seconds`), so v2's `1:30` meant
/// 1 min 30 sec — 60× shorter. See `docs/parity-audit/parsing.md`
/// (`parseDurationStr.ts`) for the divergence record.
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

    parse_compound_duration(s)
}

/// Parse one-or-more `<number><unit>` segments. A single trailing bare
/// number (no unit) defaults to minutes for back-compat with `"5"`.
fn parse_compound_duration(s: &str) -> Result<Duration> {
    if s.is_empty() {
        return Err(ParseError::InvalidTime(s.to_string()));
    }
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut total_secs: i64 = 0;
    let mut segments = 0;

    while i < bytes.len() {
        let num_start = i;
        // Optional leading sign (only on the first segment).
        if segments == 0 && (bytes[i] == b'-' || bytes[i] == b'+') {
            i += 1;
        }
        let digits_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if i == digits_start {
            return Err(ParseError::InvalidTime(s.to_string()));
        }
        let num_str = &s[num_start..i];
        let n: f64 = num_str
            .parse()
            .map_err(|_| ParseError::InvalidTime(s.to_string()))?;

        let unit_start = i;
        while i < bytes.len() && (bytes[i] as char).is_alphabetic() {
            i += 1;
        }
        let unit = &s[unit_start..i];

        let secs = match unit {
            "s" | "sec" | "second" | "seconds" => n as i64,
            "m" | "min" | "minute" | "minutes" => (n * 60.0) as i64,
            "h" | "hr" | "hour" | "hours" => (n * 3600.0) as i64,
            "d" | "day" | "days" => (n * 86400.0) as i64,
            "" => {
                // Bare number is only allowed as the entire input (default: minutes).
                if segments != 0 || i != bytes.len() {
                    return Err(ParseError::InvalidTime(s.to_string()));
                }
                (n * 60.0) as i64
            }
            _ => {
                return Err(ParseError::InvalidTime(format!(
                    "Unknown duration unit: '{unit}'"
                )))
            }
        };
        total_secs += secs;
        segments += 1;
    }

    if segments == 0 {
        return Err(ParseError::InvalidTime(s.to_string()));
    }
    Ok(Duration::seconds(total_secs))
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

/// Canonical ISO 8601 duration parser: `PT5M`, `PT1H30M`, `P1DT5H`, `P3W`,
/// optional leading `-` for negative spans. Unknown unit letters,
/// unparseable numbers, and trailing residue are all errors — the parser
/// never silently coerces a malformed segment to zero. This is the single
/// home for ISO-8601 duration parsing across the workspace; `datum-core`'s
/// `Dur` delegates here.
pub fn parse_iso_duration(s: &str) -> Result<Duration> {
    let err = || ParseError::InvalidTime(format!("Invalid ISO 8601 duration: '{s}'"));
    let (sign, body) = match s.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, s),
    };
    let rest = body.strip_prefix('P').ok_or_else(err)?;
    if rest.is_empty() {
        return Err(err());
    }

    // Week form (`PnW`) is mutually exclusive with the rest of the grammar.
    if rest.ends_with('W') && !rest.contains('T') {
        let n: f64 = rest[..rest.len() - 1].parse().map_err(|_| err())?;
        return Ok(Duration::seconds((sign * n * 7.0 * 86400.0) as i64));
    }

    let mut total: f64 = 0.0;
    let mut buf = String::new();
    let mut seen_t = false;
    let mut had_any = false;

    for c in rest.chars() {
        if c == 'T' {
            if seen_t || !buf.is_empty() {
                return Err(err());
            }
            seen_t = true;
        } else if c.is_ascii_digit() || c == '.' || c == ',' {
            buf.push(if c == ',' { '.' } else { c });
        } else {
            if buf.is_empty() {
                return Err(err());
            }
            let n: f64 = buf.parse().map_err(|_| err())?;
            buf.clear();
            let secs = match (seen_t, c) {
                (false, 'Y') => n * 365.0 * 86400.0,
                (false, 'M') => n * 30.0 * 86400.0,
                (false, 'D') => n * 86400.0,
                (true, 'H') => n * 3600.0,
                (true, 'M') => n * 60.0,
                (true, 'S') => n,
                _ => return Err(err()),
            };
            total += secs;
            had_any = true;
        }
    }

    // Trailing digits with no unit, or no segments at all, are malformed.
    if !buf.is_empty() || !had_any {
        return Err(err());
    }
    Ok(Duration::seconds((sign * total) as i64))
}

/// Format a `chrono::Duration` as a canonical ISO 8601 string (`PT0S`,
/// `PT5M`, `PT1H30M`, `P1D`, `-PT5M`). Round-trips with
/// [`parse_iso_duration`].
pub fn format_iso_duration(d: Duration) -> String {
    let total = d.num_seconds();
    if total == 0 {
        return "PT0S".to_string();
    }
    let (sign, mut secs) = if total < 0 {
        ("-", -total)
    } else {
        ("", total)
    };
    let days = secs / 86400;
    secs %= 86400;
    let hours = secs / 3600;
    secs %= 3600;
    let mins = secs / 60;
    secs %= 60;

    let mut out = String::from(sign);
    out.push('P');
    if days > 0 {
        out.push_str(&format!("{days}D"));
    }
    if hours > 0 || mins > 0 || secs > 0 {
        out.push('T');
        if hours > 0 {
            out.push_str(&format!("{hours}H"));
        }
        if mins > 0 {
            out.push_str(&format!("{mins}M"));
        }
        if secs > 0 {
            out.push_str(&format!("{secs}S"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_rejects_unknown_unit() {
        // Q is not a known unit — must error, not silently parse as zero.
        assert!(parse_iso_duration("PT5Q").is_err());
    }

    #[test]
    fn iso_rejects_unparseable_number() {
        assert!(parse_iso_duration("PTxM").is_err());
    }

    #[test]
    fn iso_rejects_empty_body() {
        assert!(parse_iso_duration("PT").is_err());
        assert!(parse_iso_duration("P").is_err());
    }

    #[test]
    fn iso_rejects_trailing_digits() {
        // A number with no unit is malformed.
        assert!(parse_iso_duration("PT5").is_err());
    }

    #[test]
    fn iso_parses_well_known_compound() {
        let d = parse_iso_duration("P1Y2M3DT4H5M6S").unwrap();
        let expect = 365 * 86400 + 2 * 30 * 86400 + 3 * 86400 + 4 * 3600 + 5 * 60 + 6;
        assert_eq!(d.num_seconds(), expect);
    }

    #[test]
    fn iso_parses_week_and_negative() {
        assert_eq!(
            parse_iso_duration("P3W").unwrap().num_seconds(),
            3 * 7 * 86400
        );
        assert_eq!(parse_iso_duration("-PT5M").unwrap().num_seconds(), -300);
    }

    #[test]
    fn iso_round_trips_through_formatter() {
        for d in [
            Duration::zero(),
            Duration::minutes(5),
            Duration::seconds(3600 + 30 * 60),
            Duration::days(1),
            Duration::seconds(-(86400 + 5 * 3600 + 6)),
        ] {
            let s = format_iso_duration(d);
            assert_eq!(
                parse_iso_duration(&s).unwrap(),
                d,
                "round-trip failed for {s}"
            );
        }
    }

    #[test]
    fn format_zero_is_pt0s() {
        assert_eq!(format_iso_duration(Duration::zero()), "PT0S");
    }
}
