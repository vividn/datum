//! The `tz` checker: a stored offset its zone disagrees with, and a zone
//! name nothing resolves.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use datum_core::db::{agreed_zone, Db, NEIGHBOURS};
use datum_core::entry::Entry;

use super::{render_flagged_block, rows_with, Domain, FlaggedEntry, Issue, IssueExample, Severity};

/// Recompute each live entry's `occur_time` offset from its `timezone` at
/// `utc` and flag any row whose stored `offset_minutes` disagrees — the
/// `tz` checker. A `DatumTime`'s three fields are not independent
/// (`crates/datum-parse/src/time/datum_time.rs`): `offset_minutes` must
/// equal the offset `timezone` produces at `utc`. v3 derives the offset at
/// construction so a freshly-written row is always in sync, but a legacy
/// import, a sync merge, a hand-edited DB row, or a time mutation that
/// skipped `refresh_offset` can leave a row desynced — and a wrong stored
/// offset renders as a wrong wall-clock time, the cardinal error for a
/// time tracker. This is the `check`-time complement to v3's
/// prevention-by-construction (and the replacement for v2's dropped
/// read-time mismatch warn).
///
/// A row with an unparseable `timezone` is flagged here too, as
/// `unknown_timezone`: the offset can't be verified until the name is
/// fixed, and it is the name and not the offset that is wrong. A row with *no* stored `timezone` is skipped — a legacy import
/// can carry an offset without a zone name (da-vfbq), and an offset alone
/// is self-verifying: there is no zone claim for it to disagree with.
/// Tombstones are skipped too — historical rows, not live data.
///
/// Report-only: the "which field is authoritative — tz or offset?" call
/// deserves a human, so there is deliberately no `--fix` re-derivation yet
/// (a future `--fix` would trust `timezone` and re-run `refresh_offset`).
///
/// With `around`, under each row the entries either side of it and their
/// zones and offsets, which `datum check tz` shows (da-3zq6g). With `near`,
/// a zone the rows around one agree is not its own is read only near those
/// instants ([`off_zone`]).
pub(super) fn collect_tz_drift(
    db: &Db,
    entries: &[Entry],
    around: bool,
    near: Option<&BTreeSet<String>>,
) -> Result<Vec<Issue>> {
    // Two faults with two fixes (da-kvosn): an offset its zone disagrees
    // with, and a zone name nothing resolves, whose offset may be right.
    let mut mismatched: Vec<(Entry, String, String)> = Vec::new();
    let mut unknown: Vec<(Entry, String, String)> = Vec::new();
    for e in entries {
        if e.deleted_at.is_some() {
            continue;
        }
        let Some(occur) = e.occur_time.as_ref() else {
            continue;
        };
        let tz = occur.timezone.as_deref().unwrap_or("—");
        let stored = occur.offset_minutes;
        match occur.expected_offset_minutes() {
            // No stored zone, or one that agrees with the offset: nothing to
            // reconcile.
            Ok(None) => {}
            Ok(Some(expected)) if expected == stored => {}
            // The stored→expected offset, or the name that failed, so the
            // row's problem is legible without re-reading the DB.
            Ok(Some(expected)) => mismatched.push((
                e.clone(),
                format!("tz={tz}  stored_offset={stored}  expected_offset={expected}"),
                format!("{stored}→{expected}"),
            )),
            Err(err) => unknown.push((
                e.clone(),
                format!("tz={tz}  stored_offset={stored}  {err}"),
                format!("tz={tz}"),
            )),
        }
    }

    let issue = |rows: Vec<(Entry, String, String)>,
                 severity,
                 category,
                 says: &str|
     -> Result<Option<Issue>> {
        let Some(first) = rows.first() else {
            return Ok(None);
        };
        let found: Vec<Entry> = rows.iter().map(|r| r.0.clone()).collect();
        let rendered_block = match around {
            true => {
                let under = found.iter().map(|e| neighbors(db, e).map(Some));
                rows_with(db, &found, &under.collect::<Result<Vec<_>>>()?)
            }
            false => render_flagged_block(db, &found),
        };
        Ok(Some(Issue {
            domain: Domain::Entries,
            severity,
            category,
            message: match rows.len() {
                1 => format!("1 entry has {says}"),
                n => format!("{n} entries have {says}"),
            },
            count: rows.len(),
            example: Some(IssueExample {
                id: Some(first.0.slug.to_string()),
                field: Some(first.0.field.clone()),
                detail: Some(first.1.clone()),
            }),
            entries: rows
                .iter()
                .map(|(e, _, state)| FlaggedEntry {
                    hid: e.hid.to_string(),
                    field: e.field.clone(),
                    state: state.clone(),
                })
                .collect(),
            rendered_block,
        }))
    };
    let names: std::collections::BTreeSet<&str> = unknown
        .iter()
        .filter_map(|(e, ..)| e.occur_time.as_ref()?.timezone.as_deref())
        .collect();
    let names = names.into_iter().collect::<Vec<_>>().join(", ");
    Ok([
        issue(
            mismatched,
            Severity::Error,
            "tz_offset_mismatch",
            "a stored UTC offset that disagrees with their timezone at that instant",
        )?,
        issue(
            unknown,
            Severity::Error,
            "unknown_timezone",
            &format!(
                "a timezone name datum cannot resolve ({names}), so the offset cannot be checked"
            ),
        )?,
        issue(
            off_zone(db, near)?,
            Severity::Warning,
            "tz_off_neighbours",
            "a timezone the entries around them agree is not theirs",
        )?,
    ]
    .into_iter()
    .flatten()
    .collect())
}

