//! Time parsing for datum entries.
//!
//! Supports datum-compatible syntax:
//!   -y / -yy / -yyy     = 1/2/3 days ago (stackable)
//!   -q / -qq / -qqqq    = 5/10/20 min ago (stackable, 5 min each)
//!   -d -1 / -d +1       = relative day offset
//!   -t -8 / -t +5       = relative minute offset from now
//!   -t 1513             = 15:13 absolute time today
//!   -t 9:15am           = natural time today
//!   ISO 8601 full timestamps

use chrono::{DateTime, Duration, Local, NaiveTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::error::{DatumError, Result};

/// A timestamp with full timezone metadata, as stored in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatumTime {
    /// UTC timestamp (canonical)
    pub utc: DateTime<Utc>,
    /// UTC offset in minutes (e.g. -300 for UTC-5)
    pub offset_minutes: i32,
    /// IANA timezone name (e.g. "America/Chicago")
    pub timezone: String,
}

impl DatumTime {
    pub fn now() -> Self {
        let local = Local::now();
        let utc = local.with_timezone(&Utc);
        let offset_minutes = local.offset().local_minus_utc() / 60;
        let timezone = iana_time_zone::get_timezone()
            .unwrap_or_else(|_| "UTC".to_string());
        DatumTime { utc, offset_minutes, timezone }
    }

    pub fn from_utc_and_tz(utc: DateTime<Utc>, timezone: &str) -> Result<Self> {
        let tz: Tz = timezone.parse().map_err(|_| {
            DatumError::InvalidTime(format!("Unknown timezone: {timezone}"))
        })?;
        let local = utc.with_timezone(&tz);
        // chrono-tz: convert to fixed_offset to get total UTC offset in minutes
        let offset_secs = local.fixed_offset().offset().local_minus_utc();
        let offset_minutes = offset_secs / 60;
        Ok(DatumTime {
            utc,
            offset_minutes,
            timezone: timezone.to_string(),
        })
    }

    pub fn local_time(&self) -> DateTime<chrono::FixedOffset> {
        let offset = chrono::FixedOffset::east_opt(self.offset_minutes * 60)
            .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
        self.utc.with_timezone(&offset)
    }
}

/// Parsed result of a date argument (-d flag).
pub struct DateArg {
    pub days_offset: i64,
    pub absolute: Option<chrono::NaiveDate>,
}

/// Parsed result of a time argument (-t flag).
pub struct TimeArg {
    pub minutes_offset: Option<i64>,
    pub absolute: Option<NaiveTime>,
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

/// Parse a -d / --date argument.
/// Supports: "+1", "-1", "0", or ISO date "2024-03-16"
pub fn parse_date_arg(s: &str) -> Result<DateArg> {
    // Relative integer offset
    if let Ok(n) = s.parse::<i64>() {
        return Ok(DateArg { days_offset: n, absolute: None });
    }

    // Absolute ISO date
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(DateArg { days_offset: 0, absolute: Some(d) });
    }

    Err(DatumError::InvalidTime(format!("Cannot parse date: '{s}'")))
}

/// Parse a -t / --time argument.
/// Supports:
///   "+5" / "-8"         relative minutes
///   "1513" / "15:13"    absolute HH:MM today
///   "9:15am" / "21:30"  absolute time today
pub fn parse_time_arg(s: &str) -> Result<TimeArg> {
    // Literal "now" → current moment (minutes_offset = 0). Matches datum.
    if s.eq_ignore_ascii_case("now") {
        return Ok(TimeArg { minutes_offset: Some(0), absolute: None });
    }

    // Relative minute offset: "+5" or "-8"
    if let Ok(n) = s.parse::<i64>() {
        if s.starts_with('+') || s.starts_with('-') {
            return Ok(TimeArg { minutes_offset: Some(n), absolute: None });
        }
        // Plain number with no sign: treat as HHMM if 3-4 digits, else relative
        if s.len() == 4 {
            let h = s[..2].parse::<u32>().ok();
            let m = s[2..].parse::<u32>().ok();
            if let (Some(h), Some(m)) = (h, m) {
                if let Some(t) = NaiveTime::from_hms_opt(h, m, 0) {
                    return Ok(TimeArg { minutes_offset: None, absolute: Some(t) });
                }
            }
        }
        // 1-2 digit plain number: relative minutes
        return Ok(TimeArg { minutes_offset: Some(n), absolute: None });
    }

    // Absolute time formats
    for fmt in &["%H:%M:%S", "%H:%M", "%I:%M%P", "%I:%M %p", "%I%P", "%I %p"] {
        if let Ok(t) = NaiveTime::parse_from_str(s, fmt) {
            return Ok(TimeArg { minutes_offset: None, absolute: Some(t) });
        }
    }

    Err(DatumError::InvalidTime(format!("Cannot parse time: '{s}'")))
}

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
                let h: i64 = parts[0].parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
                let m: i64 = parts[1].parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
                Ok(Duration::minutes(h * 60 + m))
            }
            3 => {
                let h: i64 = parts[0].parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
                let m: i64 = parts[1].parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
                let sec: i64 = parts[2].parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
                Ok(Duration::seconds(h * 3600 + m * 60 + sec))
            }
            4 => {
                let d: i64 = parts[0].parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
                let h: i64 = parts[1].parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
                let m: i64 = parts[2].parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
                let sec: i64 = parts[3].parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
                Ok(Duration::seconds(d * 86400 + h * 3600 + m * 60 + sec))
            }
            _ => Err(DatumError::InvalidTime(s.to_string())),
        };
    }

    // Named unit suffixes
    let (num_str, unit) = split_unit(s);
    let n: f64 = num_str.parse().map_err(|_| DatumError::InvalidTime(s.to_string()))?;
    let secs = match unit {
        "s" | "sec" | "second" | "seconds" => (n) as i64,
        "m" | "min" | "minute" | "minutes" => (n * 60.0) as i64,
        "h" | "hr" | "hour" | "hours" => (n * 3600.0) as i64,
        "d" | "day" | "days" => (n * 86400.0) as i64,
        "" => (n * 60.0) as i64, // default: minutes
        _ => return Err(DatumError::InvalidTime(format!("Unknown duration unit: '{unit}'"))),
    };
    Ok(Duration::seconds(secs))
}

