//! `DatumTime` — the canonical timestamp stored on every entry.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::error::{ParseError, Result};

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
    /// Construct `DatumTime` from a UTC instant and an IANA timezone name.
    /// Returns `InvalidTime` if the name isn't recognised.
    pub fn from_utc_and_tz(utc: DateTime<Utc>, timezone: &str) -> Result<Self> {
        let tz: Tz = timezone.parse().map_err(|_| {
            ParseError::InvalidTime(format!("Unknown timezone: {timezone}"))
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

#[cfg(feature = "native")]
impl DatumTime {
    /// Snapshot the current moment using the host's local timezone.
    ///
    /// Requires the `native` feature (default-on); disable when targeting
    /// `wasm32-unknown-unknown` and construct via [`DatumTime::from_utc_and_tz`]
    /// with an explicit timezone.
    pub fn now() -> Self {
        let local = chrono::Local::now();
        let utc = local.with_timezone(&Utc);
        let offset_minutes = local.offset().local_minus_utc() / 60;
        let timezone = iana_time_zone::get_timezone()
            .unwrap_or_else(|_| "UTC".to_string());
        DatumTime { utc, offset_minutes, timezone }
    }
}
