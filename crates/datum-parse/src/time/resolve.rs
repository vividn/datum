//! The single local-wall-clock → UTC resolution utility (da-ztae).
//!
//! Converting a local time to a UTC instant is not a total function: on the
//! two days a year a zone changes offset, one local time happens twice and
//! another never happens at all. Every hand-rolled conversion in this
//! codebase used to reach for `.single()` and then either bail with a vague
//! "ambiguous local midnight" or `unwrap()` — which is to say, each call
//! site invented its own policy, and several invented the wrong one.
//!
//! This module has three parts:
//!
//! * [`Resolved`] — the *result*, an enum rather than an `Option`, so the
//!   two failure shapes are distinguishable and neither can be dropped
//!   silently. (`TimeArg` was a struct of four `Option`s and `coerce.rs`
//!   quietly ignored one of them; da-svvy. An enum makes that a compile
//!   error, not a bug report.)
//! * [`Resolved::for_boundary`] / [`Resolved::for_logging`] — the two
//!   *policies*, named once here instead of re-decided per call site.
//! * [`ZoneTimeline`] — the cache. The only fact that needs the database is
//!   *which IANA zone the user was in at instant T*, which changes when a
//!   person moves: a handful of segments over a lifetime. Load it once and
//!   every DST computation after that is pure `chrono-tz` arithmetic with no
//!   I/O, so there is nothing left for a batch API to amortize.
//!
//! Note that a cache keyed on "the last observed offset change" would be
//! *wrong* here: DST transitions are rules in the tz database, not rows in
//! ours, and they happen twice a year without any entry marking them. Keying
//! on zone identity instead of on offset changes is both simpler and correct.

use chrono::{DateTime, FixedOffset, NaiveDateTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;

use crate::error::{ParseError, Result};
use crate::time::datum_time::DatumTime;

/// The zone a local time is to be read in: a named IANA zone (whose DST
/// rules `chrono-tz` knows) or a bare fixed offset.
///
/// A fixed offset is what remains when `occur_time_tz` is NULL: per
/// [`DatumTime::timezone`]'s contract, `None` means *unknown*, and an offset
/// with no name is the whole of what we know. A fixed offset has no
/// transitions, so resolution against one is always [`Resolved::Unique`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Zone {
    /// A named IANA zone, with DST rules.
    Iana(Tz),
    /// A bare UTC offset — the zone name is unknown.
    Fixed(FixedOffset),
}

impl Zone {
    /// Parse an IANA zone name. Returns `InvalidTime` if unrecognised.
    pub fn iana(name: &str) -> Result<Self> {
        name.parse()
            .map(Zone::Iana)
            .map_err(|_| ParseError::InvalidTime(format!("Unknown timezone: {name}")))
    }

    /// The zone a [`DatumTime`] carries: its IANA name where it has one,
    /// otherwise the fixed offset stored beside it.
    pub fn of(t: &DatumTime) -> Result<Self> {
        match t.timezone.as_deref() {
            Some(name) => Zone::iana(name),
            None => Ok(Zone::Fixed(*t.local_time().offset())),
        }
    }

    /// The host's zone, resolved the same way [`DatumTime::now`] resolves it
    /// (honouring `TZ`), so a boundary computed here and a timestamp written
    /// there agree.
    #[cfg(feature = "native")]
    pub fn host() -> Self {
        Zone::iana(&crate::time::clock::host_iana_tz_name())
            .unwrap_or(Zone::Fixed(chrono::offset::Utc.fix()))
    }

    /// A name for error messages: the IANA name, or the offset spelled out.
    pub fn name(&self) -> String {
        match self {
            Zone::Iana(tz) => tz.name().to_string(),
            Zone::Fixed(off) => off.to_string(),
        }
    }

    /// The UTC offset this zone produces at a given instant.
    pub fn offset_at(&self, utc: DateTime<Utc>) -> FixedOffset {
        match self {
            Zone::Iana(tz) => utc.with_timezone(tz).offset().fix(),
            Zone::Fixed(off) => *off,
        }
    }

