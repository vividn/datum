//! Bulk retime paths.
//!
//! Post-pivot the entry's time-shape is just `occur_time` + `dur` — no
//! end_time to slide. These methods retime `occur_time` and leave `dur`
//! untouched — except `clear_occur_time` (`retime -T`), which makes a note.
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
use crate::ids::Slug;

use super::row_mapper::row_to_entry;
use super::types::{RetimeFilter, Retimed};
use super::Db;

impl Db {
    /// Retime matching rows to a fresh absolute `occur_time`, which
    /// `new_start` makes from the time each row has now (da-jix52: `retime
    /// -t 1040` is 10:40 on the row's own day). Returns one
    /// [`Retimed`] per moved row — the entry plus the time it came from, so
    /// the caller can render `before ⟶ after`.
    ///
    /// The new time carries its own uncertainty (`DatumTime::uncertainty`,
    /// an ISO-8601 ± radius), so retiming replaces both together and cannot
    /// leave a stale uncertainty describing a time that no longer exists —
    /// which is the whole point of da-b8rs putting it there.
    ///
    /// da-sunq: the whole batch runs in one transaction — a mid-batch
    /// failure (rekey collision, disk error) rolls back every
    /// already-shifted row, so a partial shift is never visible.
    ///
    /// `zone_stated` (`retime -z`) drops a zone's `tz_from` marker
    /// ([`Entry::zone_stated`]) in the same save, so in the same op.
    pub fn set_start_time(
        &self,
        filter: &RetimeFilter,
        zone_stated: bool,
        mut new_start: impl FnMut(&crate::time::DatumTime) -> Result<crate::time::DatumTime>,
    ) -> Result<Vec<Retimed>> {
        self.retime_with(filter, |e| {
            if let Some(t) = e.occur_time.as_mut() {
                *t = new_start(t)?;
            }
            crate::timezone_field::keep_in_named_zone(e)?;
            if zone_stated {
                e.zone_stated();
            }
            Ok(())
        })
    }

    /// The rows [`Self::set_start_time`] would move, read outside its
    /// transaction: retime asks about a time they make ambiguous before it
    /// writes (da-z3fzv).
    pub fn retime_targets(&self, filter: &RetimeFilter) -> Result<Vec<Entry>> {
        select_retime_targets(self, filter)
    }

    /// Shift matching rows' `occur_time` by `offset`. Atomic over the
    /// batch, same as [`Self::set_start_time`] (da-sunq).
    pub fn update_times(&self, filter: &RetimeFilter, offset: Duration) -> Result<Vec<Retimed>> {
        self.retime_with(filter, |e| {
            if let Some(t) = e.occur_time.as_mut() {
                t.utc += offset;
                // A raw `utc` mutation desyncs `offset_minutes` (see the
                // DatumTime doc-comment); re-derive it so a cross-DST shift
                // renders the intended wall-clock and rekeys %occur_time%
                // slugs correctly. Propagates on unknown-tz.
                t.refresh_offset()?;
            }
            Ok(())
        })
    }

    /// `retime -T` (da-1d0v2): take matching rows' `occur_time` away, making
    /// each the note `-T` logs. A state or a duration leaves a point
    /// (`Dur::moment()`), as `start -T` / `occur -T` store; the span and the
    /// following state, which only a time can place, go. The default `%occur_time%` id becomes
    /// `%create_time%`, so the slug is `field:<create_time>c` as a timeless
    /// insert's is. Atomic over the batch, same as [`Self::set_start_time`].
    pub fn clear_occur_time(&self, filter: &RetimeFilter) -> Result<Vec<Retimed>> {
        self.retime_with(filter, |e| {
            e.occur_time = None;
            e.following_state = None;
            e.dur = (e.state.is_some() || e.dur.is_some()).then(crate::dur::Dur::moment);
            if e.id_structure.as_deref() == Some("%occur_time%") {
                e.id_structure = Some("%create_time%".into());
            }
            Ok(())
        })
    }

