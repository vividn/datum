//! The zone timeline: which IANA zone the user was in, over time.
//!
//! This is the whole of what the database contributes to local→UTC
//! resolution (da-ztae). Everything else — every DST rule, every offset on
//! every day of every year — already lives in `chrono-tz`, so once the zone
//! for an instant is known the conversion is pure in-memory arithmetic.
//!
//! That is why this loads once and there is no batch API: after this query
//! there is no I/O left to amortize.

use chrono::{DateTime, Utc};

use crate::entry::Entry;
use crate::error::Result;
use crate::time::{Zone, ZoneTimeline};

use super::row_mapper::row_to_entry;
use super::Db;

/// The meta fields as `entries_zoned`'s predicate names them
/// (`stats_cache.sql`), which a query must repeat word for word for SQLite
/// to read that index. It names no `setting`: a setting carries no zone.
const NOT_ZONED_META_SQL: &str =
    "field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')";

/// One row per *change* of zone, not one per entry.
///
/// `LAG` does the collapsing in SQLite so the scan returns a handful of
/// rows rather than the whole table. `ZoneTimeline::new` collapses again as
/// a safety net, so an older SQLite that ignored the window function would
/// still yield a correct (merely larger) timeline.
///
/// Rows with a NULL `occur_time_tz` are skipped rather than treated as a
/// zone: a missing name means the zone is *unknown* (da-vfbq), and the bare
/// offset stored beside it changes at every DST transition — reading those
/// as relocations would invent a move twice a year, which is precisely the
/// error a cache keyed on offset changes makes.
///
/// **No outer `ORDER BY occur_time`** — do not add one back (da-uhur). The
/// inner scan already walks `entries_occur_time`, so the handful of rows
/// that survive the filter come out in occur_time order for free, and
/// `ZoneTimeline::new` sorts its observations regardless: the timeline does
/// not depend on this order in the first place. All the outer sort bought
/// was a temp B-tree in the query plan.
///
/// What actually spills here is the window function, whose partition —
/// every live entry — is materialised into an ephemeral table the plan
/// never mentions. That is why this is a tidy-up and `temp_store = MEMORY`
/// (`Db::configure_connection`) is the fix for the browser.
///
/// From `?1` on, plus the newest zoned row before it — the zone in effect
/// there — so a reader of the recent past reads the recent past, not the
/// whole history (da-cwpr1). `''` reaches every row. To `?2` and no
/// further, so a reader of a past day does not read on to now (da-e853a);
/// `'~'`, after every timestamp, reaches the newest.
const ZONE_CHANGES_SQL: &str = "\
    SELECT occur_time, occur_time_tz FROM (
        SELECT occur_time,
               occur_time_tz,
               LAG(occur_time_tz) OVER (ORDER BY occur_time) AS prev_tz
          FROM entries
         WHERE deleted_at IS NULL
           AND superseded_by IS NULL
           AND occur_time >= ?1
           AND occur_time <= ?2
           AND occur_time_tz IS NOT NULL
           AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
    )
    WHERE prev_tz IS NULL OR prev_tz <> occur_time_tz
    UNION ALL
    SELECT * FROM (
        SELECT occur_time, occur_time_tz
          FROM entries
         WHERE deleted_at IS NULL
           AND superseded_by IS NULL
           AND occur_time < ?1
           AND occur_time_tz IS NOT NULL
           AND field NOT IN ('spec', 'layout', 'alias', 'view', 'command', 'index', 'kit')
         ORDER BY occur_time DESC
         LIMIT 1
    )";

impl Db {
    /// Load the [`ZoneTimeline`] this database describes.
    ///
    /// `fallback` answers for a database that records no named zone at all
    /// (a fresh db, or one whose rows all predate zone names) — callers pass
    /// [`Zone::host`], or the zone an explicit `-z` named.
    ///
    /// Zone names the tz database does not recognise are skipped rather than
    /// failing the load: one unparseable row should not make every boundary
    /// in the database unresolvable, and `datum check tz` is where that row
    /// gets reported.
    ///
    /// `since` bounds the read: the timeline is exact from there on, and
    /// before it extends the zone in effect at `since` backwards.
    pub fn zone_timeline(
        &self,
        fallback: Zone,
        since: Option<DateTime<Utc>>,
    ) -> Result<ZoneTimeline> {
        self.zone_timeline_between(fallback, since, None)
    }