    /// The wall-clock reading this zone shows at `utc`.
    pub fn local_naive(&self, utc: DateTime<Utc>) -> NaiveDateTime {
        match self {
            Zone::Iana(tz) => utc.with_timezone(tz).naive_local(),
            Zone::Fixed(off) => utc.with_timezone(off).naive_local(),
        }
    }

    /// Read `naive` as a wall-clock reading in this zone.
    ///
    /// This is *the* conversion; `clippy.toml` disallows the chrono calls it
    /// wraps so that new call sites cannot quietly re-derive it.
    pub fn resolve(&self, naive: NaiveDateTime) -> Resolved {
        match self {
            Zone::Iana(tz) => resolve_in(naive, tz),
            Zone::Fixed(off) => resolve_in(naive, off),
        }
    }
}

/// The result of reading a local wall-clock time in a zone.
///
/// Modelled on `chrono::LocalResult`, but carrying the one fact chrono
/// throws away — *where* the gap was — so [`Resolved::for_boundary`] can
/// clamp to it rather than guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    /// Exactly one instant has this local reading. The ordinary case.
    Unique(DateTime<Utc>),
    /// The clock read this twice — a fall-back overlap. `first` is the
    /// earlier instant (pre-transition), `second` the later.
    Ambiguous {
        first: DateTime<Utc>,
        second: DateTime<Utc>,
    },
    /// The clock never read this — a spring-forward gap. `transition` is the
    /// instant the offset changed, i.e. the first instant *after* the gap.
    Nonexistent { transition: DateTime<Utc> },
}

impl Resolved {
    /// Policy for **period boundaries** (day/week/month starts, `--since`,
    /// `--until`, day bounds): always yields an instant, never fails.
    ///
    /// * `Ambiguous` picks the **first** — clean boundaries, no missed data.
    ///   Taking the later instant would drop the hour that the clock repeated.
    /// * `Nonexistent` clamps to the **transition instant**. Together these
    ///   mean consecutive boundaries always abut: no interval is skipped and
    ///   none is double-counted.
    pub fn for_boundary(self) -> DateTime<Utc> {
        match self {
            Resolved::Unique(t) => t,
            Resolved::Ambiguous { first, .. } => first,
            Resolved::Nonexistent { transition } => transition,
        }
    }

    /// Policy for a **time the user typed** (logging an entry, `--time`,
    /// `--date`, `retime`): refuses rather than silently moving it.
    ///
    /// A user who types a local time that does not exist has made an error,
    /// and shifting it by an hour behind their back hides that. An ambiguous
    /// time is a genuine question only they can answer — a `-z` / offset
    /// spelled out on the command line resolves it.
    pub fn for_logging(self) -> Result<DateTime<Utc>> {
        match self {
            Resolved::Unique(t) => Ok(t),
            Resolved::Ambiguous { first, second } => Err(ParseError::InvalidTime(format!(
                "Ambiguous local time: the clock read this twice \
                 ({first} and {second} UTC). Disambiguate with an explicit offset."
            ))),
            Resolved::Nonexistent { transition } => Err(ParseError::InvalidTime(format!(
                "Nonexistent local time: the clock skipped it at the \
                 transition {transition} UTC (daylight-saving gap)."
            ))),
        }
    }

    /// The instant, when there is exactly one. `None` for both the ambiguous
    /// and the nonexistent case — prefer [`for_boundary`] or [`for_logging`],
    /// which say which of those two you meant to happen.
    ///
    /// [`for_boundary`]: Self::for_boundary
    /// [`for_logging`]: Self::for_logging
    pub fn single(self) -> Option<DateTime<Utc>> {
        match self {
            Resolved::Unique(t) => Some(t),
            _ => None,
        }
    }
}