    /// Apply `change` to every row the filter selects, in one transaction
    /// (da-sunq: a mid-batch failure — rekey collision, disk error — rolls
    /// back every row already changed, so a partial retime is never
    /// visible), through [`Db::save_edits`].
    fn retime_with(
        &self,
        filter: &RetimeFilter,
        mut change: impl FnMut(&mut Entry) -> Result<()>,
    ) -> Result<Vec<Retimed>> {
        let (saved, befores) = self.save_edits(
            |_| {
                let (mut rows, mut befores) = (Vec::new(), Vec::new());
                for mut e in select_retime_targets(self, filter)? {
                    // Targets always carry an occur_time (the selector
                    // excludes timeless rows), so the `before` half of the
                    // pair is always there to render.
                    befores.push(e.occur_time.clone());
                    let old_slug = e.slug.clone();
                    change(&mut e)?;
                    rows.push((old_slug, e));
                }
                Ok((rows, befores))
            },
            crate::history::ChangeKind::Retime,
        )?;
        Ok(saved
            .into_iter()
            .zip(befores)
            .filter_map(|(entry, before)| {
                Some(Retimed {
                    before: before?,
                    entry,
                })
            })
            .collect())
    }

    /// Save each row `compute` edits, by the slug it had, as `kind`, in one
    /// transaction. The rows are found and changed before the lock, and
    /// saved on the database they were read from (da-8i7f0), each through
    /// [`Db::save_existing_entry_with_auto_reinterp_kind_in_tx`], so a
    /// templated slug is re-derived. Returns the saved rows and what else
    /// `compute` gave.
    pub(super) fn save_edits<P>(
        &self,
        compute: impl FnMut(&super::View) -> Result<(Vec<(Slug, Entry)>, P)>,
        kind: crate::history::ChangeKind,
    ) -> Result<(Vec<Entry>, P)> {
        self.compute_then_write_with(compute, |_, (rows, more)| {
            let mut saved = Vec::with_capacity(rows.len());
            for (slug, e) in rows {
                let (e, _) = self.save_existing_entry_with_auto_reinterp_kind_in_tx(
                    &slug,
                    e,
                    DEFAULT_ID_DELIMITER,
                    kind,
                )?;
                saved.push(e);
            }
            Ok((saved, more))
        })
    }
}

