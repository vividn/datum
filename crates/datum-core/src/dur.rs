//! Parsed view over the `dur` text column.
//!
//! Storage stays as `Option<String>` on `Entry::dur` (da-ju71.2 narrows
//! scope: the type-cascade is parked under da-ju71.6). This enum is the
//! semantic interpretation that callers reach for when they want to
//! reason about a row's duration intent without re-implementing the
//! grammar each time.
//!
//! Storage forms:
//! - `"MOMENT"` → [`Dur::Moment`] — explicit point-in-time (occur).
//! - ISO-8601 duration (`"PT5M"`, `"P1D"`, …) → [`Dur::Of`] — a real span.
//! - `"UNKNOWN"` → [`Dur::Unknown`] — open-ended / not-yet-known.
//!
//! `parse` returns `None` for anything else; the verb-derivation path
//! preserves today's "malformed ⇒ error" behavior by falling through to
//! the existing ISO-8601 classifier rather than treating an unparseable
//! string as `Unknown`.

use chrono::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dur {
    Moment,
    Of(Duration),
    Unknown,
}

impl Dur {
    /// Parse a stored `dur` value into its semantic form. Returns `None`
    /// for malformed ISO-8601 spans so the caller can decide whether to
    /// surface a parse error or fall back.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "MOMENT" => Some(Dur::Moment),
            "UNKNOWN" => Some(Dur::Unknown),
            iso => parse_iso_duration(iso).map(Dur::Of),
        }
    }

    /// Render to the canonical storage string.
    pub fn to_storage_string(&self) -> String {
        match self {
            Dur::Moment => "MOMENT".to_string(),
            Dur::Unknown => "UNKNOWN".to_string(),
            Dur::Of(d) => format_iso_duration(*d),
        }
    }
}

fn parse_iso_duration(input: &str) -> Option<Duration> {
    let (sign, body) = match input.strip_prefix('-') {
        Some(rest) => (-1i64, rest),
        None => (1, input),
    };
    let rest = body.strip_prefix('P')?;
    if rest.is_empty() {
        return None;
    }

    if rest.ends_with('W') && !rest.contains('T') {
        let n_str = &rest[..rest.len() - 1];
        let n: f64 = n_str.parse().ok()?;
        let secs = (n * 7.0 * 86400.0) as i64;
        return Some(Duration::seconds(sign * secs));
    }

    let mut total: f64 = 0.0;
    let mut buf = String::new();
    let mut seen_t = false;
    let mut had_any = false;

    for c in rest.chars() {
        if c == 'T' {
            if seen_t || !buf.is_empty() {
                return None;
            }
            seen_t = true;
        } else if c.is_ascii_digit() || c == '.' || c == ',' {
            buf.push(if c == ',' { '.' } else { c });
        } else {
            if buf.is_empty() {
                return None;
            }
            let n: f64 = buf.parse().ok()?;
            buf.clear();
            let secs = match (seen_t, c) {
                (false, 'Y') => n * 365.0 * 86400.0,
                (false, 'M') => n * 30.0 * 86400.0,
                (false, 'D') => n * 86400.0,
                (true, 'H') => n * 3600.0,
                (true, 'M') => n * 60.0,
                (true, 'S') => n,
                _ => return None,
            };
            total += secs;
            had_any = true;
        }
    }

    if !buf.is_empty() || !had_any {
        return None;
    }
    Some(Duration::seconds(sign * total as i64))
}

fn format_iso_duration(d: Duration) -> String {
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
    fn moment_round_trips() {
        let d = Dur::parse("MOMENT").unwrap();
        assert_eq!(d, Dur::Moment);
        assert_eq!(d.to_storage_string(), "MOMENT");
    }

    #[test]
    fn unknown_round_trips() {
        let d = Dur::parse("UNKNOWN").unwrap();
        assert_eq!(d, Dur::Unknown);
        assert_eq!(d.to_storage_string(), "UNKNOWN");
    }

    #[test]
    fn iso_duration_round_trips() {
        let d = Dur::parse("PT5M").unwrap();
        assert_eq!(d, Dur::Of(Duration::minutes(5)));
        assert_eq!(d.to_storage_string(), "PT5M");
    }

    #[test]
    fn iso_day_duration_round_trips() {
        let d = Dur::parse("P1D").unwrap();
        assert_eq!(d, Dur::Of(Duration::days(1)));
        assert_eq!(d.to_storage_string(), "P1D");
    }

    #[test]
    fn malformed_returns_none() {
        for bad in ["", "moment", "P", "PXX", "5", "PT5"] {
            assert!(Dur::parse(bad).is_none(), "expected None for {bad:?}");
        }
    }
}