/// Read `naive` as wall-clock in `tz`, recovering the transition instant on
/// the gap branch (which `chrono::LocalResult::None` discards).
#[allow(clippy::disallowed_methods)] // this *is* the sanctioned wrapper
fn resolve_in<Z: TimeZone>(naive: NaiveDateTime, tz: &Z) -> Resolved {
    use chrono::offset::LocalResult;
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Resolved::Unique(dt.with_timezone(&Utc)),
        // Ordered by *instant*, not by tuple position: `chrono_tz::Tz` and
        // `chrono::Local` order the pair oppositely, so keying on position
        // would pick the wrong side on one of the two paths.
        LocalResult::Ambiguous(a, b) => {
            let (a, b) = (a.with_timezone(&Utc), b.with_timezone(&Utc));
            Resolved::Ambiguous {
                first: a.min(b),
                second: a.max(b),
            }
        }
        LocalResult::None => Resolved::Nonexistent {
            transition: gap_transition(naive, tz),
        },
    }
}

/// Locate the offset transition that swallowed `naive`.
///
/// `naive` is unrepresentable in `tz`, so no exact arithmetic recovers the
/// transition; but it must lie within one maximal UTC offset (±26h) of
/// `naive` read as though it were UTC. Bisect that window for the first
/// instant carrying the post-gap offset. ~17 iterations to the second, all
/// in memory.
fn gap_transition<Z: TimeZone>(naive: NaiveDateTime, tz: &Z) -> DateTime<Utc> {
    let anchor = naive.and_utc();
    let offset_at = |secs: i64| {
        let t = DateTime::from_timestamp(secs, 0).unwrap_or(anchor);
        tz.offset_from_utc_datetime(&t.naive_utc()).fix()
    };

    // Bisect over whole seconds: tz transitions land on second boundaries,
    // and integer bounds keep the answer free of sub-second residue.
    let mut lo = anchor.timestamp() - 30 * 3600;
    let mut hi = anchor.timestamp() + 30 * 3600;
    let before = offset_at(lo);
    if offset_at(hi) == before {
        // No transition in the window — unreachable for a real gap. Fall back
        // to reading `naive` against the surrounding offset rather than
        // panicking on a time the user may have typed.
        return anchor - before;
    }
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if offset_at(mid) == before {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    DateTime::from_timestamp(hi, 0).unwrap_or(anchor)
}

// ─── Zone timeline ──────────────────────────────────────────────────────────

/// One `[from, …)` stretch of history during which a single zone applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneSegment {
    /// First instant covered. The *earliest* segment also covers everything
    /// before it: history had to start somewhere, and the alternative is
    /// having no answer for instants predating the first record.
    pub from: DateTime<Utc>,
    pub zone: Zone,
}

/// Which zone the user was in, over time — the whole of what the database
/// contributes to local→UTC resolution.
///
/// Build it once per process (see `Db::zone_timeline`); afterwards every
/// lookup is a binary search over a handful of segments and every DST
/// computation is pure `chrono-tz` arithmetic. That is why there is no
/// batch API: the expensive part has already been removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneTimeline {
    /// Ascending by `from`, with adjacent same-zone runs collapsed. Never
    /// empty — [`ZoneTimeline::new`] falls back to a single segment.
    segments: Vec<ZoneSegment>,
}

impl ZoneTimeline {
    /// A timeline with no changes: one zone for all of history. This is the
    /// right construction for a database whose rows carry no zone, and for
    /// callers that already know the zone (an explicit `-z`).
    pub fn single(zone: Zone) -> Self {
        ZoneTimeline {
            segments: vec![ZoneSegment {
                from: DateTime::<Utc>::MIN_UTC,
                zone,
            }],
        }
    }

