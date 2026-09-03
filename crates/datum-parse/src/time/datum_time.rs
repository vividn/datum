//! `DatumTime` — the canonical timestamp stored on every entry.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::error::{ParseError, Result};

/// A timestamp with full timezone metadata, as stored in the database.
///
/// `utc`, `offset_minutes` and `timezone` are **not independent**:
/// `offset_minutes` is the signed UTC offset (in minutes) of `utc` resolved
/// at `timezone`. Construct via
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
    /// IANA timezone name (e.g. "America/Chicago"), or `None` when the zone
    /// is unknown.
    ///
    /// `None` means *unknown*, which is not the same claim as `"UTC"`: a
    /// legacy-datum backup may carry an offset with no `tz` key, and naming
    /// a zone we were not given both invents history and contradicts the
    /// offset (see da-vfbq). An offset is self-verifying; a zone name is a
    /// claim that requires a lookup. With no name, `offset_minutes` is the
    /// whole of what we know, and consumers that need a zone fall back to
    /// that fixed offset.
    pub timezone: Option<String>,
    /// How precisely this instant is known, as the ISO-8601 duration of the
    /// ± radius (`"PT5M"` for `±5m`, `"P1D"` for a date-only entry). Written
    /// by `TimeBuilder::build_with_uncertainty` from a `-q` stack or an
    /// explicit `±N`, and stored in `entries.occur_time_uncertainty`.
    ///
    /// `None` means *unknown* — we were not told how precisely the time is
    /// known — which is **not** the claim "this time is exact". Same rule as
    /// `timezone` above, for the same reason: naming a precision we were not
    /// given invents history. Renderers that need a floor may *choose* to
    /// treat unknown as second-exact, but they choose it locally and say so;
    /// the field itself makes no such claim.
    ///
    /// Deliberately **absent from the `content_hash` preimage** — see the
    /// note on `datum_core::entry::compute_hash`.
    ///
    /// [`Uncertainty::to_iso`]: crate::time::Uncertainty::to_iso
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncertainty: Option<String>,
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
            timezone: Some(timezone.to_string()),
            uncertainty: None,
        })
    }

    /// The UTC offset (in minutes) that `timezone` actually produces at
    /// `utc` — i.e. what `offset_minutes` *should* equal for the struct to
    /// be in sync. `Ok(None)` when no zone is stored: there is no claim to
    /// reconcile the offset against. Returns `InvalidTime` if `timezone`
    /// isn't a recognised IANA name. Does not mutate; [`refresh_offset`] is
    /// the in-place form, and `datum check tz` compares this against the
    /// stored value to flag already-desynced rows.
    ///
    /// [`refresh_offset`]: Self::refresh_offset
    pub fn expected_offset_minutes(&self) -> Result<Option<i32>> {
        let Some(name) = self.timezone.as_deref() else {
            return Ok(None);
        };
        let tz: Tz = name
            .parse()
            .map_err(|_| ParseError::InvalidTime(format!("Unknown timezone: {name}")))?;
        let offset_secs = self
            .utc
            .with_timezone(&tz)
            .fixed_offset()
            .offset()
            .local_minus_utc();
        Ok(Some(offset_secs / 60))
    }

    /// Re-derive `offset_minutes` from `utc` resolved at `timezone`.
    ///
    /// Call after directly mutating `utc` or `timezone`, or after shifting
    /// across a DST boundary, to restore the struct's invariant. A no-op
    /// when no zone is stored — the offset is then the only zone information
    /// there is, so there is nothing to re-derive it from. Returns
    /// `InvalidTime` if `timezone` isn't a recognised IANA name.
    pub fn refresh_offset(&mut self) -> Result<()> {
        if let Some(expected) = self.expected_offset_minutes()? {
            self.offset_minutes = expected;
        }
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
        t.timezone = Some("Mars/Olympus_Mons".to_string());
        assert!(t.refresh_offset().is_err());
    }

    /// A zone-less time is not a claim about a zone, so there is nothing for
    /// the offset to disagree with — and nothing to re-derive it from. The
    /// offset survives `refresh_offset` untouched (da-vfbq).
    #[test]
    fn absent_timezone_has_no_expected_offset() {
        let mut t = DatumTime {
            utc: at("2024-01-15T12:00:00Z"),
            offset_minutes: 330, // UTC+05:30, zone unknown
            timezone: None,
            uncertainty: None,
        };
        assert_eq!(t.expected_offset_minutes().unwrap(), None);
        t.refresh_offset().unwrap();
        assert_eq!(t.offset_minutes, 330);
        assert_eq!(t.local_time().to_rfc3339(), "2024-01-15T17:30:00+05:30");
    }
}
