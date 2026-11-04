//! The `_N` and `_+N` shorthands a listing labels its rows with, without a
//! database.
//!
//! `datum tail` reads them from SQLite: [`quick_ordinals`] runs one query a
//! side of now over every live entry, nearest first, and hands back hid →
//! `_N` / `_+N`. The
//! browser has the same rows in front of it and no such query, so da-6ec2
//! factored the two halves apart — the ORDER is here, pure, and both the
//! CLI's `build_table_contexts` and the wasm tail attach the result the
//! same way.
//!
//! [`quick_ordinals`]: datum_core::ids::quick_ordinals

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use datum_core::entry::Entry;
use datum_core::ids::{Hid, QuickRef, QUICK_ORDINAL_CAP};

use super::RenderContext;

/// hid → `_N` for the [`QUICK_ORDINAL_CAP`] newest of `entries` at or
/// before `now`, and `_+N` for as many of the nearest after it (da-xxmhn).
///
/// The order is `quick_ordinals`', spelled in Rust: by the listing's time
/// — a timeless row's is its create time (da-2vgj1) — with the slug
/// breaking a tie, the past newest first and the future nearest first.
/// `entries` may arrive in any order; a listing renders oldest-first and
/// still counts from now.
///
/// Past the cap a row has no short form. It is left out of the map rather
/// than given a number, because a `_100` the resolver would refuse is
/// worse than no shorthand at all.
pub fn quick_ordinal_map(entries: &[Entry], now: DateTime<Utc>) -> HashMap<Hid, QuickRef> {
    let key = |e: &Entry| {
        let time = e.at.as_ref().map_or(e.create_time, |t| t.utc);
        (time, e.slug.0.clone())
    };
    let mut order: Vec<_> = entries.iter().map(|e| (key(e), e)).collect();
    order.sort_by(|a, b| a.0.cmp(&b.0));
    let split = order.partition_point(|(k, _)| k.0 <= now);
    let (past, future) = order.split_at(split);
    let past = past.iter().rev().take(QUICK_ORDINAL_CAP).enumerate();
    let past = past.map(|(i, (_, e))| {
        (
            e.hid.clone(),
            QuickRef {
                future: false,
                n: i + 1,
            },
        )
    });
    let future = future.iter().take(QUICK_ORDINAL_CAP).enumerate();
    let future = future.map(|(i, (_, e))| {
        (
            e.hid.clone(),
            QuickRef {
                future: true,
                n: i + 1,
            },
        )
    });
    past.chain(future).collect()
}

