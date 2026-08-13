//! Bulk retime paths.
//!
//! Post-pivot the entry's time-shape is just `occur_time` + `dur` — no
//! end_time to slide. These methods retime `occur_time` and leave `dur`
//! untouched.
//!
//! da-5ns.4: when the row's `id_structure` references `%occur_time%`, a
//! retime cascades into the slug. Each row goes through
//! [`Db::save_existing_entry_with_auto_reinterp`] so a templated slug is
//! re-derived against the new occur_time and the row is rekeyed in place.
//! Rows without a templated slug (or whose template doesn't reference
//! occur_time) stay put — the helper's slug-changed check is the gate.

use chrono::Duration;

use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::field_spec::DEFAULT_ID_DELIMITER;
use crate::time::Uncertainty;

use super::row_mapper::row_to_entry;
use super::types::RetimeFilter;
use super::Db;

impl Db {
    /// Retime matching rows to a fresh absolute `occur_time` + precision.
    ///
    /// The precision is rendered into a single ISO-8601 duration string
    /// (e.g. `PT1S`, `PT1M`, `P1D`) via [`Uncertainty::to_iso_precision`]
    /// and stored in `occur_time_precision` — the v=20 schema collapsed
    /// the (precision_seconds, precision_unit) pair into one column.
    ///
    /// da-sunq: the whole batch runs in one transaction — a mid-batch
    /// failure (rekey collision, disk error) rolls back every
    /// already-shifted row, so a partial shift is never visible.
    pub fn set_start_time_with_precision(
        &self,
        filter: &RetimeFilter,
        new_start: &crate::time::DatumTime,
        precision: &Uncertainty,
    ) -> Result<usize> {
        let entries = select_retime_targets(self, filter)?;
        let precision_iso = precision.to_iso_precision();
        let tx = self.conn.unchecked_transaction()?;
        let mut count = 0usize;
        for mut e in entries {
            let old_slug = e.slug.clone();
            e.occur_time = Some(new_start.clone());
            e.occur_time_precision = precision_iso.clone();
            self.save_existing_entry_with_auto_reinterp_kind_in_tx(
                &old_slug,
                e,
                DEFAULT_ID_DELIMITER,
                crate::history::ChangeKind::Retime,
            )?;
            count += 1;
        }
        tx.commit()?;
        Ok(count)
    }

    /// Shift matching rows' `occur_time` by `offset`. Atomic over the
    /// batch, same as [`Self::set_start_time_with_precision`] (da-sunq).
    pub fn update_times(&self, filter: &RetimeFilter, offset: Duration) -> Result<usize> {
        let entries = select_retime_targets(self, filter)?;
        let tx = self.conn.unchecked_transaction()?;
        let mut count = 0usize;
        for e in entries {
            let old_slug = e.slug.clone();
            let mut shifted = e;
            if let Some(t) = shifted.occur_time.as_mut() {
                t.utc += offset;
                // A raw `utc` mutation desyncs `offset_minutes` (see the
                // DatumTime doc-comment); re-derive it so a cross-DST shift
                // renders the intended wall-clock and rekeys %occur_time%
                // slugs correctly. Propagates on unknown-tz.
                t.refresh_offset()?;
            }
            self.save_existing_entry_with_auto_reinterp_kind_in_tx(
                &old_slug,
                shifted,
                DEFAULT_ID_DELIMITER,
                crate::history::ChangeKind::Retime,
            )?;
            count += 1;
        }
        tx.commit()?;
        Ok(count)
    }
}