/// The [`NEIGHBOURS`] entries either side of `e`, oldest first, each at its
/// own wall clock and offset, in its zone.
fn neighbors(db: &Db, e: &Entry) -> Result<String> {
    let Some(at) = e.occur_time.as_ref() else {
        return Ok("no time, so no entries around it".into());
    };
    let (mut before, after) = db.entries_around(at.utc)?;
    before.reverse();
    let said = |rows: &[Entry]| {
        let each: Vec<String> = (rows.iter())
            .filter_map(|n| {
                let t = n.occur_time.as_ref()?;
                let offset = chrono::FixedOffset::east_opt(t.offset_minutes * 60)?;
                Some(format!(
                    "{} {} ({})",
                    t.utc.with_timezone(&offset).format("%Y-%m-%d %H:%M %:z"),
                    t.timezone.as_deref().unwrap_or("—"),
                    n.short_uid(),
                ))
            })
            .collect();
        match each.is_empty() {
            true => "none".to_string(),
            false => each.join(", "),
        }
    };
    Ok(format!(
        "before: {}\n  ↳ after: {}",
        said(&before),
        said(&after)
    ))
}

/// The entries in a run of fewer than [`NEIGHBOURS`] in one zone, where the
/// entries either side of the run agree on another ([`agreed_zone`],
/// da-h8dvi): a spike. A move is a step, and a run as long as the window
/// is taken for one, and a run at either end of the record, with nothing on
/// one side, is not a spike. A warning, as a short trip logged once or twice
/// looks the same.
///
/// Every live zoned row's zone, in time order, is read from its index, and
/// only the rows found are read whole (da-3zq6g). With `near`, only the
/// stretches around those instants: a row moved, gone or come there changes
/// what is found within [`REACH`]` / 2` rows of it, and no further, and each
/// stretch reads [`REACH`] either side, so a run and the rows either side of
/// it are whole for every row it decides.
fn off_zone(db: &Db, near: Option<&BTreeSet<String>>) -> Result<Vec<(Entry, String, String)>> {
    let mut off: BTreeMap<i64, (String, String)> = BTreeMap::new();
    match near {
        None => {
            let zoned = db.zone_sequence()?;
            for (i, around) in spikes(&zoned) {
                off.insert(zoned[i].0, (zoned[i].1.clone(), around));
            }
        }
        Some(near) => {
            for at in near {
                let (zoned, at) = db.zone_window(at, REACH)?;
                let decided = at.saturating_sub(REACH / 2)..at + REACH / 2;
                for (i, around) in spikes(&zoned)
                    .into_iter()
                    .filter(|(i, _)| decided.contains(i))
                {
                    off.insert(zoned[i].0, (zoned[i].1.clone(), around));
                }
            }
        }
    }
    let ids: Vec<i64> = off.keys().copied().collect();
    let mut found: Vec<(Entry, String, String)> = (db.entries_at_rowids(&ids)?.into_iter())
        .zip(off.into_values())
        .map(|(e, (own, around))| {
            (
                e,
                format!("tz={own}  around={around}"),
                format!("{own}, around {around}"),
            )
        })
        .collect();
    found.sort_by_key(|(e, ..)| e.occur_time.as_ref().map(|t| t.utc));
    Ok(found)
}