    /// Build from observations of "zone `z` was in effect at instant `t`",
    /// in any order. Adjacent runs of the same zone collapse, so feeding one
    /// observation per entry row yields a segment only where the zone
    /// actually changed. `fallback` is used when there are no observations.
    pub fn new(
        observations: impl IntoIterator<Item = (DateTime<Utc>, Zone)>,
        fallback: Zone,
    ) -> Self {
        let mut obs: Vec<(DateTime<Utc>, Zone)> = observations.into_iter().collect();
        if obs.is_empty() {
            return ZoneTimeline::single(fallback);
        }
        obs.sort_by_key(|(t, _)| *t);

        let mut segments: Vec<ZoneSegment> = Vec::new();
        for (from, zone) in obs {
            match segments.last() {
                Some(last) if last.zone == zone => {}
                _ => segments.push(ZoneSegment { from, zone }),
            }
        }
        // The earliest segment extends backwards over all prior history.
        segments[0].from = DateTime::<Utc>::MIN_UTC;
        ZoneTimeline { segments }
    }

    pub fn segments(&self) -> &[ZoneSegment] {
        &self.segments
    }

    /// The zone in effect at `instant`.
    pub fn zone_at(&self, instant: DateTime<Utc>) -> &Zone {
        let idx = match self.segments.binary_search_by(|s| s.from.cmp(&instant)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };
        &self.segments[idx].zone
    }

