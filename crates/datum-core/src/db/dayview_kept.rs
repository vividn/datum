//! The storage of `dayview --digests`' kept days (da-7z5i9): the tables and
//! triggers are `dayview_kept.sql`'s, what a day is kept as is `datum-commands`'
//! `commands/dayview.rs`.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use rusqlite::types::Value;

use crate::dur::{Dur, DurSpan};
use crate::error::Result;

use super::{Db, Seen, WriteBatch};

/// The kept days' tables and triggers (`dayview_kept.sql`).
pub(crate) const DAYVIEW_KEPT_SQL: &str = include_str!("dayview_kept.sql");

/// How many days are kept, the least recently kept dropped.
const KEPT: i64 = 512;

/// How far past a day's end a zone change can move its cut: a move west
/// reads that day's date again for up to a day, and the day's window reads
/// the zones two days on.
const ZONE_REACH_MS: i64 = 2 * 86_400_000;

/// A day's picture as kept: its digest, the end of its window, and each
/// field whose drawing reads past that end, to the last instant it reads.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptDay {
    pub digest: String,
    pub end_ms: i64,
    pub reach: BTreeMap<String, i64>,
}

/// A write not yet accounted for: its field (`''`: a zone change) and the
/// earliest instant it reached, `None` where that does not read (before
/// every day).
type Dirty = (String, Option<i64>);

impl KeptDay {
    /// Whether a write in `dirty` can have changed this day's picture.
    fn reached(&self, dirty: &[Dirty]) -> bool {
        dirty.iter().any(|(field, edge)| {
            let until = match field.is_empty() {
                true => self.end_ms + ZONE_REACH_MS,
                false => self.reach.get(field).copied().unwrap_or(self.end_ms),
            };
            edge.is_none_or(|edge| edge <= until)
        })
    }
}

/// The leading edge of a dirty row: `at` less `dur`. An unknown duration may
/// begin anywhere before it.
fn leading_edge(dur: &str, at: &str) -> Option<i64> {
    let at = at.parse::<DateTime<Utc>>().ok()?.timestamp_millis();
    if dur.is_empty() {
        return Some(at);
    }
    match Dur::parse(dur)?.span {
        // Checked, though no `at` and `dur` that parse reach past i64.
        DurSpan::Of(d) => at.checked_sub(d.num_milliseconds().max(0)),
        DurSpan::Moment => Some(at),
        DurSpan::Unknown => None,
    }
}

impl Db {
    /// The writes the kept days have not accounted for. None where the
    /// tables are not laid (a read-only open of an older file).
    fn dayview_dirty(&self) -> Result<Vec<Dirty>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT field, dur, at FROM dayview_kept_dirty")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|r| {
            let (field, dur, at) = r?;
            Ok((field, leading_edge(&dur, &at)))
        })
        .collect()
    }

    /// Every kept day, by key.
    fn dayview_all_kept(&self) -> Result<HashMap<String, KeptDay>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT key, end_ms, reach, digest FROM dayview_kept")?;
        let rows = stmt.query_map([], |r| {
            let reach: String = r.get(2)?;
            Ok((
                r.get::<_, String>(0)?,
                KeptDay {
                    end_ms: r.get(1)?,
                    reach: serde_json::from_str(&reach).unwrap_or_default(),
                    digest: r.get(3)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The digest of each day of `keys` that is kept and that no write has
    /// reached since. Reads only: a check that finds every day kept writes
    /// nothing, so datum.day hears no change from it.
    pub fn dayview_kept(&self, keys: &[String]) -> Result<HashMap<String, String>> {
        let dirty = self.dayview_dirty()?;
        let mut kept = self.dayview_all_kept()?;
        Ok(keys
            .iter()
            .filter_map(|k| kept.remove_entry(k))
            .filter(|(_, day)| !day.reached(&dirty))
            .map(|(k, day)| (k, day.digest))
            .collect())
    }

    /// Keep `days`, by key, as computed on `seen`: every kept day a write
    /// has reached since is dropped, those writes are accounted for, and the
    /// least recently kept past [`KEPT`] go. Waits for no lock: a day not
    /// kept is drawn again next time. Whether they were kept.
    pub fn dayview_keep(&self, seen: Seen, days: &[(String, KeptDay)]) -> Result<bool> {
        if days.is_empty() || self.read_only() {
            return Ok(false);
        }
        let dirty = self.dayview_dirty()?;
        let mut batch = WriteBatch::on(seen);
        for (key, day) in self.dayview_all_kept()? {
            if day.reached(&dirty) {
                batch.push(
                    "DELETE FROM dayview_kept WHERE key = ?1",
                    vec![Value::Text(key)],
                );
            }
        }
        batch.push("DELETE FROM dayview_kept_dirty", vec![]);
        for (key, day) in days {
            batch.push(
                "INSERT OR REPLACE INTO dayview_kept (key, end_ms, reach, digest)
                 VALUES (?1, ?2, ?3, ?4)",
                vec![
                    Value::Text(key.clone()),
                    Value::Integer(day.end_ms),
                    Value::Text(serde_json::to_string(&day.reach)?),
                    Value::Text(day.digest.clone()),
                ],
            );
        }
        batch.push(
            "DELETE FROM dayview_kept
              WHERE rowid <= (SELECT max(rowid) FROM dayview_kept) - ?1",
            vec![Value::Integer(KEPT)],
        );
        let wait: u64 = self
            .conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))?;
        self.conn.busy_timeout(std::time::Duration::ZERO)?;
        let kept = self.write(&batch);
        self.conn
            .busy_timeout(std::time::Duration::from_millis(wait))?;
        Ok(kept.unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(end_ms: i64, reach: &[(&str, i64)]) -> KeptDay {
        KeptDay {
            digest: String::new(),
            end_ms,
            reach: reach.iter().map(|(f, t)| (f.to_string(), *t)).collect(),
        }
    }

    #[test]
    fn a_write_reaches_a_day_up_to_its_end_and_a_field_up_to_where_it_reads() {
        let d = day(1000, &[("sleep", 5000)]);
        let w = |f: &str, t: Option<i64>| vec![(f.to_string(), t)];
        assert!(d.reached(&w("mood", Some(1000))), "at the day's end");
        assert!(!d.reached(&w("mood", Some(1001))), "after it");
        assert!(
            d.reached(&w("mood", Some(-1_000_000))),
            "a state carried in"
        );
        assert!(d.reached(&w("sleep", Some(5000))), "where its block ends");
        assert!(!d.reached(&w("sleep", Some(5001))));
        assert!(
            d.reached(&w("", Some(1000 + ZONE_REACH_MS))),
            "a zone change"
        );
        assert!(!d.reached(&w("", Some(1001 + ZONE_REACH_MS))));
        assert!(d.reached(&w("mood", None)), "an instant that does not read");
        assert!(!d.reached(&[]));
    }

    #[test]
    fn a_leading_edge_is_the_instant_less_its_duration() {
        let at = "2026-11-02T12:00:00.000Z";
        let t = at.parse::<DateTime<Utc>>().unwrap().timestamp_millis();
        assert_eq!(leading_edge("", at), Some(t));
        assert_eq!(leading_edge("MOMENT", at), Some(t));
        assert_eq!(leading_edge("PT30M", at), Some(t - 1_800_000));
        assert_eq!(leading_edge("UNKNOWN", at), None);
        assert_eq!(leading_edge("PT30M", ""), None);
    }
}
