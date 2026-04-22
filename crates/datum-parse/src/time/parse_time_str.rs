//! Parse `-t` / `--time` arguments, plus stacked-flag counting helpers.

use chrono::NaiveTime;

use crate::error::{ParseError, Result};
use crate::time::uncertainty::{parse_uncertainty, split_uncertainty, Uncertainty};

/// Parsed result of a time argument (-t flag).
pub struct TimeArg {
    pub minutes_offset: Option<i64>,
    pub absolute: Option<NaiveTime>,
    pub uncertainty: Option<Uncertainty>,
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
///   "+5" / "-8"         relative minutes (+ = future, - = past)
///   "1513" / "15:13"    absolute HH:MM today
///   "9:15am" / "21:30"  absolute time today
/// plus an optional trailing uncertainty `±N[unit]` / `+-N[unit]`.
pub fn parse_time_arg(s: &str) -> Result<TimeArg> {
    let (primary, uncert_body) = split_uncertainty(s);
    let uncertainty = uncert_body.map(parse_uncertainty).transpose()?;

    // Literal "now" → current moment (minutes_offset = 0). Matches datum.
    if primary.eq_ignore_ascii_case("now") {
        return Ok(TimeArg { minutes_offset: Some(0), absolute: None, uncertainty });
    }

    // Relative minute offset: "+5" (future) or "-8" (past).
    if let Ok(n) = primary.parse::<i64>() {
        if primary.starts_with('+') || primary.starts_with('-') {
            return Ok(TimeArg { minutes_offset: Some(n), absolute: None, uncertainty });
        }
        // Plain unsigned number → absolute time.
        // "H" / "HH" → hours; "HMM" → 3-digit hour+minute; "HHMM" → 4-digit.
        // Datum treats 24 as next-midnight.
        if let Some(t) = plain_number_to_time(primary) {
            return Ok(TimeArg { minutes_offset: None, absolute: Some(t), uncertainty });
        }
        // Fall back: relative minutes.
        return Ok(TimeArg { minutes_offset: Some(n), absolute: None, uncertainty });
    }

    // `5am` / `5pm` / `5a` / `5p` — hour + meridian, no minutes.
    if let Some(t) = parse_hour_meridian(primary) {
        return Ok(TimeArg { minutes_offset: None, absolute: Some(t), uncertainty });
    }

    // Absolute time formats. Short-meridian forms (single-char `a`/`p`)
    // aren't directly supported by chrono, so we handle them explicitly.
    if let Some(t) = parse_short_meridian(primary) {
        return Ok(TimeArg { minutes_offset: None, absolute: Some(t), uncertainty });
    }
    for fmt in &["%H:%M:%S%.f", "%H:%M:%S", "%H:%M", "%I:%M%P", "%I:%M %p", "%I%P", "%I %p"] {
        if let Ok(t) = NaiveTime::parse_from_str(primary, fmt) {
            return Ok(TimeArg { minutes_offset: None, absolute: Some(t), uncertainty });
        }
    }

    Err(ParseError::InvalidTime(format!("Cannot parse time: '{s}'")))
}

/// Convert a plain-integer string (no sign, variable width) into a
/// `NaiveTime`. Mirrors datum's parseTimeStr: 1–2 digits = hours,
/// 3 digits = H:MM, 4 digits = HH:MM.
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