    /// Read `naive` as a wall-clock reading in whichever zone was in effect
    /// *at the instant it denotes*.
    ///
    /// That is mildly circular — the zone determines the instant — so each
    /// segment is tried from the most recent backwards and the first whose
    /// own range contains the resulting instant wins. Where no segment
    /// agrees (the local time falls in the seam of a move), the earliest
    /// segment answers, so a reading always resolves.
    pub fn resolve_local(&self, naive: NaiveDateTime) -> Resolved {
        for (i, seg) in self.segments.iter().enumerate().rev() {
            let resolved = seg.zone.resolve(naive);
            let instant = resolved.for_boundary();
            let below_next = match self.segments.get(i + 1) {
                Some(next) => instant < next.from,
                None => true,
            };
            if instant >= seg.from && below_next {
                return resolved;
            }
        }
        self.segments[0].zone.resolve(naive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse::<DateTime<Utc>>().unwrap()
    }

    fn naive(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    fn chicago() -> Zone {
        Zone::iana("America/Chicago").unwrap()
    }

    #[test]
    fn ordinary_local_time_is_unique() {
        assert_eq!(
            chicago().resolve(naive("2024-06-15T09:15:00")),
            Resolved::Unique(utc("2024-06-15T14:15:00Z"))
        );
    }

    /// Fall-back: 2024-11-03 01:30 happens twice in Chicago, once at CDT
    /// (-5) and again an hour later at CST (-6). `first` is the genuinely
    /// earlier *instant*, regardless of chrono's tuple order.
    #[test]
    fn fall_back_overlap_is_ambiguous_ordered_by_instant() {
        assert_eq!(
            chicago().resolve(naive("2024-11-03T01:30:00")),
            Resolved::Ambiguous {
                first: utc("2024-11-03T06:30:00Z"),
                second: utc("2024-11-03T07:30:00Z"),
            }
        );
    }

    /// Spring-forward: 02:30 never happens on 2024-03-10. chrono reports
    /// only "None"; we recover the instant the clock jumped (02:00 CST).
    #[test]
    fn spring_forward_gap_reports_the_transition_instant() {
        assert_eq!(
            chicago().resolve(naive("2024-03-10T02:30:00")),
            Resolved::Nonexistent {
                transition: utc("2024-03-10T08:00:00Z")
            }
        );
    }

    #[test]
    fn gap_transition_is_found_from_anywhere_inside_the_gap() {
        for t in ["02:00:00", "02:00:01", "02:30:00", "02:59:59"] {
            assert_eq!(
                chicago().resolve(naive(&format!("2024-03-10T{t}"))),
                Resolved::Nonexistent {
                    transition: utc("2024-03-10T08:00:00Z")
                },
                "gap probe at {t}"
            );
        }
    }

    /// A half-hour zone, to prove the bisection isn't hour-aligned by luck.
    #[test]
    fn gap_transition_in_a_half_hour_zone() {
        let lord_howe = Zone::iana("Australia/Lord_Howe").unwrap();
        // 2024-10-06 02:00 +10:30 → 02:30 +11:00; 02:15 is inside the gap.
        assert_eq!(
            lord_howe.resolve(naive("2024-10-06T02:15:00")),
            Resolved::Nonexistent {
                transition: utc("2024-10-05T15:30:00Z")
            }
        );
    }

    /// A fixed offset has no rules and therefore no transitions: every local
    /// reading resolves uniquely. This is the `occur_time_tz IS NULL` path.
    #[test]
    fn fixed_offset_is_always_unique() {
        let z = Zone::Fixed(FixedOffset::east_opt(330 * 60).unwrap());
        assert_eq!(
            z.resolve(naive("2024-03-10T02:30:00")),
            Resolved::Unique(utc("2024-03-09T21:00:00Z"))
        );
    }

    #[test]
    fn zone_of_datum_time_prefers_the_name_and_falls_back_to_the_offset() {
        let named = DatumTime::from_utc_and_tz(utc("2024-06-15T12:00:00Z"), "America/Chicago");
        assert_eq!(Zone::of(&named.unwrap()).unwrap(), chicago());

        let unnamed = DatumTime {
            utc: utc("2024-06-15T12:00:00Z"),
            offset_minutes: 330,
            timezone: None,
        };
        assert_eq!(
            Zone::of(&unnamed).unwrap(),
            Zone::Fixed(FixedOffset::east_opt(330 * 60).unwrap())
        );
    }

    // ─── policies ───────────────────────────────────────────────────────────

    #[test]
    fn boundary_policy_takes_the_first_of_an_overlap() {
        assert_eq!(
            chicago()
                .resolve(naive("2024-11-03T01:30:00"))
                .for_boundary(),
            utc("2024-11-03T06:30:00Z")
        );
    }

    #[test]
    fn boundary_policy_clamps_a_gap_to_the_transition() {
        assert_eq!(
            chicago()
                .resolve(naive("2024-03-10T02:30:00"))
                .for_boundary(),
            utc("2024-03-10T08:00:00Z")
        );
    }

    /// The point of both boundary rules: consecutive local hour boundaries
    /// stay contiguous across a spring-forward, so no interval is skipped
    /// and none is double-counted.
    #[test]
    fn boundary_policy_keeps_the_day_contiguous_across_a_gap() {
        let z = chicago();
        let bounds: Vec<DateTime<Utc>> = (0..5)
            .map(|h| {
                z.resolve(naive(&format!("2024-03-10T0{h}:00:00")))
                    .for_boundary()
            })
            .collect();
        assert!(
            bounds.windows(2).all(|w| w[0] <= w[1]),
            "boundaries must not go backwards: {bounds:?}"
        );
        // 00:00 → 06:00Z, 01:00 → 07:00Z, 02:00 (gap) → 08:00Z, 03:00 → 08:00Z.
        assert_eq!(bounds.first().unwrap(), &utc("2024-03-10T06:00:00Z"));
        assert_eq!(bounds[2], utc("2024-03-10T08:00:00Z"));
        assert_eq!(bounds[3], utc("2024-03-10T08:00:00Z"));
        // The whole day is covered: first boundary to last, no hole.
        assert_eq!(
            *bounds.last().unwrap() - *bounds.first().unwrap(),
            Duration::hours(3)
        );
    }

    #[test]
    fn logging_policy_refuses_rather_than_guessing() {
        assert!(chicago()
            .resolve(naive("2024-11-03T01:30:00"))
            .for_logging()
            .is_err());
        assert!(chicago()
            .resolve(naive("2024-03-10T02:30:00"))
            .for_logging()
            .is_err());
        assert_eq!(
            chicago()
                .resolve(naive("2024-06-15T09:15:00"))
                .for_logging()
                .unwrap(),
            utc("2024-06-15T14:15:00Z")
        );
    }

    // ─── zone timeline ──────────────────────────────────────────────────────

    #[test]
    fn single_zone_timeline_answers_for_all_of_history() {
        let tl = ZoneTimeline::single(chicago());
        assert_eq!(tl.zone_at(utc("1970-01-01T00:00:00Z")), &chicago());
        assert_eq!(tl.zone_at(utc("2099-01-01T00:00:00Z")), &chicago());
    }

    #[test]
    fn timeline_collapses_runs_and_extends_the_first_segment_backwards() {
        let berlin = Zone::iana("Europe/Berlin").unwrap();
        let tl = ZoneTimeline::new(
            [
                (utc("2024-01-01T12:00:00Z"), chicago()),
                (utc("2024-02-01T12:00:00Z"), chicago()),
                (utc("2024-06-01T12:00:00Z"), berlin.clone()),
                (utc("2024-07-01T12:00:00Z"), berlin.clone()),
            ],
            chicago(),
        );
        assert_eq!(tl.segments().len(), 2, "same-zone runs collapse");
        assert_eq!(tl.segments()[0].from, DateTime::<Utc>::MIN_UTC);
        assert_eq!(tl.zone_at(utc("1999-01-01T00:00:00Z")), &chicago());
        assert_eq!(tl.zone_at(utc("2024-05-01T00:00:00Z")), &chicago());
        assert_eq!(tl.zone_at(utc("2024-06-15T00:00:00Z")), &berlin);
    }

    #[test]
    fn timeline_observations_need_not_be_ordered() {
        let berlin = Zone::iana("Europe/Berlin").unwrap();
        let tl = ZoneTimeline::new(
            [
                (utc("2024-06-01T12:00:00Z"), berlin.clone()),
                (utc("2024-01-01T12:00:00Z"), chicago()),
            ],
            chicago(),
        );
        assert_eq!(tl.zone_at(utc("2024-03-01T00:00:00Z")), &chicago());
        assert_eq!(tl.zone_at(utc("2024-08-01T00:00:00Z")), &berlin);
    }

    #[test]
    fn empty_timeline_uses_the_fallback_zone() {
        let tl = ZoneTimeline::new([], chicago());
        assert_eq!(tl.segments().len(), 1);
        assert_eq!(tl.zone_at(utc("2024-06-01T00:00:00Z")), &chicago());
    }

    /// The point of the whole cache: DST is computed from the *rules*, not
    /// sampled from recorded offsets. Here the only observation is in
    /// January (CST, -6), yet a July local time still resolves at CDT (-5),
    /// and a boundary that has not happened yet resolves too.
    #[test]
    fn timeline_computes_dst_for_instants_no_entry_witnessed() {
        let tl = ZoneTimeline::new([(utc("2024-01-05T18:00:00Z"), chicago())], chicago());
        assert_eq!(
            tl.resolve_local(naive("2024-07-15T00:00:00")),
            Resolved::Unique(utc("2024-07-15T05:00:00Z"))
        );
        assert_eq!(
            tl.resolve_local(naive("2024-11-03T01:30:00")),
            Resolved::Ambiguous {
                first: utc("2024-11-03T06:30:00Z"),
                second: utc("2024-11-03T07:30:00Z"),
            }
        );
    }

    /// A move: the same local reading means different instants on either
    /// side of the relocation, and each is resolved in its own zone.
    #[test]
    fn timeline_resolves_each_local_time_in_the_zone_then_in_effect() {
        let berlin = Zone::iana("Europe/Berlin").unwrap();
        let tl = ZoneTimeline::new(
            [
                (utc("2024-01-01T12:00:00Z"), chicago()),
                (utc("2024-06-01T12:00:00Z"), berlin),
            ],
            chicago(),
        );
        // Before the move: Chicago, CST (-6).
        assert_eq!(
            tl.resolve_local(naive("2024-02-10T09:00:00")),
            Resolved::Unique(utc("2024-02-10T15:00:00Z"))
        );
        // After the move: Berlin, CEST (+2).
        assert_eq!(
            tl.resolve_local(naive("2024-08-10T09:00:00")),
            Resolved::Unique(utc("2024-08-10T07:00:00Z"))
        );
    }
}
