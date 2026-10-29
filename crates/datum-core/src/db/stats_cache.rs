//! The storage of `stats`' per-day cache (da-n7tp2): the tables and
//! triggers are `stats_cache.sql`'s, what a day holds and when it is filled is
//! `datum-commands`' `commands/stats/cache.rs`. Days are `YYYY-MM-DD` in
//! the database's own zone, which compare as text.

use rusqlite::types::Value;
use rusqlite::{params, OptionalExtension};

use crate::error::{DatumError, Result};

use super::{Db, WriteBatch};

/// The cache's tables, triggers and index (`stats_cache.sql`).
pub(crate) const STATS_CACHE_SQL: &str = include_str!("stats_cache.sql");

/// One state label of a cached day, or of a sum of them.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedState {
    pub state: String,
    pub count: usize,
    pub seconds: i64,
    /// The classes the label was read with: 1 tracked, 2 closed, 4 untracked.
    pub class: u8,
}

/// One cached day: its states, and its first and last counted entry
/// (RFC 3339 in their own zones) when it counted one.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedDay {
    pub day: String,
    pub states: Vec<CachedState>,
    pub span: Option<(String, String)>,
}

/// A run of cached days summed: each state's, and the first counted entry
/// of the first day that counted one with the last of the last.
pub type CachedSum = (Vec<CachedState>, Option<(String, String)>);

/// What a refill writes: `field`'s days from `from` on (all of them for
/// `None`) replaced by `days`, and the field cached from `lo` (`''`: its
/// first day) up to `through`; with `zone`, every field's days from that
/// day on forgotten (`''`: all of them) — a zone change it read.
pub struct Refill {
    pub zone: Option<String>,
    pub from: Option<String>,
    pub days: Vec<CachedDay>,
    pub lo: String,
    pub through: String,
}

/// A write the cache has not accounted for yet: `(field, dur, at)`, as
/// `stats_cache.sql` describes `stats_cache_dirty`.
pub type Dirty = (String, String, String);

/// The statements that record `refill` for `field`.
fn refill_batch(batch: &mut WriteBatch, field: &str, stamp: &str, refill: &Refill) {
    let text = |s: &str| Value::Text(s.to_string());
    batch.push(
        "INSERT OR REPLACE INTO _meta (key, value) VALUES ('stats_cache', ?1)",
        vec![text(stamp)],
    );
    if let Some(day) = &refill.zone {
        batch.push(
            "UPDATE stats_cache_through SET day = min(day, ?1)",
            vec![text(day)],
        );
    }
    let from = text(refill.from.as_deref().unwrap_or_default());
    batch.push(
        "DELETE FROM stats_cache WHERE field = ?1 AND day >= ?2",
        vec![text(field), from.clone()],
    );
    batch.push(
        "DELETE FROM stats_cache_days WHERE field = ?1 AND day >= ?2",
        vec![text(field), from],
    );
    for d in &refill.days {
        for s in &d.states {
            batch.push(
                "INSERT INTO stats_cache (field, day, state, count, seconds, class)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                vec![
                    text(field),
                    text(&d.day),
                    text(&s.state),
                    Value::Integer(s.count as i64),
                    Value::Integer(s.seconds),
                    Value::Integer(s.class.into()),
                ],
            );
        }
        if let Some((first, last)) = &d.span {
            batch.push(
                "INSERT INTO stats_cache_days (field, day, first, last) VALUES (?1, ?2, ?3, ?4)",
                vec![text(field), text(&d.day), text(first), text(last)],
            );
        }
    }
    batch.push(
        "INSERT OR REPLACE INTO stats_cache_through (field, lo, day) VALUES (?1, ?2, ?3)",
        vec![text(field), text(&refill.lo), text(&refill.through)],
    );
    // Exactly the rows `fill` was handed: the batch lands only on the
    // database it read.
    batch.push(
        "DELETE FROM stats_cache_dirty WHERE field IN (?1, '')",
        vec![text(field)],
    );
}

