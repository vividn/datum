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
        // Plain number with no sign: treat as HHMM if 3-4 digits, else relative
        if primary.len() == 4 {
            let h = primary[..2].parse::<u32>().ok();
            let m = primary[2..].parse::<u32>().ok();
            if let (Some(h), Some(m)) = (h, m) {
                if let Some(t) = NaiveTime::from_hms_opt(h, m, 0) {
                    return Ok(TimeArg {
                        minutes_offset: None,
                        absolute: Some(t),
                        uncertainty,
                    });
                }
            }
        }
        // 1-2 digit plain number: relative minutes
        return Ok(TimeArg { minutes_offset: Some(n), absolute: None, uncertainty });
    }

    // Absolute time formats
    for fmt in &["%H:%M:%S", "%H:%M", "%I:%M%P", "%I:%M %p", "%I%P", "%I %p"] {
        if let Ok(t) = NaiveTime::parse_from_str(primary, fmt) {
            return Ok(TimeArg { minutes_offset: None, absolute: Some(t), uncertainty });
        }
    }

    Err(ParseError::InvalidTime(format!("Cannot parse time: '{s}'")))
}
