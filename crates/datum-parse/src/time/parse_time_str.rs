//! Parse `-t` / `--time` arguments, plus stacked-flag counting helpers.

use chrono::{DateTime, NaiveDateTime, NaiveTime, Utc};

use crate::error::{ParseError, Result};
use crate::time::uncertainty::{
    parse_uncertainty, split_direction_suffix, split_uncertainty, Direction, Uncertainty,
};

/// Parsed result of a time argument (-t flag).
pub struct TimeArg {
    pub minutes_offset: Option<i64>,
    pub absolute: Option<NaiveTime>,
    /// Full ISO-8601 datetime with timezone suffix (`Z` or `±HH:MM`) — a
    /// fully-resolved instant. Caller stores `utc` directly and keeps its
    /// own `timezone` field (e.g. from `-z`).
    pub iso_instant: Option<DateTime<Utc>>,
    /// ISO-8601 datetime without a timezone suffix
    /// (`2024-03-16T15:30:00`). Caller interprets the wall-clock moment
    /// in its current effective timezone.
    pub iso_naive: Option<NaiveDateTime>,
    pub uncertainty: Option<Uncertainty>,
    /// Trailing `+` / `-` direction hint. Forces resolution into the next
    /// (`Future`) or most-recent-prior (`Past`) occurrence. Already-pinned
    /// inputs (ISO instants, ISO naive datetimes, relative `±N` offsets)
    /// ignore the hint.
    pub direction: Option<Direction>,
}

/// Count stackable short flags like -yyy (3) or -qqqq (4).
/// Returns None if the string isn't a pure repetition of `ch`.
pub fn count_stacked_flag(s: &str, ch: char) -> Option<usize> {
    if s.starts_with('-') && s[1..].chars().all(|c| c == ch) && !s[1..].is_empty() {
        Some(s[1..].len())
    } else {
        None
    }
}