/// How far either side of an instant [`off_zone`] reads: a row there changes
/// the runs within `2 ×` [`NEIGHBOURS`] rows of it, each of which reads
/// [`NEIGHBOURS`] more of itself and [`NEIGHBOURS`] either side.
const REACH: usize = 4 * NEIGHBOURS + 2;

/// The rows of `zoned` in a run of fewer than [`NEIGHBOURS`] in one zone
/// whose neighbors agree on another, each with that zone.
fn spikes(zoned: &[(i64, String)]) -> Vec<(usize, String)> {
    let zones: Vec<&str> = zoned.iter().map(|(_, z)| z.as_str()).collect();
    let mut off = Vec::new();
    let mut start = 0;
    for run in zoned.chunk_by(|a, b| a.1 == b.1) {
        let end = start + run.len();
        let before = &zones[start.saturating_sub(NEIGHBOURS)..start];
        let after = &zones[end..(end + NEIGHBOURS).min(zones.len())];
        let own = run[0].1.as_str();
        let around = (run.len() < NEIGHBOURS)
            .then(|| agreed_zone(before, after))
            .flatten()
            .filter(|z| **z != own);
        if let Some(around) = around {
            off.extend((start..end).map(|i| (i, around.to_string())));
        }
        start = end;
    }
    off
}

#[cfg(test)]
mod tests {
    use super::super::tests::{all, t_at};
    use super::*;
    use datum_core::entry::Verb;
    use datum_core::time::DatumTime;

    /// da-3zq6g: a second run reads a zone the rows around one agree is not
    /// its own only around the instants that changed and those it found last
    /// time. For random zone sequences and a random change (a row in, a row
    /// gone, a row moved), what those stretches find is what the whole
    /// sequence gives, the changed instants being what `check_changes`
    /// logged: a moved row's old place too.
    #[test]
    fn stretches_around_a_change_find_what_the_whole_sequence_finds() {
        let mut seed: u64 = 0x3e7a_91c4;
        let mut next = |n: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        let zones = ["Europe/Berlin", "America/New_York", "Asia/Tokyo"];
        let base = t_at("2026-01-01T00:00:00+00:00").utc;
        let at = |i: u64| base + chrono::Duration::minutes(10 * i as i64);
        let hids = |found: Vec<(Entry, String, String)>| -> BTreeSet<String> {
            found.into_iter().map(|(e, ..)| e.hid.to_string()).collect()
        };
        for _ in 0..150 {
            let db = Db::open_in_memory().unwrap();
            // Mostly one zone, in runs, so spikes and steps both happen.
            let mut zone = 0;
            for i in 0..40 {
                if next(4) == 0 {
                    zone = next(3) as usize;
                }
                let t = DatumTime::from_utc_and_tz(at(i), zones[zone]).unwrap();
                db.insert_entry(&Entry::new(
                    "walk",
                    Verb::Point,
                    None,
                    None,
                    Some(t),
                    None,
                    "d",
                ))
                .unwrap();
            }
            let found = off_zone(&db, None).unwrap();
            let through = db.check_through().unwrap();
            let row: String = db
                .conn()
                .query_row(
                    "SELECT hid FROM entries ORDER BY occur_time LIMIT 1 OFFSET ?1",
                    [next(40)],
                    |r| r.get(0),
                )
                .unwrap();
            let to = (at(next(40)) + chrono::Duration::minutes(5)).to_rfc3339();
            match next(3) {
                0 => {
                    let t = at(next(40)) + chrono::Duration::minutes(3);
                    let t = DatumTime::from_utc_and_tz(t, zones[next(3) as usize]);
                    let e =
                        Entry::new("walk", Verb::Point, None, None, Some(t.unwrap()), None, "d");
                    db.insert_entry(&e).unwrap();
                }
                1 => {
                    let gone = "UPDATE entries SET deleted_at = '2026-02-01T00:00:00+00:00' WHERE hid = ?1";
                    db.conn().execute(gone, [&row]).unwrap();
                }
                _ => {
                    let moved = "UPDATE entries SET occur_time = ?2 WHERE hid = ?1";
                    db.conn().execute(moved, [&row, &to]).unwrap();
                }
            }
            // The instants a second run reads around, as it reads them: what
            // the last run found, and what the triggers logged since, a moved
            // row's old place among them.
            let logged = db.check_changed(through).unwrap().unwrap().into_iter();
            let near: BTreeSet<String> = (found.iter())
                .filter_map(|(e, ..)| Some(e.occur_time.as_ref()?.utc.to_rfc3339()))
                .chain(logged.map(|(.., at)| at).filter(|at| !at.is_empty()))
                .collect();
            let whole = hids(off_zone(&db, None).unwrap());
            assert_eq!(hids(off_zone(&db, Some(&near)).unwrap()), whole);
        }
    }