/// Hang each row's ordinal on its [`RenderContext`], by hid.
///
/// `ctxs` and `entries` are the same listing in the same order — one
/// context per entry, as every table caller builds them. A row the map
/// does not name keeps `None`, which is what drops the `_n` column when
/// no row in the listing has a shorthand.
pub fn attach_quick_ordinals(
    ctxs: &mut [RenderContext],
    entries: &[Entry],
    ordinals: &HashMap<Hid, QuickRef>,
) {
    for (ctx, e) in ctxs.iter_mut().zip(entries) {
        ctx.quick_ordinal = ordinals.get(&e.hid).copied();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::entry::Verb;
    use datum_core::time::DatumTime;

    fn at(iso: &str) -> DatumTime {
        DatumTime::from_utc_and_tz(iso.parse().unwrap(), "UTC").unwrap()
    }

    fn row(field: &str, iso: &str) -> Entry {
        Entry::new(field, Verb::Point, None, None, Some(at(iso)), None, "d1")
    }

    fn past(n: usize) -> QuickRef {
        QuickRef { future: false, n }
    }

    /// After every row the tests below make, so each is in the past.
    fn now() -> DateTime<Utc> {
        "2026-06-01T00:00:00Z".parse().unwrap()
    }

    /// `_1` is the newest row, whatever order the listing arrived in —
    /// a tail renders oldest-first and still counts from the bottom.
    #[test]
    fn one_is_the_newest_row_however_the_listing_is_ordered() {
        let oldest = row("a", "2026-05-01T10:00:00Z");
        let middle = row("b", "2026-05-01T11:00:00Z");
        let newest = row("c", "2026-05-01T12:00:00Z");
        let listing = vec![oldest.clone(), middle.clone(), newest.clone()];
        let map = quick_ordinal_map(&listing, now());
        assert_eq!(map.get(&newest.hid), Some(&past(1)));
        assert_eq!(map.get(&middle.hid), Some(&past(2)));
        assert_eq!(map.get(&oldest.hid), Some(&past(3)));

        let reversed: Vec<Entry> = listing.iter().rev().cloned().collect();
        assert_eq!(quick_ordinal_map(&reversed, now()), map);
    }

    /// A timeless row counts at its create time, where the listing puts it
    /// (da-2vgj1): created after every dated row, it is `_1`.
    #[test]
    fn a_timeless_row_counts_at_its_create_time() {
        let dated = row("a", "2026-05-01T10:00:00Z");
        let mut timeless = row("b", "2026-05-01T09:00:00Z");
        timeless.at = None;
        timeless.create_time = "2026-05-01T11:00:00Z".parse().unwrap();
        let map = quick_ordinal_map(&[timeless.clone(), dated.clone()], now());
        assert_eq!(map.get(&timeless.hid), Some(&past(1)));
        assert_eq!(map.get(&dated.hid), Some(&past(2)));
    }

    /// Past the cap there is no shorthand — absent, not guessed. The
    /// `--json` surface reports the same absence as `null`
    /// (`tail_json.rs`: "past the ordinal cap there is no shorthand").
    #[test]
    fn past_the_cap_a_row_has_no_shorthand() {
        let base: chrono::DateTime<chrono::Utc> = "2026-05-01T00:00:00Z".parse().unwrap();
        let rows: Vec<Entry> = (0..QUICK_ORDINAL_CAP + 3)
            .map(|i| {
                let mut e = row("f", "2026-05-01T00:00:00Z");
                e.at = Some(
                    DatumTime::from_utc_and_tz(base + chrono::Duration::minutes(i as i64), "UTC")
                        .unwrap(),
                );
                e
            })
            .collect();
        let map = quick_ordinal_map(&rows, now());
        assert_eq!(map.len(), QUICK_ORDINAL_CAP);
        // The three oldest are the three past the cap.
        for e in &rows[..3] {
            assert_eq!(map.get(&e.hid), None);
        }
    }

    /// Rows after now are `_+N`, nearest first, and `_N` counts the rest
    /// from now back (da-xxmhn); a row at now itself is past.
    #[test]
    fn rows_after_now_count_forward() {
        let past_row = row("a", "2026-05-31T23:00:00Z");
        let at_now = row("b", "2026-06-01T00:00:00Z");
        let near = row("c", "2026-06-01T01:00:00Z");
        let far = row("d", "2026-06-01T02:00:00Z");
        let listing = [far.clone(), past_row.clone(), near.clone(), at_now.clone()];
        let map = quick_ordinal_map(&listing, now());
        assert_eq!(map.get(&at_now.hid), Some(&past(1)));
        assert_eq!(map.get(&past_row.hid), Some(&past(2)));
        assert_eq!(map.get(&near.hid), Some(&QuickRef { future: true, n: 1 }));
        assert_eq!(map.get(&far.hid), Some(&QuickRef { future: true, n: 2 }));
    }

    /// The attach step is keyed by hid, so a context lands on its own row
    /// even where the listing and the ordinal universe differ.
    #[test]
    fn contexts_take_the_ordinal_of_the_row_they_sit_beside() {
        let a = row("a", "2026-05-01T10:00:00Z");
        let b = row("b", "2026-05-01T11:00:00Z");
        let listing = vec![a.clone(), b.clone()];
        let mut ctxs = vec![RenderContext::default(), RenderContext::default()];
        attach_quick_ordinals(&mut ctxs, &listing, &quick_ordinal_map(&listing, now()));
        assert_eq!(ctxs[0].quick_ordinal, Some(past(2)));
        assert_eq!(ctxs[1].quick_ordinal, Some(past(1)));
    }
}