/// Parse a -t / --time argument.
/// Supports:
///   "+5" / "-8"                           relative minutes (+ = future, - = past)
///   "1513" / "15:13"                      absolute HH:MM today
///   "9:15am" / "21:30"                    absolute time today
///   "2024-03-16T15:30:00Z"                ISO-8601 instant (Z or ±HH:MM)
///   "2024-03-16T15:30:00"                 ISO-8601 datetime, no tz
///                                         (caller interprets in effective tz)
/// plus an optional trailing uncertainty `±N[unit]` / `+-N[unit]`.
///
/// An unsigned integer that is not a valid clock time (e.g. `2500`) is
/// rejected rather than treated as a relative minute-offset — relative
/// offsets require an explicit leading sign (`+2500` / `-2500`).
pub fn parse_time_arg(s: &str) -> Result<TimeArg> {
    // Direction suffix (`+` / `-`) sits at the very tail, after any
    // uncertainty marker. Strip it first so `3±5m+` and `3+` both work.
    let (s_no_dir, direction) = split_direction_suffix(s)?;
    let (primary, uncert_body) = split_uncertainty(s_no_dir);
    let uncertainty = uncert_body.map(parse_uncertainty).transpose()?;

    // Literal "now" → current moment (minutes_offset = 0). Matches datum.
    if primary.eq_ignore_ascii_case("now") {
        return Ok(TimeArg {
            minutes_offset: Some(0),
            absolute: None,
            iso_instant: None,
            iso_naive: None,
            uncertainty,
            direction,
        });
    }

    // ISO-8601 datetime: `YYYY-MM-DDTHH:MM[:SS[.frac]][Z|±HH:MM]`. We detect
    // the `T` separator (or space) preceded by a date prefix before trying
    // the time-only branches, so the date portion isn't misread as a number.
    if looks_like_iso_datetime(primary) {
        if let Ok(dt) = DateTime::parse_from_rfc3339(primary) {
            return Ok(TimeArg {
                minutes_offset: None,
                absolute: None,
                iso_instant: Some(dt.with_timezone(&Utc)),
                iso_naive: None,
                uncertainty,
                direction,
            });
        }
        for fmt in &[
            "%Y-%m-%dT%H:%M:%S%.f",
            "%Y-%m-%dT%H:%M:%S",
            "%Y-%m-%dT%H:%M",
        ] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(primary, fmt) {
                return Ok(TimeArg {
                    minutes_offset: None,
                    absolute: None,
                    iso_instant: None,
                    iso_naive: Some(naive),
                    uncertainty,
                    direction,
                });
            }
        }
        return Err(ParseError::InvalidTime(format!(
            "Cannot parse ISO-8601 datetime: '{s}'"
        )));
    }

    // Relative minute offset: "+5" (future) or "-8" (past).
    if let Ok(n) = primary.parse::<i64>() {
        if primary.starts_with('+') || primary.starts_with('-') {
            return Ok(TimeArg {
                minutes_offset: Some(n),
                absolute: None,
                iso_instant: None,
                iso_naive: None,
                uncertainty,
                direction,
            });
        }
        // Plain unsigned number → absolute time.
        // "H" / "HH" → hours; "HMM" → 3-digit hour+minute; "HHMM" → 4-digit.
        // Datum treats 24 as next-midnight.
        if let Some(t) = plain_number_to_time(primary) {
            return Ok(TimeArg {
                minutes_offset: None,
                absolute: Some(t),
                iso_instant: None,
                iso_naive: None,
                uncertainty,
                direction,
            });
        }
        // Unsigned integer that isn't a valid clock time. Reject —
        // relative offsets require a leading sign. (da-4h7k: previously
        // this fell through to a minutes-offset, so `2500` silently
        // resolved to 2500 minutes from now instead of being flagged as
        // the likely `2400`/`24:00` typo it almost always is.)
        return Err(ParseError::InvalidTime(format!(
            "Cannot parse time: '{primary}' (must be HH/HHMM/HHMMSS as a \
             valid clock time; use '+{primary}' or '-{primary}' for \
             relative minutes)"
        )));
    }

    // `5am` / `5pm` / `5a` / `5p` — hour + meridian, no minutes.
    if let Some(t) = parse_hour_meridian(primary) {
        return Ok(TimeArg {
            minutes_offset: None,
            absolute: Some(t),
            iso_instant: None,
            iso_naive: None,
            uncertainty,
            direction,
        });
    }

    // Absolute time formats. Short-meridian forms (single-char `a`/`p`)
    // aren't directly supported by chrono, so we handle them explicitly.
    if let Some(t) = parse_short_meridian(primary) {
        return Ok(TimeArg {
            minutes_offset: None,
            absolute: Some(t),
            iso_instant: None,
            iso_naive: None,
            uncertainty,
            direction,
        });
    }
    for fmt in &[
        "%H:%M:%S%.f",
        "%H:%M:%S",
        "%H:%M",
        "%I:%M%P",
        "%I:%M %p",
        "%I%P",
        "%I %p",
    ] {
        if let Ok(t) = NaiveTime::parse_from_str(primary, fmt) {
            return Ok(TimeArg {
                minutes_offset: None,
                absolute: Some(t),
                iso_instant: None,
                iso_naive: None,
                uncertainty,
                direction,
            });
        }
    }

    Err(ParseError::InvalidTime(format!("Cannot parse time: '{s}'")))
}

/// Cheap heuristic for ISO-8601 datetime: at least 11 chars, dashes at
/// positions 4 and 7, `T` (or space) at position 10. No NLP risk — this
/// only matches structured calendar prefixes.
fn looks_like_iso_datetime(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 11 {
        return false;
    }
    bytes[4] == b'-'
        && bytes[7] == b'-'
        && (bytes[10] == b'T' || bytes[10] == b't' || bytes[10] == b' ')
}

