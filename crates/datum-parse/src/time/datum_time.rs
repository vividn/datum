//! `DatumTime` — the canonical timestamp stored on every entry.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::error::{ParseError, Result};

/// A timestamp with full timezone metadata, as stored in the database.
///
/// The three fields are **not independent**: `offset_minutes` is the signed
/// UTC offset (in minutes) of `utc` resolved at `timezone`. Construct via
/// [`DatumTime::from_utc_and_tz`] to compute it correctly. Directly mutating
/// `utc` or `timezone` without also updating `offset_minutes` desynchronizes
/// the struct — [`DatumTime::local_time`] then returns the wrong wall-clock
/// time without erroring, silently corrupting downstream renders / slug
/// derivations. After such a mutation (e.g. shifting across a DST boundary)
/// call [`DatumTime::refresh_offset`] to re-derive the offset.
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
        let tz: Tz = timezone
            .parse()
            .map_err(|_| ParseError::InvalidTime(format!("Unknown timezone: {timezone}")))?;
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

    /// Re-derive `offset_minutes` from `utc` resolved at `timezone`.
    ///
    /// Call after directly mutating `utc` or `timezone`, or after shifting
    /// across a DST boundary, to restore the struct's invariant. Returns
    /// `InvalidTime` if `timezone` isn't a recognised IANA name.
    pub fn refresh_offset(&mut self) -> Result<()> {
        let tz: Tz = self
            .timezone
            .parse()
            .map_err(|_| ParseError::InvalidTime(format!("Unknown timezone: {}", self.timezone)))?;
        let offset_secs = self
            .utc
            .with_timezone(&tz)
            .fixed_offset()
            .offset()
            .local_minus_utc();
        self.offset_minutes = offset_secs / 60;
        Ok(())
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
    /// Routes through [`crate::time::clock`]: tests can install a
    /// [`crate::time::clock::MockClock`] on the current thread to make
    /// `now()` return a fixed instant; production code installs nothing
    /// and falls through to [`crate::time::clock::SystemClock`].
    ///
    /// Requires the `native` feature (default-on); disable when targeting
    /// `wasm32-unknown-unknown` and construct via [`DatumTime::from_utc_and_tz`]
    /// with an explicit timezone.
    pub fn now() -> Self {
        crate::time::clock::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        s.parse::<DateTime<Utc>>().unwrap()
    }

    #[test]
    fn refresh_offset_re_derives_across_dst() {
        // Constructed in winter (CST, -360); corrupting then refreshing at a
        // summer instant (CDT, -300) must re-project from utc + timezone.
        let mut t =
            DatumTime::from_utc_and_tz(at("2024-01-15T12:00:00Z"), "America/Chicago").unwrap();
        assert_eq!(t.offset_minutes, -360);

        t.utc = at("2024-07-15T12:00:00Z");
        t.offset_minutes = 0; // desynchronized
        t.refresh_offset().unwrap();
        assert_eq!(t.offset_minutes, -300);
    }

    #[test]
    fn refresh_offset_rejects_unknown_timezone() {
        let mut t = DatumTime::from_utc_and_tz(at("2024-01-15T12:00:00Z"), "UTC").unwrap();
        t.timezone = "Mars/Olympus_Mons".to_string();
        assert!(t.refresh_offset().is_err());
    }
}