fn split_unit(s: &str) -> (&str, &str) {
    let idx = s
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphabetic())
        .last()
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    (&s[..idx], &s[idx..])
}

fn parse_iso_duration(s: &str) -> Result<Duration> {
    // Minimal ISO 8601 duration parser: PT5M, PT1H30M, P1DT5H, etc.
    let s = if s.starts_with('P') { &s[1..] } else { s };
    let mut secs: i64 = 0;
    let (date_part, time_part) = if let Some(idx) = s.find('T') {
        (&s[..idx], &s[idx + 1..])
    } else {
        (s, "")
    };

    for (part, multiplier) in [
        (date_part, vec![('Y', 365 * 86400), ('M', 30 * 86400), ('W', 7 * 86400), ('D', 86400)]),
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

/// Build a `DatumTime` from the combination of -y stacks, -q stacks,
/// -d arg, -t arg, and an optional base time (defaults to now).
pub struct TimeBuilder {
    pub days_back: i64,
    pub minutes_back: i64,
    pub date_override: Option<chrono::NaiveDate>,
    pub time_override: Option<NaiveTime>,
}

impl TimeBuilder {
    pub fn now() -> Self {
        TimeBuilder {
            days_back: 0,
            minutes_back: 0,
            date_override: None,
            time_override: None,
        }
    }

    pub fn with_yesterday_count(mut self, n: i64) -> Self {
        self.days_back += n;
        self
    }

    pub fn with_quick_count(mut self, n: i64) -> Self {
        self.minutes_back += n * 5;
        self
    }

    pub fn with_date_arg(mut self, arg: DateArg) -> Self {
        if let Some(abs) = arg.absolute {
            self.date_override = Some(abs);
        } else {
            self.days_back += -arg.days_offset; // -1 means yesterday → days_back += 1
        }
        self
    }

    pub fn with_time_arg(mut self, arg: TimeArg) -> Self {
        if let Some(abs) = arg.absolute {
            self.time_override = Some(abs);
        } else if let Some(offset) = arg.minutes_offset {
            self.minutes_back += -offset; // -8 means 8 min ago → minutes_back += 8
        }
        self
    }

    pub fn build(self) -> DatumTime {
        let mut dt = Local::now();

        // Apply minute offsets first
        dt = dt - chrono::Duration::minutes(self.minutes_back);

        // Apply day offsets
        dt = dt - chrono::Duration::days(self.days_back);

        // Override date if specified
        if let Some(date) = self.date_override {
            let time = self.time_override.unwrap_or_else(|| dt.time());
            if let Some(new_dt) = date.and_time(time).and_local_timezone(Local).single() {
                dt = new_dt;
            }
        } else if let Some(time) = self.time_override {
            if let Some(new_dt) = dt.date_naive().and_time(time).and_local_timezone(Local).single() {
                dt = new_dt;
            }
        }

        let utc = dt.with_timezone(&Utc);
        let offset_minutes = dt.offset().local_minus_utc() / 60;
        let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());
        DatumTime { utc, offset_minutes, timezone }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stacked_flags() {
        assert_eq!(count_stacked_flag("-y", 'y'), Some(1));
        assert_eq!(count_stacked_flag("-yy", 'y'), Some(2));
        assert_eq!(count_stacked_flag("-yyy", 'y'), Some(3));
        assert_eq!(count_stacked_flag("-qqqq", 'q'), Some(4));
        assert_eq!(count_stacked_flag("-d", 'y'), None);
        assert_eq!(count_stacked_flag("y", 'y'), None);
    }

    #[test]
    fn test_parse_time_arg_hhmm() {
        let t = parse_time_arg("1513").unwrap();
        assert_eq!(t.absolute.unwrap(), NaiveTime::from_hms_opt(15, 13, 0).unwrap());
    }

    #[test]
    fn test_parse_time_arg_relative() {
        let t = parse_time_arg("-8").unwrap();
        assert_eq!(t.minutes_offset.unwrap(), -8);
        let t = parse_time_arg("+5").unwrap();
        assert_eq!(t.minutes_offset.unwrap(), 5);
    }

    #[test]
    fn test_parse_duration_minutes_default() {
        assert_eq!(parse_duration("5").unwrap(), Duration::minutes(5));
        assert_eq!(parse_duration("30m").unwrap(), Duration::minutes(30));
        assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
    }

    #[test]
    fn test_parse_duration_colon() {
        assert_eq!(parse_duration("1:30").unwrap(), Duration::minutes(90));
        assert_eq!(parse_duration("1:15:30").unwrap(), Duration::seconds(4530));
    }

    #[test]
    fn test_parse_iso_duration() {
        assert_eq!(parse_duration("PT5M").unwrap(), Duration::minutes(5));
        assert_eq!(parse_duration("PT1H30M").unwrap(), Duration::minutes(90));
    }
}