/// Convert a plain-integer string (no sign, variable width) into a
/// `NaiveTime`. Mirrors datum's parseTimeStr: 1–2 digits = hours,
/// 3 digits = H:MM, 4 digits = HH:MM, 5 digits = HMMSS,
/// 6 digits = HH:MM:SS (da-xvsz — previously a 6-digit form fell
/// through to minute-offset interpretation, e.g. `-t 153045`
/// would land 153,045 minutes from now instead of at 15:30:45).
fn plain_number_to_time(s: &str) -> Option<NaiveTime> {
    if s.is_empty() || s.starts_with(['+', '-']) || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u32 = s.parse().ok()?;
    match s.len() {
        1 | 2 => {
            // "0".."24". Datum treats 24 as next-midnight; NaiveTime can't
            // represent that, so we return 00:00:00 and leave the day roll
            // to the composition layer.
            if n == 24 {
                return NaiveTime::from_hms_opt(0, 0, 0);
            }
            if n > 23 {
                return None;
            }
            NaiveTime::from_hms_opt(n, 0, 0)
        }
        3 => {
            let h = n / 100;
            let m = n % 100;
            NaiveTime::from_hms_opt(h, m, 0)
        }
        4 => {
            let h = n / 100;
            let m = n % 100;
            NaiveTime::from_hms_opt(h, m, 0)
        }
        5 | 6 => {
            // HMMSS (5) or HHMMSS (6). Single-digit hour for 5-digit form;
            // full HH:MM:SS for 6-digit (da-xvsz — the previously-missing
            // arm that made `-t 153045` land 153_045 minutes from now).
            let h = n / 10000;
            let m = (n / 100) % 100;
            let sec = n % 100;
            NaiveTime::from_hms_opt(h, m, sec)
        }
        _ => None,
    }
}

/// Parse an hour-only meridian form: `5am`, `5pm`, `12a`, `12p`.
fn parse_hour_meridian(s: &str) -> Option<NaiveTime> {
    let lower = s.to_ascii_lowercase();
    let (num_part, meridian) = if let Some(stripped) = lower.strip_suffix("am") {
        (stripped, 'a')
    } else if let Some(stripped) = lower.strip_suffix("pm") {
        (stripped, 'p')
    } else if let Some(stripped) = lower.strip_suffix('a') {
        (stripped, 'a')
    } else if let Some(stripped) = lower.strip_suffix('p') {
        (stripped, 'p')
    } else {
        return None;
    };
    if num_part.is_empty() || !num_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // Colon-containing forms are handled by chrono's format strings.
    let n: u32 = num_part.parse().ok()?;
    if n == 0 || n > 12 {
        return None;
    }
    let h = match (meridian, n) {
        ('a', 12) => 0,
        ('a', h) => h,
        ('p', 12) => 12,
        ('p', h) => h + 12,
        _ => return None,
    };
    NaiveTime::from_hms_opt(h, 0, 0)
}

/// Parse `H:MM<m>` / `H:MM:SS<m>` where `<m>` is a single-letter meridian
/// (`a` / `p`). Datum accepts these; chrono's format specifiers require
/// the two-letter form.
fn parse_short_meridian(s: &str) -> Option<NaiveTime> {
    let lower = s.to_ascii_lowercase();
    let (body, meridian) = if let Some(stripped) = lower.strip_suffix('a') {
        if stripped.ends_with("am") {
            return None; // already 2-letter; fall through to chrono
        }
        (stripped, 'a')
    } else if let Some(stripped) = lower.strip_suffix('p') {
        if stripped.ends_with("pm") {
            return None;
        }
        (stripped, 'p')
    } else {
        return None;
    };
    // Body must include at least one `:` — bare-hour meridian is handled
    // by `parse_hour_meridian` instead.
    if !body.contains(':') {
        return None;
    }
    let t = NaiveTime::parse_from_str(body, "%H:%M:%S%.f")
        .or_else(|_| NaiveTime::parse_from_str(body, "%H:%M:%S"))
        .or_else(|_| NaiveTime::parse_from_str(body, "%H:%M"))
        .ok()?;
    use chrono::Timelike;
    let h = t.hour();
    if h > 12 {
        return None;
    }
    let shifted_h = match (meridian, h) {
        ('a', 12) => 0,
        ('a', h) => h,
        ('p', 12) => 12,
        ('p', h) => h + 12,
        _ => return None,
    };
    NaiveTime::from_hms_opt(shifted_h, t.minute(), t.second())
        .and_then(|nt| nt.with_nanosecond(t.nanosecond()))
}