    /// [`Self::zone_timeline`] exact up to `until` alone, for a reader of a
    /// window in the past: after it the timeline holds the zone in effect
    /// there, whatever moves came later.
    pub fn zone_timeline_between(
        &self,
        fallback: Zone,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Result<ZoneTimeline> {
        let since = since.map(|t| t.to_rfc3339()).unwrap_or_default();
        let until = until.map_or_else(|| "~".to_string(), |t| t.to_rfc3339());
        let mut stmt = self.conn.prepare(ZONE_CHANGES_SQL)?;
        let rows = stmt.query_map([since, until], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut observations: Vec<(DateTime<Utc>, Zone)> = Vec::new();
        for row in rows {
            let (occur, tz_name) = row?;
            if let (Ok(at), Ok(zone)) = (occur.parse::<DateTime<Utc>>(), Zone::named(&tz_name)) {
                observations.push((at, zone));
            }
        }
        Ok(ZoneTimeline::new(observations, fallback))
    }

    /// The named zones of the [`NEIGHBOURS`] entries nearest before `at`
    /// and of those nearest after it, the rows [`ZONE_CHANGES_SQL`] reads,
    /// for [`agreed_zone`] (da-h8dvi).
    pub fn zones_around(&self, at: DateTime<Utc>) -> Result<(Vec<String>, Vec<String>)> {
        let side = |cmp: &str, order: &str| -> Result<Vec<String>> {
            let sql = format!(
                "SELECT occur_time_tz FROM entries
                  WHERE deleted_at IS NULL
                    AND superseded_by IS NULL
                    AND occur_time {cmp} ?1
                    AND occur_time_tz IS NOT NULL
                    AND {NOT_ZONED_META_SQL}
                  ORDER BY occur_time {order}
                  LIMIT {NEIGHBOURS}"
            );
            let mut stmt = self.conn.prepare_cached(&sql)?;
            let rows = stmt.query_map([at.to_rfc3339()], |row| row.get(0))?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        };
        Ok((side("<", "DESC")?, side(">", "ASC")?))
    }

    /// The rows [`Db::zones_around`] reads, whole, nearest first: what
    /// `datum check tz` shows around a row it names (da-3zq6g).
    pub fn entries_around(&self, at: DateTime<Utc>) -> Result<(Vec<Entry>, Vec<Entry>)> {
        let side = |cmp: &str, order: &str| -> Result<Vec<Entry>> {
            let sql = format!(
                "SELECT * FROM entries
                  WHERE deleted_at IS NULL
                    AND superseded_by IS NULL
                    AND occur_time {cmp} ?1
                    AND occur_time_tz IS NOT NULL
                    AND {NOT_ZONED_META_SQL}
                  ORDER BY occur_time {order}
                  LIMIT {NEIGHBOURS}"
            );
            let mut stmt = self.conn.prepare_cached(&sql)?;
            let rows = stmt.query_map([at.to_rfc3339()], row_to_entry)?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        };
        Ok((side("<", "DESC")?, side(">", "ASC")?))
    }

    /// Every live zoned row's rowid and zone, oldest first, read from the
    /// `entries_zoned` index alone and no row: what `datum check tz` walks
    /// for a zone the rows around one agree is not its own (da-3zq6g).
    pub fn zone_sequence(&self) -> Result<Vec<(i64, String)>> {
        let sql = format!(
            "SELECT rowid, occur_time_tz FROM entries
              WHERE deleted_at IS NULL
                AND superseded_by IS NULL
                AND occur_time_tz IS NOT NULL
                AND {NOT_ZONED_META_SQL}
              ORDER BY occur_time"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// A stretch of [`Db::zone_sequence`]: the `reach` rows nearest before
    /// `at` and the `reach` from it on, oldest first, and how many are
    /// before it. A seek of the index either way (da-3zq6g).
    pub fn zone_window(&self, at: &str, reach: usize) -> Result<(Vec<(i64, String)>, usize)> {
        let side = |cmp: &str, order: &str| -> Result<Vec<(i64, String)>> {
            let sql = format!(
                "SELECT rowid, occur_time_tz FROM entries
                  WHERE deleted_at IS NULL
                    AND superseded_by IS NULL
                    AND occur_time {cmp} ?1
                    AND occur_time_tz IS NOT NULL
                    AND {NOT_ZONED_META_SQL}
                  ORDER BY occur_time {order}
                  LIMIT ?2"
            );
            let mut stmt = self.conn.prepare_cached(&sql)?;
            let rows = stmt.query_map(rusqlite::params![at, reach], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        };
        let mut window = side("<", "DESC")?;
        window.reverse();
        let before = window.len();
        window.extend(side(">=", "ASC")?);
        Ok((window, before))
    }

    /// The rows of `rowids`, in their order.
    pub fn entries_at_rowids(&self, rowids: &[i64]) -> Result<Vec<Entry>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT * FROM entries WHERE rowid = ?1")?;
        rowids
            .iter()
            .map(|id| Ok(stmt.query_row([id], row_to_entry)?))
            .collect()
    }
}

/// How many entries either side of an instant speak for its zone.
pub const NEIGHBOURS: usize = 3;

/// The zone the entries around an instant agree on (da-h8dvi): the one more
/// than half of those `before` it share, when more than half of those
/// `after` it share it too. An empty side agrees on nothing; a side of one
/// is that one.
/// A window and not the one entry beside it, so a lone mis-zoned row is
/// outvoted rather than copied forward; and both sides, so a move — the zone
/// changing and staying changed — names nothing. A real move looks like a
/// step and a mistake like a spike.
pub fn agreed_zone<'a, S: PartialEq>(before: &'a [S], after: &[S]) -> Option<&'a S> {
    fn most<S: PartialEq>(side: &[S]) -> Option<&S> {
        side.iter()
            .find(|z| 2 * side.iter().filter(|o| o == z).count() > side.len())
    }
    let zone = most(before)?;
    (most(after)? == zone).then_some(zone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{entry_builder, fresh_db};
    use crate::time::DatumTime;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse::<DateTime<Utc>>().unwrap()
    }

    fn chicago() -> Zone {
        Zone::named("America/Chicago").unwrap()
    }

    fn berlin() -> Zone {
        Zone::named("Europe/Berlin").unwrap()
    }

    /// Insert an entry stamped with an explicit zone at an explicit instant.
    fn logged_in(db: &Db, at: &str, tz: &str) {
        let utc = utc(at);
        let t = DatumTime::from_utc_and_tz(utc, tz).expect("valid IANA name");
        entry_builder("coffee").time_start(t).insert(db);
    }

    #[test]
    fn empty_db_answers_with_the_fallback() {
        let db = fresh_db();
        let tl = db.zone_timeline(chicago(), None).unwrap();
        assert_eq!(tl.segments().len(), 1);
        assert_eq!(tl.zone_at(utc("2024-06-01T00:00:00Z")), &chicago());
    }

    /// The point of the loader: one segment per *move*, not one per entry.
    #[test]
    fn many_entries_in_one_zone_collapse_to_a_single_segment() {
        let db = fresh_db();
        for day in 1..=5 {
            logged_in(&db, &format!("2024-01-0{day}T12:00:00Z"), "America/Chicago");
        }
        let tl = db.zone_timeline(berlin(), None).unwrap();
        assert_eq!(tl.segments().len(), 1);
        assert_eq!(tl.zone_at(utc("2024-01-03T00:00:00Z")), &chicago());
    }

    #[test]
    fn a_move_becomes_a_segment_boundary() {
        let db = fresh_db();
        logged_in(&db, "2024-01-05T18:00:00Z", "America/Chicago");
        logged_in(&db, "2024-02-05T18:00:00Z", "America/Chicago");
        logged_in(&db, "2024-06-05T10:00:00Z", "Europe/Berlin");
        logged_in(&db, "2024-07-05T10:00:00Z", "Europe/Berlin");

        let tl = db.zone_timeline(chicago(), None).unwrap();
        assert_eq!(tl.segments().len(), 2, "one segment per move");
        assert_eq!(tl.zone_at(utc("2024-03-01T00:00:00Z")), &chicago());
        assert_eq!(tl.zone_at(utc("2024-08-01T00:00:00Z")), &berlin());
        // Everything before the first entry is covered by the first segment.
        assert_eq!(tl.zone_at(utc("1999-01-01T00:00:00Z")), &chicago());
    }

    /// From `since` on, a bounded timeline is the whole one: the zone in
    /// effect at `since` is the newest row's before it, not the fallback.
    #[test]
    fn a_bounded_timeline_starts_with_the_zone_in_effect() {
        let db = fresh_db();
        logged_in(&db, "2024-01-05T18:00:00Z", "Europe/Berlin");
        logged_in(&db, "2024-02-05T18:00:00Z", "America/Chicago");
        logged_in(&db, "2024-06-05T10:00:00Z", "Europe/Berlin");

        let tl = db
            .zone_timeline(berlin(), Some(utc("2024-03-01T00:00:00Z")))
            .unwrap();
        assert_eq!(tl.segments().len(), 2, "Chicago at `since`, then Berlin");
        assert_eq!(tl.zone_at(utc("2024-03-01T00:00:00Z")), &chicago());
        assert_eq!(tl.zone_at(utc("2024-08-01T00:00:00Z")), &berlin());
        let past = db
            .zone_timeline(chicago(), Some(utc("2024-09-01T00:00:00Z")))
            .unwrap();
        assert_eq!(past.zone_at(utc("2024-09-01T00:00:00Z")), &berlin());
        let until = db
            .zone_timeline_between(
                berlin(),
                Some(utc("2024-01-01T00:00:00Z")),
                Some(utc("2024-03-01T00:00:00Z")),
            )
            .unwrap();
        assert_eq!(
            until.segments().len(),
            2,
            "Berlin, then Chicago, and no further"
        );
        assert_eq!(until.zone_at(utc("2024-08-01T00:00:00Z")), &chicago());
    }

    /// A move *back* is a third segment — the timeline is ordered history,
    /// not a set of zones the user has ever been in.
    #[test]
    fn moving_back_reopens_a_segment() {
        let db = fresh_db();
        logged_in(&db, "2024-01-05T18:00:00Z", "America/Chicago");
        logged_in(&db, "2024-06-05T10:00:00Z", "Europe/Berlin");
        logged_in(&db, "2024-09-05T18:00:00Z", "America/Chicago");

        let tl = db.zone_timeline(chicago(), None).unwrap();
        assert_eq!(tl.segments().len(), 3);
        assert_eq!(tl.zone_at(utc("2024-10-01T00:00:00Z")), &chicago());
    }

    /// Rows whose zone is unknown carry only a bare offset, which changes at
    /// every DST transition. Reading those as relocations would invent a
    /// move twice a year, so they contribute nothing.
    #[test]
    fn rows_without_a_zone_name_are_not_moves() {
        let db = fresh_db();
        logged_in(&db, "2024-01-05T18:00:00Z", "America/Chicago");
        entry_builder("coffee")
            .time_start(DatumTime {
                utc: utc("2024-03-05T18:00:00Z"),
                offset_minutes: -360,
                timezone: None,
                uncertainty: None,
            })
            .insert(&db);
        logged_in(&db, "2024-05-05T18:00:00Z", "America/Chicago");

        let tl = db.zone_timeline(berlin(), None).unwrap();
        assert_eq!(tl.segments().len(), 1);
        assert_eq!(tl.zone_at(utc("2024-03-05T18:00:00Z")), &chicago());
    }

    /// The reason the cache exists: after loading, DST for any instant comes
    /// from the tz rules, including instants no entry ever witnessed. Here
    /// the only Chicago entry is in January (CST), yet a July local noon
    /// still resolves at CDT.
    #[test]
    fn timeline_resolves_dst_no_entry_witnessed() {
        let db = fresh_db();
        logged_in(&db, "2024-01-05T18:00:00Z", "America/Chicago");
        let tl = db.zone_timeline(berlin(), None).unwrap();

        let july_noon = "2024-07-15T12:00:00"
            .parse::<chrono::NaiveDateTime>()
            .unwrap();
        assert_eq!(
            tl.resolve_local(july_noon).for_boundary(),
            utc("2024-07-15T17:00:00Z"),
            "July noon in Chicago is CDT (-5), not the January CST (-6)"
        );
    }

    /// The plan stays free of the temp B-tree the outer `ORDER BY` bought
    /// (da-uhur), so that nothing reintroduces the sort by accident.
    #[test]
    fn the_scan_uses_no_temp_b_tree() {
        let db = fresh_db();
        logged_in(&db, "2024-01-05T18:00:00Z", "America/Chicago");
        logged_in(&db, "2024-06-05T10:00:00Z", "Europe/Berlin");

        let sql = format!("EXPLAIN QUERY PLAN {ZONE_CHANGES_SQL}");
        let mut stmt = db.conn.prepare(&sql).unwrap();
        let plan: Vec<String> = stmt
            .query_map(["", "~"], |row| row.get::<_, String>(3))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(
            !plan.iter().any(|step| step.contains("TEMP B-TREE")),
            "the zone scan sorts again: {plan:#?}"
        );
    }

    /// What the comment on `ZONE_CHANGES_SQL` claims: the rows arrive in
    /// occur_time order because of the index, not because of a sort — and
    /// not because of the order they were written in, which here is the
    /// reverse.
    #[test]
    fn the_rows_arrive_in_occur_time_order_unsorted() {
        let db = fresh_db();
        for (at, tz) in [
            ("2024-09-05T18:00:00Z", "Asia/Tokyo"),
            ("2024-06-05T10:00:00Z", "Europe/Berlin"),
            ("2024-01-05T18:00:00Z", "America/Chicago"),
        ] {
            logged_in(&db, at, tz);
        }

        let mut stmt = db.conn.prepare(ZONE_CHANGES_SQL).unwrap();
        let rows: Vec<(String, String)> = stmt
            .query_map(["", "~"], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(
            rows.iter().map(|(_, tz)| tz.as_str()).collect::<Vec<_>>(),
            ["America/Chicago", "Europe/Berlin", "Asia/Tokyo"],
        );
        assert!(rows.windows(2).all(|w| w[0].0 < w[1].0), "{rows:#?}");
    }

    /// And the timeline those rows build — every segment of it, not its
    /// length — on a history with several moves.
    #[test]
    fn several_moves_give_the_whole_timeline() {
        let db = fresh_db();
        for (at, tz) in [
            ("2024-01-05T18:00:00Z", "America/Chicago"),
            ("2024-02-05T18:00:00Z", "America/Chicago"),
            ("2024-06-05T10:00:00Z", "Europe/Berlin"),
            ("2024-07-05T10:00:00Z", "Asia/Tokyo"),
            ("2024-09-05T18:00:00Z", "America/Chicago"),
        ] {
            logged_in(&db, at, tz);
        }

        let tl = db.zone_timeline(berlin(), None).unwrap();
        assert_eq!(
            tl.segments(),
            [
                // The earliest segment reaches back over all prior history.
                (DateTime::<Utc>::MIN_UTC, chicago()),
                (utc("2024-06-05T10:00:00Z"), berlin()),
                (
                    utc("2024-07-05T10:00:00Z"),
                    Zone::named("Asia/Tokyo").unwrap()
                ),
                (utc("2024-09-05T18:00:00Z"), chicago()),
            ]
            .map(|(from, zone)| crate::time::ZoneSegment { from, zone })
        );
    }
}