impl Db {
    /// The writes the cache has not accounted for yet, after making it this
    /// datum's: a cache stamped otherwise is wiped. Writes nothing else —
    /// datum.day hears a write as a change to re-read, so the stamp itself
    /// goes in with the first refill.
    pub fn stats_cache_open(&self, stamp: &str) -> Result<Vec<Dirty>> {
        let held: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM _meta WHERE key = 'stats_cache'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if held.is_some_and(|held| held != stamp) {
            let mut wipe = WriteBatch::default();
            for table in [
                "stats_cache",
                "stats_cache_days",
                "stats_cache_through",
                "stats_cache_dirty",
            ] {
                wipe.push(format!("DELETE FROM {table}"), vec![]);
            }
            wipe.push("DELETE FROM _meta WHERE key = 'stats_cache'", vec![]);
            self.write(&wipe)?;
        }
        Ok(self
            .conn
            .prepare("SELECT field, dur, at FROM stats_cache_dirty")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// The days of `field` that are cached: `(lo, day)`, from `lo` (`''`:
    /// its first) up to `day`.
    pub fn stats_cache_through(&self, field: &str) -> Result<Option<(String, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT lo, day FROM stats_cache_through WHERE field = ?1",
                params![field],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Refill `field`: `fill` is handed the days the field has cached and
    /// the writes not accounted for, its own and every field's (`''`), and
    /// answers with the [`Refill`], or `None` when nothing needs one. It
    /// reads and computes on a view, before the write lock (da-8i7f0); a
    /// write landing meanwhile has it compute again, so what it records
    /// fresh is what it read stale. A refill accounts for every write of
    /// the field.
    pub fn stats_cache_fill<E: From<DatumError>>(
        &self,
        field: &str,
        stamp: &str,
        mut fill: impl FnMut(
            Option<(String, String)>,
            Vec<Dirty>,
        ) -> std::result::Result<Option<Refill>, E>,
    ) -> std::result::Result<(), E> {
        self.compute_then_write(|view| {
            let dirty = (view
                .conn()
                .prepare("SELECT field, dur, at FROM stats_cache_dirty WHERE field IN (?1, '')"))
            .and_then(|mut s| {
                s.query_map(params![field], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                    .collect::<rusqlite::Result<_>>()
            })
            .map_err(DatumError::from)?;
            let mut batch = WriteBatch::on(view.seen());
            if let Some(refill) = fill(self.stats_cache_through(field)?, dirty)? {
                refill_batch(&mut batch, field, stamp, &refill);
            }
            Ok((batch, ()))
        })
    }

    /// `field`'s cached days in `[from, until)` (from the first for
    /// `None`) summed per state, with the first counted entry of the
    /// first day that counted one and the last of the last.
    pub fn stats_cache_sum(
        &self,
        field: &str,
        from: Option<&str>,
        until: &str,
    ) -> Result<CachedSum> {
        let from = from.unwrap_or_default();
        let states = self
            .conn
            .prepare_cached(
                "SELECT state, sum(count), sum(seconds),
                        max(class & 1) | max(class & 2) | max(class & 4)
                   FROM stats_cache WHERE field = ?1 AND day >= ?2 AND day < ?3
                  GROUP BY state ORDER BY state",
            )?
            .query_map(params![field, from, until], |r| {
                Ok(CachedState {
                    state: r.get(0)?,
                    count: r.get(1)?,
                    seconds: r.get(2)?,
                    class: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let edge = |order: &str, col: &str| -> Result<Option<String>> {
            Ok(self
                .conn
                .prepare_cached(&format!(
                    "SELECT {col} FROM stats_cache_days
                      WHERE field = ?1 AND day >= ?2 AND day < ?3 ORDER BY day {order} LIMIT 1"
                ))?
                .query_row(params![field, from, until], |r| r.get(0))
                .optional()?)
        };
        let span = edge("ASC", "first")?.zip(edge("DESC", "last")?);
        Ok((states, span))
    }

    /// `n` cached days, `(field, day)`, or every one where fewer, picked at
    /// random among the days that held something: a cached field at random,
    /// a date at random in its cached days, and the first day from it that
    /// held something and is not yet taken, each a seek, so the sample costs
    /// its size and not the cache's (da-3zq6g).
    pub fn stats_cache_sample(&self, n: usize) -> Result<Vec<(String, String)>> {
        // Each field's cached days: its first that held something, and the
        // day they run up to.
        let mut fields: Vec<(String, String, String)> = Vec::new();
        let mut spans = self.conn.prepare(
            "SELECT t.field, (SELECT min(day) FROM stats_cache c
                               WHERE c.field = t.field AND c.day >= t.lo), t.day
               FROM stats_cache_through t",
        )?;
        for row in spans.query_map([], |r| {
            Ok((r.get(0)?, r.get::<_, Option<String>>(1)?, r.get(2)?))
        })? {
            if let (field, Some(first), until) = row? {
                if first < until {
                    fields.push((field, first, until));
                }
            }
        }
        let mut from_on = self.conn.prepare_cached(
            "SELECT day FROM stats_cache WHERE field = ?1 AND day >= ?2 AND day < ?3
              ORDER BY day LIMIT 1",
        )?;
        let mut days = std::collections::BTreeSet::new();
        while days.len() < n && !fields.is_empty() {
            let r: i64 = self.conn.query_row("SELECT random()", [], |r| r.get(0))?;
            let r = r.unsigned_abs() as usize;
            let i = r % fields.len();
            let (field, first, until) = &fields[i];
            let date = |d: &str| d.parse::<chrono::NaiveDate>().ok();
            let span = date(first)
                .zip(date(until))
                .map_or(1, |(a, b)| (b - a).num_days().max(1));
            let mut from = (date(first).unwrap_or_default()
                + chrono::Duration::days((r / fields.len()) as i64 % span))
            .to_string();
            let mut wrapped = false;
            loop {
                let day: Option<String> = from_on
                    .query_row(params![field, from, until], |r| r.get(0))
                    .optional()?;
                match day {
                    // Taken: the next after it. `~` sorts after every digit.
                    Some(day) if days.contains(&(field.clone(), day.clone())) => {
                        from = format!("{day}~")
                    }
                    Some(day) => {
                        days.insert((field.clone(), day));
                        break;
                    }
                    None if !wrapped => {
                        from = first.clone();
                        wrapped = true;
                    }
                    // Every day of it taken.
                    None => {
                        fields.remove(i);
                        break;
                    }
                }
            }
        }
        Ok(days.into_iter().collect())
    }

    /// Stop trusting `field`'s days from `day` on: the next read fills them.
    pub fn stats_cache_forget(&self, field: &str, day: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE stats_cache_through SET day = min(day, ?2) WHERE field = ?1",
            params![field, day],
        )?;
        Ok(())
    }
}
