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

use crate::error::Result;
use crate::time::{Zone, ZoneTimeline};

use super::Db;

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
const ZONE_CHANGES_SQL: &str = "\
    SELECT occur_time, occur_time_tz FROM (
        SELECT occur_time,
               occur_time_tz,
               LAG(occur_time_tz) OVER (ORDER BY occur_time) AS prev_tz
          FROM entries
         WHERE deleted_at IS NULL
           AND superseded_by IS NULL
           AND occur_time IS NOT NULL
           AND occur_time_tz IS NOT NULL
           AND field NOT IN ('spec', 'layout')
    )
    WHERE prev_tz IS NULL OR prev_tz <> occur_time_tz";

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
    pub fn zone_timeline(&self, fallback: Zone) -> Result<ZoneTimeline> {
        let mut stmt = self.conn.prepare(ZONE_CHANGES_SQL)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut observations: Vec<(DateTime<Utc>, Zone)> = Vec::new();
        for row in rows {
            let (occur, tz_name) = row?;
            if let (Ok(at), Ok(zone)) = (occur.parse::<DateTime<Utc>>(), Zone::iana(&tz_name)) {
                observations.push((at, zone));
            }
        }
        Ok(ZoneTimeline::new(observations, fallback))
    }
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
        Zone::iana("America/Chicago").unwrap()
    }

    fn berlin() -> Zone {
        Zone::iana("Europe/Berlin").unwrap()
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
        let tl = db.zone_timeline(chicago()).unwrap();
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
        let tl = db.zone_timeline(berlin()).unwrap();
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

        let tl = db.zone_timeline(chicago()).unwrap();
        assert_eq!(tl.segments().len(), 2, "one segment per move");
        assert_eq!(tl.zone_at(utc("2024-03-01T00:00:00Z")), &chicago());
        assert_eq!(tl.zone_at(utc("2024-08-01T00:00:00Z")), &berlin());
        // Everything before the first entry is covered by the first segment.
        assert_eq!(tl.zone_at(utc("1999-01-01T00:00:00Z")), &chicago());
    }

    /// A move *back* is a third segment — the timeline is ordered history,
    /// not a set of zones the user has ever been in.
    #[test]
    fn moving_back_reopens_a_segment() {
        let db = fresh_db();
        logged_in(&db, "2024-01-05T18:00:00Z", "America/Chicago");
        logged_in(&db, "2024-06-05T10:00:00Z", "Europe/Berlin");
        logged_in(&db, "2024-09-05T18:00:00Z", "America/Chicago");

        let tl = db.zone_timeline(chicago()).unwrap();
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

        let tl = db.zone_timeline(berlin()).unwrap();
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
        let tl = db.zone_timeline(berlin()).unwrap();

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
            .query_map([], |row| row.get::<_, String>(3))
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
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
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

        let tl = db.zone_timeline(berlin()).unwrap();
        assert_eq!(
            tl.segments(),
            [
                // The earliest segment reaches back over all prior history.
                (DateTime::<Utc>::MIN_UTC, chicago()),
                (utc("2024-06-05T10:00:00Z"), berlin()),
                (
                    utc("2024-07-05T10:00:00Z"),
                    Zone::iana("Asia/Tokyo").unwrap()
                ),
                (utc("2024-09-05T18:00:00Z"), chicago()),
            ]
            .map(|(from, zone)| crate::time::ZoneSegment { from, zone })
        );
    }
}