/// Select live rows matching the retime filter. Shared between the
/// absolute (`set_start_time`) and offset (`update_times`)
/// paths so the SQL surface stays in one place.
///
/// Timeless rows (`occur_time IS NULL`, slug `field:<create_time>c`) are
/// excluded: retime moves an existing time, and giving a timeless row one
/// would contradict the `occur_time IS NULL` marker the state/timeline views
/// key on (da-r17h). Use `datum update` to attach a time to a timeless entry.
fn select_retime_targets(db: &Db, filter: &RetimeFilter) -> Result<Vec<Entry>> {
    let mut sql =
        String::from("SELECT * FROM entries WHERE deleted_at IS NULL AND occur_time IS NOT NULL");
    let mut bindings: Vec<String> = Vec::new();

    if let Some(needle) = filter.slug_or_hid {
        // da-uxgv: a superseded row is still addressable by its hid, and
        // retiming one has to keep working — the pointer survives the move
        // precisely because it is an hid and not a re-derivable key. It is
        // only kept out of the *bulk* selection below, where "every row in
        // this window" means the live stream.
        sql.push_str(" AND (slug = ? OR hid = ?) AND (superseded_by IS NULL OR hid = ?)");
        bindings.push(needle.to_string());
        bindings.push(needle.to_string());
        bindings.push(needle.to_string());
    } else {
        sql.push_str(" AND superseded_by IS NULL");
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
    use crate::db::lookup::fetch_entry_any;
    use crate::entry::Verb;
    use crate::ids::Slug;
    use crate::time::DatumTime;
    use crate::time::Uncertainty;
    use chrono::{DateTime, Utc};

    fn seed_dt(rfc: &str) -> DatumTime {
        let utc: DateTime<Utc> = rfc.parse().unwrap();
        DatumTime {
            utc,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
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

    /// `set_start_time` (the `--time` retime path) must
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
        let mut new_start = new_start;
        new_start.uncertainty = Uncertainty {
            seconds: 60,
            unit: crate::time::UncertaintyUnit::Minute,
        }
        .to_iso();
        db.set_start_time(&filter, false, |_| Ok(new_start.clone()))
            .unwrap();

        let by_hid = db.get_entry(&hid).unwrap();
        assert_eq!(by_hid.slug.as_str(), "coffee:2026-05-01T10:00:00.000Z");
        assert_eq!(by_hid.hid, hid);
    }

    /// Two custom-id rows (`--id brew`) at different instants derive one
    /// natural slug, so a sync merge keeps both and supersedes the loser
    /// (da-uxgv). Seeds that pair — the loser keeps `coffee:brew`, which is
    /// what its template says.
    fn seed_superseded_pair(db: &Db) -> (Entry, Entry) {
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

        let mut loser = Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(seed_dt("2024-01-01T09:00:00Z")),
            None,
            "d2",
        );
        loser.id_structure = Some("brew".to_string());
        loser.slug = Slug(format!("coffee:brew-staging-{}", loser.hid));
        db.insert_entry(&loser).unwrap();
        db.conn()
            .execute(
                "UPDATE entries SET slug = 'coffee:brew', superseded_by = ?2 WHERE hid = ?1",
                rusqlite::params![loser.hid, winner.hid],
            )
            .unwrap();
        loser.slug = Slug("coffee:brew".to_string());
        loser.superseded_by = Some(winner.hid.clone());
        (winner, loser)
    }

    /// da-uxgv, and the reason the pointer is an hid rather than a boolean:
    /// retiming a superseded row moves its slug, which is precisely the key
    /// a "who else holds my slug?" rule would have looked the relationship
    /// up by. The pointer outlives the move, and the row stays set aside.
    #[test]
    fn retime_preserves_supersession_of_the_loser() {
        let db = Db::open_in_memory().unwrap();
        let (winner, loser) = seed_superseded_pair(&db);

        let filter = RetimeFilter {
            slug_or_hid: Some(loser.hid.as_str()),
            ..Default::default()
        };
        db.update_times(&filter, Duration::minutes(30)).unwrap();

        let after = fetch_entry_any(db.conn(), loser.hid.as_str()).unwrap();
        assert_eq!(
            after.superseded_by,
            Some(winner.hid.clone()),
            "the relationship must survive the retime",
        );
        assert_eq!(
            after.occur_time.as_ref().unwrap().utc,
            seed_dt("2024-01-01T09:30:00Z").utc,
            "the retime itself must still apply",
        );
        assert_eq!(
            after.slug, loser.slug,
            "a `brew` template does not move with the clock, so the slug stands",
        );
        assert_eq!(
            db.get_entry("coffee:brew").unwrap().hid,
            winner.hid,
            "and the slug still resolves to the row in the stream",
        );
    }

    /// Retiming the *winner* is the harder half of the same rule: the row
    /// that was beaten is now the only holder of the contested slug, and it
    /// still must not quietly reappear. Nothing re-derives the pointer, so
    /// nothing promotes it.
    #[test]
    fn retime_of_the_winner_does_not_promote_the_loser() {
        let db = Db::open_in_memory().unwrap();
        let (winner, loser) = seed_superseded_pair(&db);

        // Give the winner a time-derived template so the retime actually
        // moves its slug off the contested one.
        db.conn()
            .execute(
                "UPDATE entries SET id_structure = '%occur_time%' WHERE hid = ?1",
                rusqlite::params![winner.hid],
            )
            .unwrap();
        let filter = RetimeFilter {
            slug_or_hid: Some(winner.hid.as_str()),
            ..Default::default()
        };
        db.update_times(&filter, Duration::minutes(30)).unwrap();

        let after = fetch_entry_any(db.conn(), loser.hid.as_str()).unwrap();
        assert_eq!(
            after.superseded_by,
            Some(winner.hid),
            "the loser stays set aside even with the slug to itself",
        );
        assert!(
            db.get_entry("coffee:brew").is_err(),
            "and the freed slug resolves to nothing in the live stream",
        );
    }

    /// da-bgq2: the retime paths report the move, not a count — each
    /// moved row comes back paired with the `occur_time` it left, which
    /// is what the RETIME line renders as `before ⟶ after`.
    #[test]
    fn retime_reports_before_and_after_per_row() {
        let db = Db::open_in_memory().unwrap();
        let e = Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(seed_dt("2024-01-01T08:00:00Z")),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();

        let filter = RetimeFilter {
            slug_or_hid: Some(&e.hid),
            ..Default::default()
        };
        let moved = db.update_times(&filter, Duration::hours(1)).unwrap();
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].entry.hid, e.hid);
        assert_eq!(moved[0].before.utc, seed_dt("2024-01-01T08:00:00Z").utc);
        assert_eq!(
            moved[0].entry.occur_time.as_ref().unwrap().utc,
            seed_dt("2024-01-01T09:00:00Z").utc,
        );

        let target = seed_dt("2024-02-01T07:00:00Z");
        let moved = db
            .set_start_time(&filter, false, |_| Ok(target.clone()))
            .unwrap();
        assert_eq!(moved.len(), 1);
        assert_eq!(
            moved[0].before.utc,
            seed_dt("2024-01-01T09:00:00Z").utc,
            "the absolute path reports where the row came from too",
        );
        assert_eq!(moved[0].entry.occur_time.as_ref().unwrap().utc, target.utc);
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
        // State-bearing (da-2fkr): `(field, occur_time)` is a single fact
        // only for rows that assert a state, so a switch is what the
        // batch's second row can still collide against. Two stateless
        // points retimed onto one instant are two events and both land —
        // `retime_batch_lands_two_stateless_points_on_one_instant` below.
        let a = Entry::new(
            "run",
            Verb::Switch,
            None,
            None,
            Some(t1.clone()),
            None,
            "d1",
        );
        let b = Entry::new(
            "run",
            Verb::Switch,
            None,
            None,
            Some(t2.clone()),
            None,
            "d1",
        );
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();

        let filter = RetimeFilter {
            field: Some("run"),
            ..Default::default()
        };
        let new_start = seed_dt("2026-05-01T10:00:00Z");
        let err = db.set_start_time(&filter, false, |_| Ok(new_start.clone()));
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

    /// da-2fkr: the same batch, stateless. Two transactions retimed onto one
    /// day are two events — both move, nothing rolls back, and the rows stay
    /// distinguishable by hid (their slugs are `%occur_time%`-free here, so
    /// neither is rewritten).
    #[test]
    fn retime_batch_lands_two_stateless_points_on_one_instant() {
        let db = Db::open_in_memory().unwrap();
        let t1 = seed_dt("2024-01-01T10:00:00Z");
        let t2 = seed_dt("2024-01-01T11:00:00Z");
        let a = Entry::new("tx", Verb::Point, None, None, Some(t1), None, "d1");
        let b = Entry::new("tx", Verb::Point, None, None, Some(t2), None, "d1");
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();

        let filter = RetimeFilter {
            field: Some("tx"),
            ..Default::default()
        };
        let new_start = seed_dt("2026-05-01T10:00:00Z");
        let moved = db
            .set_start_time(&filter, false, |_| Ok(new_start.clone()))
            .unwrap();
        assert_eq!(moved.len(), 2);

        for hid in [&a.hid, &b.hid] {
            let after = db.get_entry(hid).unwrap();
            assert_eq!(after.occur_time.as_ref().unwrap().utc, new_start.utc);
            assert!(after.deleted_at.is_none());
            assert_eq!(after.superseded_by, None);
        }
    }

    /// da-sunq item 3 / da-r17h: timeless rows (occur_time NULL, slug
    /// `note:<create_time>c`) are not retime targets — an absolute retime
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
            timeless_slug == crate::entry::build_slug("note", None, &timeless.create_time),
            "timeless slug is create_time-keyed, got {timeless_slug}"
        );
        db.insert_entry(&timeless).unwrap();
        db.insert_entry(&timed).unwrap();

        let filter = RetimeFilter {
            field: Some("note"),
            ..Default::default()
        };
        let new_start = seed_dt("2026-05-01T10:00:00Z");
        let moved = db
            .set_start_time(&filter, false, |_| Ok(new_start.clone()))
            .unwrap();
        assert_eq!(moved.len(), 1, "only the timed row is a retime target");

        let unset_after = db.get_entry(timeless_slug.as_str()).unwrap();
        assert!(unset_after.occur_time.is_none());
        let timed_after = db.get_entry(&timed.hid).unwrap();
        assert_eq!(timed_after.occur_time.unwrap().utc, new_start.utc);
    }

    /// da-1d0v2: `retime -T` makes a block-switch a timeless point about its
    /// state — the span and the following state go, the default id moves to
    /// `%create_time%`, and the hid stays.
    #[test]
    fn clear_occur_time_makes_a_note() {
        let db = Db::open_in_memory().unwrap();
        let mut e = Entry::new(
            "mood",
            Verb::BlockSwitch,
            Some(serde_json::json!("happy")),
            None,
            Some(seed_dt("2024-01-01T10:00:00Z")),
            crate::dur::Dur::parse("PT30M"),
            "d1",
        );
        e.following_state = Some(serde_json::json!("sad"));
        e.id_structure = Some("%occur_time%".to_string());
        db.insert_entry(&e).unwrap();

        let filter = RetimeFilter {
            slug_or_hid: Some(&e.hid.0),
            ..Default::default()
        };
        let moved = db.clear_occur_time(&filter).unwrap();
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].before.utc, seed_dt("2024-01-01T10:00:00Z").utc);

        let after = db.get_entry(&e.hid.0).unwrap();
        assert!(after.occur_time.is_none());
        assert_eq!(after.verb, Verb::Point);
        assert_eq!(after.state, Some(serde_json::json!("happy")));
        assert!(after.dur.as_ref().is_some_and(crate::dur::Dur::is_moment));
        assert!(after.following_state.is_none());
        assert!(
            after.slug == crate::entry::build_slug("mood", None, &after.create_time),
            "got {}",
            after.slug
        );
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

    /// da-2nik companion: `Entry::computed_start_time` walks `occur_time.utc`
    /// BACK by the duration to derive the start (da-zwct: `occur_time` is a
    /// block's end). A block whose start crosses a DST boundary must render
    /// with the start's offset, not the end's stale one.
    #[test]
    fn computed_start_time_refreshes_offset_across_dst() {
        use chrono::Timelike;

        // The block ENDS 2024-03-31 04:30 Berlin (CEST, UTC+2); the 2h
        // that precede it start at 01:30 Berlin (CET, UTC+1).
        let occur =
            DatumTime::from_utc_and_tz("2024-03-31T02:30:00Z".parse().unwrap(), "Europe/Berlin")
                .unwrap();
        let mut e = Entry::new("focus", Verb::Point, None, None, Some(occur), None, "d1");
        e.dur = Some(crate::dur::Dur::of(chrono::Duration::hours(2)));

        let start = e.computed_start_time().unwrap();
        assert_eq!(
            start.offset_minutes, 60,
            "start offset must re-derive to CET"
        );
        let local = start.local_time();
        assert_eq!(
            (local.hour(), local.minute()),
            (1, 30),
            "start wall-clock must be 01:30, not the end's stale-offset 02:30"
        );
    }
}