/// Select live rows matching the retime filter. Shared between the
/// absolute (`set_start_time_with_precision`) and offset (`update_times`)
/// paths so the SQL surface stays in one place.
///
/// Timeless rows (`occur_time IS NULL`, slug `field:c<create_time>`) are
/// excluded: retime moves an existing time, and giving a timeless row one
/// would contradict the `occur_time IS NULL` marker the state/timeline views
/// key on (da-r17h). Use `datum update` to attach a time to a timeless entry.
fn select_retime_targets(db: &Db, filter: &RetimeFilter) -> Result<Vec<Entry>> {
    let mut sql =
        String::from("SELECT * FROM entries WHERE deleted_at IS NULL AND occur_time IS NOT NULL");
    let mut bindings: Vec<String> = Vec::new();

    if let Some(needle) = filter.slug_or_hid {
        sql.push_str(" AND (slug = ? OR hid = ?)");
        bindings.push(needle.to_string());
        bindings.push(needle.to_string());
    }
    if let Some(field) = filter.field {
        sql.push_str(" AND field = ?");
        bindings.push(field.to_string());
    }
    if let Some(after) = filter.after_utc {
        sql.push_str(" AND occur_time >= ?");
        bindings.push(after.to_rfc3339());
    }
    if let Some(before) = filter.before_utc {
        sql.push_str(" AND occur_time <= ?");
        bindings.push(before.to_rfc3339());
    }

    let mut stmt = db.conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(bindings.iter()), row_to_entry)?;
    rows.map(|r| r.map_err(DatumError::Db))
        .collect::<Result<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Verb;
    use crate::ids::Slug;
    use crate::time::DatumTime;
    use chrono::{DateTime, Utc};

    fn seed_dt(rfc: &str) -> DatumTime {
        let utc: DateTime<Utc> = rfc.parse().unwrap();
        DatumTime {
            utc,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
        }
    }

    /// da-5ns.4: when `id_structure` references `%occur_time%`, retiming
    /// the entry must rekey the row in place (hid preserved, slug
    /// updated to track the new timestamp).
    #[test]
    fn retime_rekeys_when_id_structure_references_occur_time() {
        let db = Db::open_in_memory().unwrap();
        let mut e = Entry::new(
            "nap",
            Verb::Point,
            None,
            None,
            Some(seed_dt("2024-01-01T10:00:00Z")),
            None,
            "d1",
        );
        e.id_structure = Some("%occur_time%".to_string());
        let hid = e.hid.clone();
        db.insert_entry(&e).unwrap();
        // Stamp the templated slug post-insert so we start from the
        // canonical form (the default slug builder doesn't honor
        // id_structure, so we rekey explicitly here).
        let stamped_slug = Slug("nap:2024-01-01T10:00:00.000Z".to_string());
        let mut stamped = e.clone();
        stamped.slug = stamped_slug.clone();
        db.save_existing_entry_with_rekey(&e.slug, &stamped)
            .unwrap();

        let filter = RetimeFilter {
            slug_or_hid: Some(stamped_slug.as_str()),
            ..Default::default()
        };
        db.update_times(&filter, Duration::minutes(30)).unwrap();

        let by_hid = db.get_entry(&hid).unwrap();
        assert_eq!(by_hid.slug.as_str(), "nap:2024-01-01T10:30:00.000Z");
        assert_eq!(by_hid.hid, hid);
        assert!(db.get_entry(stamped_slug.as_str()).is_err());
    }

    /// `set_start_time_with_precision` (the `--time` retime path) must
    /// also trigger slug re-interpolation against the templated id.
    #[test]
    fn set_start_time_rekeys_when_template_references_occur_time() {
        let db = Db::open_in_memory().unwrap();
        let mut e = Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(seed_dt("2024-01-01T10:00:00Z")),
            None,
            "d1",
        );
        e.id_structure = Some("%occur_time%".to_string());
        let hid = e.hid.clone();
        db.insert_entry(&e).unwrap();
        let stamped_slug = Slug("coffee:2024-01-01T10:00:00.000Z".to_string());
        let mut stamped = e.clone();
        stamped.slug = stamped_slug.clone();
        db.save_existing_entry_with_rekey(&e.slug, &stamped)
            .unwrap();

        let new_start = seed_dt("2026-05-01T10:00:00Z");
        let filter = RetimeFilter {
            slug_or_hid: Some(stamped_slug.as_str()),
            ..Default::default()
        };
        let precision = Uncertainty {
            seconds: 60,
            unit: crate::time::PrecisionUnit::Minute,
        };
        db.set_start_time_with_precision(&filter, &new_start, &precision)
            .unwrap();

        let by_hid = db.get_entry(&hid).unwrap();
        assert_eq!(by_hid.slug.as_str(), "coffee:2026-05-01T10:00:00.000Z");
        assert_eq!(by_hid.hid, hid);
    }

    /// Two custom-id rows (`--id brew`) at different instants share one
    /// natural slug, so a sync merge keeps both and parks the loser under
    /// `coffee:brew_<hid>` (da-zlug). Seeds that pair.
    fn seed_parked_pair(db: &Db) -> (Entry, Entry) {
        let mut winner = Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(seed_dt("2024-01-01T08:00:00Z")),
            None,
            "d1",
        );
        winner.id_structure = Some("brew".to_string());
        winner.slug = Slug("coffee:brew".to_string());
        db.insert_entry(&winner).unwrap();

        let mut parked = Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(seed_dt("2024-01-01T09:00:00Z")),
            None,
            "d2",
        );
        parked.id_structure = Some("brew".to_string());
        parked.slug = crate::slug::parked("coffee:brew", &parked.hid);
        db.insert_entry(&parked).unwrap();
        (winner, parked)
    }

    /// da-l4uz: retiming a parked row must not re-derive its slug back onto
    /// the winner's — that is the collision the merge resolved, and the
    /// resolution is what two devices agreed on without coordinating.
    #[test]
    fn retime_keeps_a_parked_row_parked() {
        let db = Db::open_in_memory().unwrap();
        let (winner, parked) = seed_parked_pair(&db);

        let filter = RetimeFilter {
            slug_or_hid: Some(parked.slug.as_str()),
            ..Default::default()
        };
        db.update_times(&filter, Duration::minutes(30)).unwrap();

        let after = db.get_entry(&parked.hid).unwrap();
        assert_eq!(after.slug, parked.slug, "the park must survive the retime");
        assert_eq!(
            after.occur_time.as_ref().unwrap().utc,
            seed_dt("2024-01-01T09:30:00Z").utc,
            "the retime itself must still apply",
        );
        assert_eq!(
            db.get_entry("coffee:brew").unwrap().hid,
            winner.hid,
            "the winner keeps the contested slug",
        );
    }

    /// The park is kept only while someone else holds the natural slug.
    /// Alone, the row re-derives to the plain slug — the repair a user
    /// wants once the collision is gone.
    #[test]
    fn retime_unparks_when_the_contested_slug_is_free() {
        let db = Db::open_in_memory().unwrap();
        let mut lone = Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(seed_dt("2024-01-01T09:00:00Z")),
            None,
            "d2",
        );
        lone.id_structure = Some("brew".to_string());
        lone.slug = crate::slug::parked("coffee:brew", &lone.hid);
        db.insert_entry(&lone).unwrap();

        let filter = RetimeFilter {
            slug_or_hid: Some(lone.slug.as_str()),
            ..Default::default()
        };
        db.update_times(&filter, Duration::minutes(30)).unwrap();

        assert_eq!(
            db.get_entry(&lone.hid).unwrap().slug.as_str(),
            "coffee:brew"
        );
    }

    /// da-sunq: a mid-batch failure must roll back the whole batch.
    /// Two rows of one field retimed to the same absolute instant — the
    /// second row trips the partial UNIQUE index on (field, occur_time),
    /// and the row that was already shifted must come back untouched.
    #[test]
    fn retime_batch_rolls_back_on_midbatch_failure() {
        let db = Db::open_in_memory().unwrap();
        let t1 = seed_dt("2024-01-01T10:00:00Z");
        let t2 = seed_dt("2024-01-01T11:00:00Z");
        let a = Entry::new("run", Verb::Point, None, None, Some(t1.clone()), None, "d1");
        let b = Entry::new("run", Verb::Point, None, None, Some(t2.clone()), None, "d1");
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();

        let filter = RetimeFilter {
            field: Some("run"),
            ..Default::default()
        };
        let new_start = seed_dt("2026-05-01T10:00:00Z");
        let err = db.set_start_time_with_precision(&filter, &new_start, &Uncertainty::exact());
        assert!(
            err.is_err(),
            "second row must collide on (field, occur_time)"
        );

        // Neither row may carry the new time — including the one that
        // was successfully shifted before the failure.
        let a_after = db.get_entry(&a.hid).unwrap();
        let b_after = db.get_entry(&b.hid).unwrap();
        assert_eq!(a_after.occur_time.as_ref().unwrap().utc, t1.utc);
        assert_eq!(b_after.occur_time.as_ref().unwrap().utc, t2.utc);
    }

    /// da-sunq item 3 / da-r17h: timeless rows (occur_time NULL, slug
    /// `note:c<create_time>`) are not retime targets — an absolute retime
    /// over the field must not hand them an occur_time, since `occur_time
    /// IS NULL` is what the state/timeline views key the timeless row on.
    #[test]
    fn retime_skips_timeless_rows() {
        let db = Db::open_in_memory().unwrap();
        let timeless = Entry::new("note", Verb::Point, None, None, None, None, "d1");
        let timed = Entry::new(
            "note",
            Verb::Point,
            None,
            None,
            Some(seed_dt("2024-01-01T10:00:00Z")),
            None,
            "d1",
        );
        let timeless_slug = timeless.slug.clone();
        assert!(
            timeless_slug.as_str().starts_with("note:c"),
            "timeless slug is create_time-keyed, got {timeless_slug}"
        );
        db.insert_entry(&timeless).unwrap();
        db.insert_entry(&timed).unwrap();

        let filter = RetimeFilter {
            field: Some("note"),
            ..Default::default()
        };
        let new_start = seed_dt("2026-05-01T10:00:00Z");
        let n = db
            .set_start_time_with_precision(&filter, &new_start, &Uncertainty::exact())
            .unwrap();
        assert_eq!(n, 1, "only the timed row is a retime target");

        let unset_after = db.get_entry(timeless_slug.as_str()).unwrap();
        assert!(unset_after.occur_time.is_none());
        let timed_after = db.get_entry(&timed.hid).unwrap();
        assert_eq!(timed_after.occur_time.unwrap().utc, new_start.utc);
    }

    /// Retime on a row whose `id_structure` does NOT reference
    /// occur_time must leave the slug untouched (the slug-changed gate
    /// in [`Db::save_existing_entry_with_auto_reinterp_kind`] suppresses
    /// the rekey).
    #[test]
    fn retime_leaves_slug_alone_when_template_does_not_reference_occur_time() {
        let db = Db::open_in_memory().unwrap();
        let e = Entry::new(
            "nap",
            Verb::Point,
            None,
            None,
            Some(seed_dt("2024-01-01T10:00:00Z")),
            None,
            "d1",
        );
        let slug = e.slug.clone();
        let hid = e.hid.clone();
        db.insert_entry(&e).unwrap();

        let filter = RetimeFilter {
            slug_or_hid: Some(&hid),
            ..Default::default()
        };
        db.update_times(&filter, Duration::minutes(30)).unwrap();

        let after = db.get_entry(slug.as_str()).unwrap();
        assert_eq!(after.hid, hid);
    }

    /// da-2nik: the SHIFT path (`update_times`) mutates `occur_time.utc`
    /// directly. When the shift crosses a DST boundary the stored
    /// `offset_minutes` desyncs unless `refresh_offset` re-derives it —
    /// leaving `local_time()` rendering the wrong wall-clock (off by the
    /// DST hour). Shift a Europe/Berlin entry across spring-forward and
    /// assert the persisted row lands on the intended local time.
    #[test]
    fn retime_shift_across_dst_refreshes_offset() {
        use chrono::Timelike;

        let db = Db::open_in_memory().unwrap();
        // 2024-03-31 01:30 Berlin (CET, UTC+1) — 30 min before the
        // 02:00→03:00 spring-forward transition.
        let before =
            DatumTime::from_utc_and_tz("2024-03-31T00:30:00Z".parse().unwrap(), "Europe/Berlin")
                .unwrap();
        assert_eq!(before.offset_minutes, 60, "pre-shift is CET (UTC+1)");

        let e = Entry::new("wake", Verb::Point, None, None, Some(before), None, "d1");
        let hid = e.hid.clone();
        db.insert_entry(&e).unwrap();

        // Shift +2h → UTC 02:30 = 04:30 Berlin (CEST, UTC+2). Intended
        // wall-clock is 04:30.
        let filter = RetimeFilter {
            slug_or_hid: Some(&hid),
            ..Default::default()
        };
        db.update_times(&filter, Duration::hours(2)).unwrap();

        let after = db.get_entry(&hid).unwrap();
        let t = after.occur_time.unwrap();
        assert_eq!(
            t.offset_minutes, 120,
            "offset must re-derive to CEST (UTC+2) after crossing DST"
        );
        let local = t.local_time();
        assert_eq!(
            (local.hour(), local.minute()),
            (4, 30),
            "local_time must land on the intended 04:30 wall-clock, not the stale-offset 03:30"
        );
    }

    /// da-2nik companion: `Entry::computed_end_time` bumps `occur_time.utc`
    /// by the duration to derive the end. A block whose end crosses a DST
    /// boundary must render with the end's offset, not the start's stale
    /// one.
    #[test]
    fn computed_end_time_refreshes_offset_across_dst() {
        use chrono::Timelike;

        // Start 2024-03-31 01:30 Berlin (CET, UTC+1); 2h block ends
        // 04:30 Berlin (CEST, UTC+2).
        let start =
            DatumTime::from_utc_and_tz("2024-03-31T00:30:00Z".parse().unwrap(), "Europe/Berlin")
                .unwrap();
        let mut e = Entry::new("focus", Verb::Point, None, None, Some(start), None, "d1");
        e.dur = Some(crate::dur::Dur::Of(chrono::Duration::hours(2)));

        let end = e.computed_end_time().unwrap();
        assert_eq!(end.offset_minutes, 120, "end offset must re-derive to CEST");
        let local = end.local_time();
        assert_eq!(
            (local.hour(), local.minute()),
            (4, 30),
            "end wall-clock must be 04:30, not the start's stale-offset 03:30"
        );
    }
}
