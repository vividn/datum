//! `stats` per closed day (da-n7tp2). vividn: «could there be some sort of
//! index that is kept that aggregates results without having to load the
//! entire history?». A window's whole closed days are read from the cache
//! — each one the rollup of that day alone, filled by ONE read of the
//! field's timeline cut at midnights — and only its partial edges and
//! today are computed live, by the code that computes an uncached window.
//!
//! It stays true by the triggers in `datum-core`'s `db/stats_cache.sql` —
//! every write leaves the first day it can have changed, which the next read
//! refills from — and a stamp: this module's [`VERSION`], the host zone the
//! zone timeline falls back to and the schema version. `-z` reads around it — its days are another zone's.
//! `datum check stats` recomputes a sample of cached days from the entries.

use datum_core::db::{CachedDay, CachedState, Dirty, Refill};
use datum_core::migrations::CURRENT_SCHEMA_VERSION;
use datum_parse::Dur;

use super::*;

/// Bump when a cached day would read differently: what a [`Tally`] holds,
/// how a span or a row is counted, or how days are cut.
const VERSION: u32 = 2;

/// The cache, ready to read for one run.
pub(super) struct Cache<'a> {
    db: &'a Db,
    /// Today in the database's zone: the first day never cached.
    today: NaiveDate,
    /// The database's zones, exact from a few days back.
    zones: ZoneTimeline,
    /// The first day each field's writes not accounted for reach (`None`:
    /// all of them), as of opening.
    dirty: BTreeMap<String, Option<NaiveDate>>,
    /// The first day a zone change reaches in every field, when one is
    /// not accounted for yet: folded into the next refill.
    zone: Option<Option<NaiveDate>>,
}

impl<'a> Cache<'a> {
    /// The cache for this run, or `None` when it reads around it.
    pub(super) fn open(db: &'a Db, args: &StatsArgs, zones: &ZoneTimeline) -> Result<Option<Self>> {
        if args.no_cache || args.time.timezone.is_some() {
            return Ok(None);
        }
        Ok(Some(Cache::new(db, zones)?))
    }

    /// `zones` must be exact from a few days back, as `resolve_window`'s are.
    fn new(db: &'a Db, zones: &ZoneTimeline) -> Result<Self> {
        let today = local_date_at(zones, DatumTime::now().utc);
        let (mut dirty, mut zone) = (BTreeMap::new(), None);
        for row in db.stats_cache_open(&stamp())? {
            let day = reach(zones, &row);
            let slot = match row.0.is_empty() {
                true => zone.get_or_insert(day),
                false => dirty.entry(row.0).or_insert(day),
            };
            *slot = earlier(*slot, day);
        }
        Ok(Cache {
            db,
            today,
            zones: zones.clone(),
            dirty,
            zone,
        })
    }

    /// The tally of `[start, end)`: the cached days wholly inside it, and
    /// the stretches before and after them computed live. `None` when no
    /// closed day lies wholly inside it.
    pub(super) fn tally(
        &self,
        field: &str,
        start: Option<DateTime<Utc>>,
        end: DateTime<Utc>,
        zones: &ZoneTimeline,
    ) -> Result<Option<Tally>> {
        let first = first_day(start, zones)?;
        let last = local_date_at(zones, end.min(midnight(zones, self.today)));
        if first.is_some_and(|f| f >= last) {
            return Ok(None);
        }
        self.fill(field, first)?;
        let (lo, hi) = (first.map(|d| midnight(zones, d)), midnight(zones, last));

        let mut tally = Tally::default();
        if let (Some(s), Some(lo)) = (start, lo) {
            if s < lo {
                tally.add(&load_timeline(field, self.db, start, lo)?, start, lo);
            }
        }
        let day = |d: NaiveDate| d.to_string();
        let (states, span) =
            self.db
                .stats_cache_sum(field, first.map(day).as_deref(), &day(last))?;
        for s in states {
            if s.seconds > 0 {
                *tally.dur.entry(s.state.clone()).or_default() += s.seconds;
            }
            if s.count > 0 {
                *tally.count.entry(s.state.clone()).or_default() += s.count;
            }
            for (bit, class) in CLASSES {
                if s.class & bit != 0 {
                    note_class(&mut tally.class, &s.state, class);
                }
            }
        }
        if let Some((first, last)) = span {
            tally.earliest.get_or_insert(parse(&first)?);
            tally.latest = Some(parse(&last)?);
        }
        if hi < end {
            tally.add(
                &load_timeline(field, self.db, Some(hi), end)?,
                Some(hi),
                end,
            );
        }
        // A day is cached with its own readings only: the window's reading
        // of the labels outside it is today's, as it is uncached.
        let outside = seen_outside(self.db, &timeline_params(field, start, end))?;
        for (state, class, _) in outside {
            note_class(&mut tally.class, &state, class);
        }
        Ok(Some(tally))
    }