    /// A row whose stored `occur_time_offset` disagrees with the offset its
    /// `occur_time_tz` produces at that instant is flagged by the `tz`
    /// checker as a `tz_offset_mismatch` ERROR. v3 never writes such a row
    /// (the offset is derived at construction), so the test seeds the
    /// desync directly into the column — exactly the legacy-import /
    /// machine-timezone-seam class this checker exists to catch.
    #[test]
    fn tz_check_flags_desynced_offset() {
        let db = Db::open_in_memory().unwrap();
        // America/Chicago at this instant is CST (-360). new_legacy derives
        // the correct offset; we then corrupt the stored column to +0.
        let occur =
            DatumTime::from_utc_and_tz(t_at("2024-01-15T12:00:00+00:00").utc, "America/Chicago")
                .unwrap();
        assert_eq!(occur.offset_minutes, -360);
        let entry = Entry::new_legacy("work", Verb::Switch, None, None, Some(occur), None, "d1");
        db.insert_entry(&entry).unwrap();
        db.conn()
            .execute(
                "UPDATE entries SET occur_time_offset = 0 WHERE field = 'work'",
                [],
            )
            .unwrap();

        let issues = collect_tz_drift(&db, &all(&db), false, None).unwrap();
        assert_eq!(issues.len(), 1, "exactly one tz issue");
        let issue = &issues[0];
        assert_eq!(issue.category, "tz_offset_mismatch");
        assert_eq!(issue.severity, Severity::Error);
        assert_eq!(issue.count, 1);
        let detail = issue
            .example
            .as_ref()
            .and_then(|e| e.detail.as_deref())
            .unwrap_or_default();
        assert!(
            detail.contains("stored_offset=0") && detail.contains("expected_offset=-360"),
            "detail names both offsets: {detail}",
        );
    }

    /// da-h8dvi: hourly `work` rows, `B` in Berlin and `T` in Tokyo, and the
    /// hours `check tz` reports as off their neighbours' zone.
    fn off_hours(zones: &str) -> Vec<String> {
        let db = Db::open_in_memory().unwrap();
        for (h, z) in zones.chars().enumerate() {
            let tz = if z == 'B' {
                "Europe/Berlin"
            } else {
                "Asia/Tokyo"
            };
            let at = t_at("2024-01-15T00:00:00+00:00").utc + chrono::Duration::hours(h as i64);
            let occur = DatumTime::from_utc_and_tz(at, tz).unwrap();
            let entry = Entry::new_legacy("work", Verb::Point, None, None, Some(occur), None, "d1");
            db.insert_entry(&entry).unwrap();
        }
        let issues = collect_tz_drift(&db, &all(&db), false, None).unwrap();
        let Some(issue) = issues.iter().find(|i| i.category == "tz_off_neighbours") else {
            return vec![];
        };
        assert_eq!(issue.severity, Severity::Warning);
        issue.entries.iter().map(|e| e.state.clone()).collect()
    }

    /// A spike — one or two rows the zone around them agrees is not theirs —
    /// is reported whole; a step, a move, is not, nor is a run as long as
    /// the window, nor a spike at the edge with nothing past it.
    #[test]
    fn tz_check_flags_a_spike_and_not_a_step() {
        let tokyo = "Asia/Tokyo, around Europe/Berlin".to_string();
        assert_eq!(off_hours("BBBTBBB"), std::slice::from_ref(&tokyo));
        assert_eq!(off_hours("BBBTTBBB"), [tokyo.clone(), tokyo]);
        assert!(off_hours("BBBTTTT").is_empty());
        assert!(off_hours("BBBTTTBBB").is_empty());
        assert!(off_hours("BBBT").is_empty());
        assert!(off_hours("TBBB").is_empty());
        // Two spikes a run of two apart are each found.
        assert_eq!(off_hours("BTBBTBBB").len(), 2);
    }

    /// An in-sync row (offset derived correctly at construction) produces no
    /// `tz` issue — the checker only fires on genuine desync.
    #[test]
    fn tz_check_clean_db_emits_nothing() {
        let db = Db::open_in_memory().unwrap();
        let occur =
            DatumTime::from_utc_and_tz(t_at("2024-07-15T12:00:00+00:00").utc, "America/Chicago")
                .unwrap();
        let entry = Entry::new_legacy("work", Verb::Switch, None, None, Some(occur), None, "d1");
        db.insert_entry(&entry).unwrap();
        assert!(collect_tz_drift(&db, &all(&db), false, None)
            .unwrap()
            .is_empty());
    }

