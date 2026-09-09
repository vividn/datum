//! The `_N` shorthands a listing labels its rows with, without a database.
//!
//! `datum tail` reads them from SQLite: [`quick_ordinals`] runs one query
//! over every live entry, newest first, and hands back hid → `_N`. The
//! browser has the same rows in front of it and no such query, so da-6ec2
//! factored the two halves apart — the ORDER is here, pure, and both the
//! CLI's `build_table_contexts` and the wasm tail attach the result the
//! same way.
//!
//! [`quick_ordinals`]: datum_core::ids::quick_ordinals

use std::collections::HashMap;

use datum_core::{entry::Entry, ids::Hid, ids::QUICK_ORDINAL_CAP};

use super::RenderContext;

/// hid → `_N` for the [`QUICK_ORDINAL_CAP`] newest of `entries`.
///
/// The order is `quick_ordinals`' `ORDER BY occur_time DESC, slug DESC`,
/// spelled in Rust: newest occur time first, ties broken by the slug
/// descending, and — as SQLite orders a descending column — rows with no
/// occur time last. `entries` may arrive in any order; a listing renders
/// oldest-first and still counts from its newest row.
///
/// Past the cap a row has no short form. It is left out of the map rather
/// than given a number, because a `_100` the resolver would refuse is
/// worse than no shorthand at all.
pub fn quick_ordinal_map(entries: &[Entry]) -> HashMap<Hid, usize> {
    let mut order: Vec<&Entry> = entries.iter().collect();
    order.sort_by(|a, b| {
        let key = |e: &Entry| (e.occur_time.as_ref().map(|t| t.utc), e.slug.0.clone());
        // Descending on both, and `None` sorts below every instant here —
        // which puts the timeless rows last, where the SQL leaves them.
        key(b).cmp(&key(a))
    });
    order
        .into_iter()
        .take(QUICK_ORDINAL_CAP)
        .enumerate()
        .map(|(i, e)| (e.hid.clone(), i + 1))
        .collect()
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
    ordinals: &HashMap<Hid, usize>,
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

    /// `_1` is the newest row, whatever order the listing arrived in —
    /// a tail renders oldest-first and still counts from the bottom.
    #[test]
    fn one_is_the_newest_row_however_the_listing_is_ordered() {
        let oldest = row("a", "2026-05-01T10:00:00Z");
        let middle = row("b", "2026-05-01T11:00:00Z");
        let newest = row("c", "2026-05-01T12:00:00Z");
        let listing = vec![oldest.clone(), middle.clone(), newest.clone()];
        let map = quick_ordinal_map(&listing);
        assert_eq!(map.get(&newest.hid), Some(&1));
        assert_eq!(map.get(&middle.hid), Some(&2));
        assert_eq!(map.get(&oldest.hid), Some(&3));

        let reversed: Vec<Entry> = listing.iter().rev().cloned().collect();
        assert_eq!(quick_ordinal_map(&reversed), map);
    }

    /// A timeless row has no occur time to be newest by; SQLite puts it
    /// last under a descending column and so does this.
    #[test]
    fn a_timeless_row_counts_last() {
        let dated = row("a", "2026-05-01T10:00:00Z");
        let mut timeless = row("b", "2026-05-01T11:00:00Z");
        timeless.occur_time = None;
        let map = quick_ordinal_map(&[timeless.clone(), dated.clone()]);
        assert_eq!(map.get(&dated.hid), Some(&1));
        assert_eq!(map.get(&timeless.hid), Some(&2));
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
                e.occur_time = Some(
                    DatumTime::from_utc_and_tz(base + chrono::Duration::minutes(i as i64), "UTC")
                        .unwrap(),
                );
                e
            })
            .collect();
        let map = quick_ordinal_map(&rows);
        assert_eq!(map.len(), QUICK_ORDINAL_CAP);
        // The three oldest are the three past the cap.
        for e in &rows[..3] {
            assert_eq!(map.get(&e.hid), None);
        }
    }

    /// The attach step is keyed by hid, so a context lands on its own row
    /// even where the listing and the ordinal universe differ.
    #[test]
    fn contexts_take_the_ordinal_of_the_row_they_sit_beside() {
        let a = row("a", "2026-05-01T10:00:00Z");
        let b = row("b", "2026-05-01T11:00:00Z");
        let listing = vec![a.clone(), b.clone()];
        let mut ctxs = vec![RenderContext::default(), RenderContext::default()];
        attach_quick_ordinals(&mut ctxs, &listing, &quick_ordinal_map(&listing));
        assert_eq!(ctxs[0].quick_ordinal, Some(2));
        assert_eq!(ctxs[1].quick_ordinal, Some(1));
    }
}