    /// Cache `field`'s days from `want` (`None`: its first) up to today: one
    /// read of its timeline, cut at each midnight. Writes nothing when those
    /// days are cached and no write reached them: a write today leaves the
    /// cache as it is.
    fn fill(&self, field: &str, want: Option<NaiveDate>) -> Result<()> {
        let held = self.db.stats_cache_through(field)?;
        let reached = self.dirty.get(field).copied().into_iter().chain(self.zone);
        if plan(held, reached, want, self.today).is_none() {
            return Ok(());
        }
        self.db
            .stats_cache_fill(field, &stamp(), |held, dirty| -> Result<Option<Refill>> {
                let reached = dirty.iter().map(|row| reach(&self.zones, row));
                // Every field's: a zone change this refill accounts for.
                let zone = (dirty.iter().filter(|row| row.0.is_empty()))
                    .map(|row| reach(&self.zones, row))
                    .reduce(earlier);
                let Some((from, lo)) = plan(held, reached, want, self.today) else {
                    return Ok(None);
                };
                let since =
                    from.map(|d| d.and_time(chrono::NaiveTime::MIN).and_utc() - Duration::days(4));
                let zones = self.db.zone_timeline(Zone::host(), since)?;
                let until = midnight(&zones, self.today);
                let timeline =
                    load_timeline(field, self.db, from.map(|d| midnight(&zones, d)), until)?;
                let begins = (timeline.spans.first().map(|s| s.from))
                    .into_iter()
                    .chain(timeline.rows.first().map(|r| r.occur))
                    .min();
                // A field with no closed day has nothing to cache: `stats
                // typo`, or a field first logged today, keeps no watermark.
                let Some(first) = from.or(begins.map(|t| local_date_at(&zones, t))) else {
                    return Ok(None);
                };
                let dates: Vec<NaiveDate> =
                    first.iter_days().take_while(|d| *d < self.today).collect();
                if dates.is_empty() {
                    return Ok(None);
                }
                let edges: Vec<DateTime<Utc>> = (dates.iter().chain([&self.today]))
                    .map(|d| midnight(&zones, *d))
                    .collect();
                let days = (dates.iter().zip(cut(&timeline, &edges)))
                    .map(|(d, t)| cached_day(d.to_string(), t))
                    .collect();
                let day = |d: Option<NaiveDate>| d.map(|d| d.to_string());
                Ok(Some(Refill {
                    zone: zone.map(|d| day(d).unwrap_or_default()),
                    // Replacing from `lo` itself is the whole field anew.
                    from: day(from).filter(|_| from != lo),
                    days,
                    lo: day(lo).unwrap_or_default(),
                    through: self.today.to_string(),
                }))
            })
    }
}

/// The first whole day at or after `start` (`None`: the field's first).
fn first_day(start: Option<DateTime<Utc>>, zones: &ZoneTimeline) -> Result<Option<NaiveDate>> {
    let Some(start) = start else {
        return Ok(None);
    };
    let d = local_date_at(zones, start);
    Ok(Some(match midnight(zones, d) < start {
        true => d.succ_opt().context("date overflow")?,
        false => d,
    }))
}

/// The refill that makes `[want, today)` cached, given the days `held` —
/// `(lo, day)` — and the first days writes `reached` since: `(from, lo)`,
/// read from `from` and cached from `lo` on (`None`: the field's first), or
/// `None` when they are cached already. A cache that reaches back far
/// enough is topped up from where it stops; one that does not is read anew
/// from `want`.
fn plan(
    held: Option<(String, String)>,
    reached: impl Iterator<Item = Option<NaiveDate>>,
    want: Option<NaiveDate>,
    today: NaiveDate,
) -> Option<(Option<NaiveDate>, Option<NaiveDate>)> {
    let anew = Some((want, want));
    let Some((lo, day)) = held else {
        return anew;
    };
    let lo = lo.parse::<NaiveDate>().ok();
    // A write that reached the field's first day leaves nothing cached.
    let Some(hi) = reached.fold(day.parse::<NaiveDate>().ok(), earlier) else {
        return anew;
    };
    let reaches = lo.is_none_or(|lo| want.is_some_and(|w| lo <= w));
    match lo.is_none_or(|lo| hi > lo) && reaches {
        true if hi >= today => None,
        true => Some((Some(hi), lo)),
        false => anew,
    }
}