    /// An unparseable stored `occur_time_tz` can't be reconciled against the
    /// offset, so the row is flagged too — `local_time` is meaningless until
    /// the name is repaired.
    #[test]
    fn tz_check_flags_unparseable_timezone() {
        let db = Db::open_in_memory().unwrap();
        let entry = Entry::new_legacy(
            "work",
            Verb::Switch,
            None,
            None,
            Some(t_at("2024-01-15T12:00:00+00:00")),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();
        db.conn()
            .execute(
                "UPDATE entries SET occur_time_tz = 'Mars/Olympus_Mons' WHERE field = 'work'",
                [],
            )
            .unwrap();
        let issues = collect_tz_drift(&db, &all(&db), false, None).unwrap();
        assert_eq!(issues.len(), 1);
        // The name is the fault, not the offset, and the message says which
        // name (da-kvosn).
        assert_eq!(issues[0].category, "unknown_timezone");
        assert_eq!(
            issues[0].message,
            "1 entry has a timezone name datum cannot resolve (Mars/Olympus_Mons), \
             so the offset cannot be checked",
        );
    }

    /// da-kvosn: a fixed offset's name — `UTC+2` as v2 writes it, and
    /// `Etc/GMT-2` as v3 wrote it until then — resolves to the offset it
    /// says, so a row holding the right offset beside either is clean.
    #[test]
    fn tz_check_reads_fixed_offset_names() {
        for tz in ["UTC+2", "Etc/GMT-2"] {
            let db = Db::open_in_memory().unwrap();
            let occur = DatumTime {
                offset_minutes: 120,
                timezone: Some(tz.to_string()),
                ..t_at("2024-06-18T20:42:00+00:00")
            };
            let entry =
                Entry::new_legacy("walk", Verb::Switch, None, None, Some(occur), None, "d1");
            db.insert_entry(&entry).unwrap();
            assert!(
                collect_tz_drift(&db, &all(&db), false, None)
                    .unwrap()
                    .is_empty(),
                "{tz}"
            );
        }
    }

    /// A row with a non-zero offset and *no* stored zone is clean: an
    /// offset is self-verifying, and there is no zone claim for it to
    /// contradict. This is the shape a legacy import produces when the
    /// backup carried `o` but no `tz` (da-vfbq) — before the fix those rows
    /// were written as `tz='UTC'` beside the offset and every one of them
    /// was flagged.
    #[test]
    fn tz_check_skips_rows_with_no_stored_timezone() {
        let db = Db::open_in_memory().unwrap();
        let occur = DatumTime {
            utc: t_at("2024-01-15T12:00:00+00:00").utc,
            offset_minutes: 60,
            timezone: None,
            uncertainty: None,
        };
        let entry = Entry::new_legacy("work", Verb::Switch, None, None, Some(occur), None, "d1");
        db.insert_entry(&entry).unwrap();

        let stored: Option<String> = db
            .conn()
            .query_row(
                "SELECT occur_time_tz FROM entries WHERE field = 'work'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, None, "an absent zone stays NULL in the column");
        assert!(
            collect_tz_drift(&db, &all(&db), false, None)
                .unwrap()
                .is_empty(),
            "offset without a zone name is not a mismatch",
        );
    }

    /// Tombstoned rows are historical, not live data — the `tz` checker
    /// skips them even when their stored offset is desynced.
    #[test]
    fn tz_check_skips_tombstoned_rows() {
        let db = Db::open_in_memory().unwrap();
        let occur =
            DatumTime::from_utc_and_tz(t_at("2024-01-15T12:00:00+00:00").utc, "America/Chicago")
                .unwrap();
        let entry = Entry::new_legacy("work", Verb::Switch, None, None, Some(occur), None, "d1");
        db.insert_entry(&entry).unwrap();
        db.conn()
            .execute(
                "UPDATE entries SET occur_time_offset = 0, \
                 deleted_at = '2024-01-16T00:00:00+00:00' WHERE field = 'work'",
                [],
            )
            .unwrap();
        assert!(
            collect_tz_drift(&db, &all(&db), false, None)
                .unwrap()
                .is_empty(),
            "desynced tombstone must not be flagged",
        );
    }
}