/// The earlier of two first days, where `None` is the field's first.
fn earlier(a: Option<NaiveDate>, b: Option<NaiveDate>) -> Option<NaiveDate> {
    a.zip(b).map(|(a, b)| a.min(b))
}

/// The first day a write not accounted for can have changed: a zone
/// change's (field `''`), in any field, is two days before its UTC date — a
/// local midnight is never further — and a field's own is [`dirty_day`].
fn reach(zones: &ZoneTimeline, (field, dur, at): &Dirty) -> Option<NaiveDate> {
    match field.is_empty() {
        true => Some(at.parse::<DateTime<Utc>>().ok()?.date_naive() - Duration::days(2)),
        false => dirty_day(zones, dur, at),
    }
}

/// The first day a write of `at` and `dur` can have changed: its leading
/// edge's, in the database's zone — a block resumes the state before it, so
/// everything after, not only up to the next change. `None`, the field's
/// first, for a spec of the field (`at` is `''`).
///
/// Exact for a recent write, in `zones` — a write today must not refill
/// yesterday. An older one takes the day before its UTC date, which no zone
/// is further from, rather than read the zone history from it to now.
/// `zones` is today's timeline, so a zone change inside the exact window
/// would cut a recent day by the new zone; the zone change's own row
/// ([`reach`], two days back) covers that day, which holds only while this
/// window is no wider than that margin.
fn dirty_day(zones: &ZoneTimeline, dur: &str, at: &str) -> Option<NaiveDate> {
    let at = at.parse::<DateTime<Utc>>().ok()?;
    let secs = Dur::parse(dur).and_then(|d| d.as_seconds()).unwrap_or(0);
    let edge = at - Duration::seconds(secs);
    match edge >= DatumTime::now().utc - Duration::days(2) {
        true => Some(local_date_at(zones, edge)),
        false => edge.date_naive().pred_opt(),
    }
}

/// What a cache must carry to be this datum's.
fn stamp() -> String {
    format!("{VERSION} {} {CURRENT_SCHEMA_VERSION}", Zone::host().name())
}

/// How many cached days `datum check stats` recomputes.
const SAMPLE: usize = 32;

/// `datum check stats`: recompute a random sample of cached days from the
/// entries, each over that day alone: how many it recomputed, and every
/// `(field, day)` whose cached reading differs. With `fix`, a differing day
/// and those after it are dropped from the cache, so the next `stats` fills
/// them again.
pub fn check_sample(db: &Db, fix: bool) -> Result<(usize, Vec<(String, String)>)> {
    let zones = db.zone_timeline(Zone::host(), Some(DatumTime::now().utc - Duration::days(4)))?;
    let cache = Cache::new(db, &zones)?;
    let mut differ = Vec::new();
    let sample = db.stats_cache_sample(SAMPLE)?;
    let recomputed = sample.len();
    for (field, day) in sample {
        let date: NaiveDate = day.parse().context("stats cache: bad day")?;
        // A day a write has reached since is filled again first.
        cache.fill(&field, Some(date))?;
        let next = date.succ_opt().context("date overflow")?;
        let since = date.and_time(chrono::NaiveTime::MIN).and_utc() - Duration::days(4);
        let zones = db.zone_timeline(Zone::host(), Some(since))?;
        let edges = [midnight(&zones, date), midnight(&zones, next)];
        let timeline = load_timeline(&field, db, Some(edges[0]), edges[1])?;
        let fresh = cut(&timeline, &edges)
            .pop()
            .map(|t| cached_day(day.clone(), t));
        let (states, span) = db.stats_cache_sum(&field, Some(&day), &next.to_string())?;
        if fresh
            != Some(CachedDay {
                day: day.clone(),
                states,
                span,
            })
        {
            if fix {
                db.stats_cache_forget(&field, &day)?;
            }
            differ.push((field, day));
        }
    }
    differ.sort();
    Ok((recomputed, differ))
}

/// A class and the bit the cache stores it as.
const CLASSES: [(u8, StateClass); 3] = [
    (1, StateClass::Tracked),
    (2, StateClass::Closed),
    (4, StateClass::Untracked),
];

fn parse(t: &str) -> Result<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(t).with_context(|| format!("stats cache: bad instant {t}"))
}

/// The timeline's tally of each day `[edges[i], edges[i + 1])`, as
/// [`Tally::add`] would add that day alone, in one pass: a span is cut at
/// the midnights it crosses, a row falls in its own day.
fn cut(timeline: &Timeline, edges: &[DateTime<Utc>]) -> Vec<Tally> {
    let n = edges.len() - 1;
    let mut days: Vec<Tally> = (0..n).map(|_| Tally::default()).collect();
    let day_of = |t: DateTime<Utc>| edges.partition_point(|m| *m <= t).checked_sub(1);
    let now = DatumTime::now().utc;
    for (i, span) in timeline.spans.iter().enumerate() {
        let Some(state) = &span.state else { continue };
        let to = timeline.spans.get(i + 1).map_or(now, |next| next.from);
        let mut k = day_of(span.from).unwrap_or(0);
        // At least the day the span starts in, as `Tally::add` notes every
        // span's class; then each day it overlaps. A span starting in the
        // future has `to` (now) before `from`.
        while k < n && edges[k] < to.max(span.from + Duration::nanoseconds(1)) {
            let day = &mut days[k];
            add_overlap(
                &mut day.dur,
                state,
                span.from,
                to,
                Some(edges[k]),
                edges[k + 1],
            );
            note_class(&mut day.class, state, span.class);
            k += 1;
        }
    }
    for row in &timeline.rows {
        let Some(day) = day_of(row.occur).and_then(|k| days.get_mut(k)) else {
            continue;
        };
        note_class(&mut day.class, &row.state, row.class);
        *day.count.entry(row.state.clone()).or_default() += 1;
        day.earliest.get_or_insert(row.local);
        day.latest = Some(row.local);
    }
    days
}

fn cached_day(day: String, t: Tally) -> CachedDay {
    let states = (t.class.iter())
        .map(|(state, class)| CachedState {
            state: state.clone(),
            count: t.count.get(state).copied().unwrap_or(0),
            seconds: t.dur.get(state).copied().unwrap_or(0),
            class: CLASSES
                .iter()
                .find(|(_, c)| c == class)
                .map_or(0, |(b, _)| *b),
        })
        .collect();
    let span = t.earliest.zip(t.latest);
    CachedDay {
        day,
        states,
        span: span.map(|(a, b)| (a.to_rfc3339(), b.to_rfc3339())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_tops_up_what_reaches_back_and_reads_anew_what_does_not() {
        let d = |s: &str| s.parse::<NaiveDate>().ok();
        let held = |lo: &str, day: &str| Some((lo.to_string(), day.to_string()));
        let today = d("2026-06-15").unwrap();
        let plan = |held, reached: Vec<Option<NaiveDate>>, want| {
            plan(held, reached.into_iter(), want, today)
        };
        let june = d("2026-06-01");
        // Nothing cached yet: read from what is wanted.
        assert_eq!(plan(None, vec![], june), Some((june, june)));
        // Cached through today, and a write today: nothing to do.
        assert_eq!(
            plan(held("", "2026-06-15"), vec![d("2026-06-15")], None),
            None
        );
        // A write on the 10th: top up from it, keeping the first day.
        let tenth = d("2026-06-10");
        assert_eq!(
            plan(held("", "2026-06-15"), vec![tenth], None),
            Some((tenth, None))
        );
        // A spec of the field reaches its first day: anew.
        assert_eq!(
            plan(held("", "2026-06-15"), vec![None], june),
            Some((june, june))
        );
        // Cached from the 5th, wanted from the 1st: anew from the 1st.
        assert_eq!(
            plan(held("2026-06-05", "2026-06-15"), vec![], june),
            Some((june, june))
        );
        // Cached from the 5th, but a write lowered its end below that: anew.
        let fifth = d("2026-06-05");
        assert_eq!(
            plan(held("2026-06-05", "2026-06-03"), vec![], fifth),
            Some((fifth, fifth))
        );
    }
}
