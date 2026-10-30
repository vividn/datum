//! The `tz` checker: a stored offset its zone disagrees with, and a zone
//! name nothing resolves.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use datum_core::db::{agreed_zone, is_meta_field, Db, NEIGHBOURS};
use datum_core::entry::Entry;
use datum_core::state::FieldStateRows;
use datum_core::timezone_field::{self, TIMEZONE_FIELD};

use super::{
    field_and_state, render_flagged_block, Domain, FlaggedEntry, Issue, IssueExample, Severity,
    FLAGGED_ROW,
};

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
/// With `around`, each row between the entries either side of it, with their
/// zones and offsets ([`around_blocks`]), which `datum check tz` shows
/// (da-3zq6g, da-la4n9). With `near`,
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
    let mut not_zones: Vec<(Entry, String, String)> = Vec::new();
    for e in entries {
        if e.deleted_at.is_some() {
            continue;
        }
        // da-yw6o2: a `timezone` state from before the field was reserved
        // (a v2 import, say) stays as it was, and is said here.
        if e.field == TIMEZONE_FIELD {
            let other = [&e.state, &e.following_state]
                .into_iter()
                .flatten()
                .find(|s| !timezone_field::holds(s));
            if let Some(state) = other {
                not_zones.push((e.clone(), format!("state={state}"), state.to_string()));
            }
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
                let notes: Vec<&str> = (rows.iter())
                    .map(|(.., state)| match category {
                        "tz_off_declared" => state.as_str(),
                        _ => "",
                    })
                    .collect();
                around_blocks(db, &found, &notes)?
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
            not_zones,
            Severity::Warning,
            "timezone_state_not_a_zone",
            "a `timezone` state that is no IANA zone",
        )?,
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
        issue(
            off_declared(db, entries)?,
            Severity::Warning,
            "tz_off_declared",
            "a timezone other than the one the `timezone` field holds at that instant",
        )?,
    ]
    .into_iter()
    .flatten()
    .collect())
}

/// A block per entry of `found`, a blank line between: the [`NEIGHBOURS`]
/// entries either side of it and it, in time order, their date and time,
/// offset, zone and hid in columns, each in its own zone. It alone is marked,
/// [`FLAGGED_ROW`], and has its field and state after the columns, then its
/// note where `notes` has one.
fn around_blocks(db: &Db, found: &[Entry], notes: &[&str]) -> Result<String> {
    let mut blocks = Vec::new();
    for ((e, drawn), note) in found.iter().zip(field_and_state(db, found)).zip(notes) {
        let (mut before, after) = match e.occur_time.as_ref() {
            Some(at) => db.entries_around(at.utc)?,
            None => Default::default(),
        };
        before.reverse();
        let rows: Vec<[String; 4]> = (before.iter().chain([e]).chain(&after))
            .map(zone_columns)
            .collect();
        let width = |c: usize| rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0);
        let [time_w, offset_w, zone_w, hid_w] = [0, 1, 2, 3].map(width);
        let lines: Vec<String> = (rows.iter().enumerate())
            .map(|(i, [time, offset, zone, hid])| {
                let row =
                    format!("{time:<time_w$}  {offset:<offset_w$}  {zone:<zone_w$}  {hid:<hid_w$}");
                match i == before.len() {
                    true => format!("{FLAGGED_ROW}{row}  {drawn}  {note}")
                        .trim_end()
                        .to_string(),
                    false => row.trim_end().to_string(),
                }
            })
            .collect();
        blocks.push(lines.join("\n") + "\n");
    }
    Ok(blocks.join("\n"))
}

/// An entry's date and time at its stored offset, that offset, its zone as
/// stored, resolvable or not, and its hid. An offset no clock has is said in
/// minutes, beside the time in UTC.
fn zone_columns(e: &Entry) -> [String; 4] {
    let Some(t) = e.occur_time.as_ref() else {
        return ["—".into(), "—".into(), "—".into(), e.short_uid()];
    };
    const AT: &str = "%Y-%m-%d %H:%M:%S";
    let clock = (t.offset_minutes.checked_mul(60)).and_then(chrono::FixedOffset::east_opt);
    let (time, offset) = match clock {
        Some(clock) => {
            let at = t.utc.with_timezone(&clock);
            (at.format(AT).to_string(), at.format("%:z").to_string())
        }
        None => (
            t.utc.format(AT).to_string(),
            format!("{:+}m", t.offset_minutes),
        ),
    };
    let zone = t.timezone.clone().unwrap_or_else(|| "—".into());
    [time, offset, zone, e.short_uid()]
}

/// The rows of `entries` whose zone is not the one the `timezone` field holds
/// at their instant, by name ([`timezone_field::same_zone`], da-wpgrm). Where
/// it holds no zone it claims nothing: before its first row, under `IGNORED`.
/// Not compared: a row with no zone name, which claims none, or one nothing
/// resolves, which is `unknown_timezone`; the field's own rows, in the zone
/// they name or `timezone_state_not_a_zone`; and a meta field's, whose time is
/// no moment lived, as [`Db::zone_sequence`] leaves them out.
fn off_declared(db: &Db, entries: &[Entry]) -> Result<Vec<(Entry, String, String)>> {
    let rows = FieldStateRows::load_transitions(db.conn(), TIMEZONE_FIELD)?;
    let stretches: Vec<_> = (rows.changes().into_iter())
        .map(|c| (c.at, timezone_field::state_zone(&c.state)))
        .collect();
    let mut off = Vec::new();
    for e in entries {
        let Some(t) = e.occur_time.as_ref() else {
            continue;
        };
        if e.deleted_at.is_some()
            || e.superseded_by.is_some()
            || e.field == TIMEZONE_FIELD
            || is_meta_field(&e.field)
            || !matches!(t.expected_offset_minutes(), Ok(Some(_)))
        {
            continue;
        }
        let holding = stretches.partition_point(|(at, _)| *at <= t.utc);
        let declared = holding.checked_sub(1).and_then(|i| stretches[i].1.as_ref());
        let (Some(declared), Some(own)) = (declared, t.timezone.as_deref()) else {
            continue;
        };
        if !timezone_field::same_zone(own, declared) {
            off.push((
                e.clone(),
                format!("tz={own}  declared={declared}"),
                format!("declared {declared}"),
            ));
        }
    }
    off.sort_by_key(|(e, ..)| e.occur_time.as_ref().map(|t| t.utc));
    Ok(off)
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

    /// `timezone_field`'s `SPELLINGS` is drawn from the tzdb chrono-tz ships,
    /// whose links no running code can read. A rename adds a name, so a change
    /// in how many says to draw the table again, from the `backward` file's
    /// sections its comment names, and then change this number.
    #[test]
    fn a_new_tzdb_is_a_reminder_to_draw_the_spellings_again() {
        assert_eq!(chrono_tz::TZ_VARIANTS.len(), 597);
    }

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

    /// A `work` entry at `utc`, in `zone`.
    fn at(db: &Db, utc: &str, zone: &str) {
        let occur = DatumTime::from_utc_and_tz(t_at(utc).utc, zone).unwrap();
        let entry = Entry::new_legacy("work", Verb::Point, None, None, Some(occur), None, "d1");
        db.insert_entry(&entry).unwrap();
    }

    /// The lines of `category`'s block as `datum check tz` renders it.
    fn block_of(db: &Db, category: &str) -> Vec<String> {
        let issues = collect_tz_drift(db, &all(db), true, None).unwrap();
        let issue = issues.iter().find(|i| i.category == category).unwrap();
        (issue.rendered_block.lines())
            .map(crate::render::colors::strip_ansi)
            .collect()
    }

    /// A flagged row between its neighbors (da-la4n9): the columns line up by
    /// position across every row, whatever the zone's length or the offset's
    /// size, each row at its own clock, and only the flagged row has the entry.
    #[test]
    fn around_columns_line_up_by_position() {
        let db = Db::open_in_memory().unwrap();
        const LONG: &str = "America/Argentina/ComodRivadavia";
        at(&db, "2024-01-15T00:00:00+00:00", "Asia/Tokyo");
        at(&db, "2024-01-15T01:00:00+00:00", "Pacific/Chatham");
        at(&db, "2024-01-15T02:00:00+00:00", LONG);
        at(&db, "2024-01-15T03:00:00+00:00", "Europe/Berlin");
        db.conn()
            .execute(
                "UPDATE entries SET occur_time_offset = 0 WHERE occur_time_tz = 'Europe/Berlin'",
                [],
            )
            .unwrap();
        let rows = block_of(&db, "tz_offset_mismatch");
        let cells: Vec<(&str, &str, &str, &str)> = (rows.iter())
            .map(|r| {
                let r = r.strip_prefix(FLAGGED_ROW).unwrap_or(r);
                let hid = 29 + LONG.len() + 2;
                (&r[..19], &r[21..27], r[29..hid].trim_end(), &r[hid..])
            })
            .collect();
        assert_eq!(
            cells.iter().map(|c| (c.0, c.1, c.2)).collect::<Vec<_>>(),
            [
                ("2024-01-15 09:00:00", "+09:00", "Asia/Tokyo"),
                ("2024-01-15 14:45:00", "+13:45", "Pacific/Chatham"),
                ("2024-01-14 23:00:00", "-03:00", LONG),
                ("2024-01-15 03:00:00", "+00:00", "Europe/Berlin"),
            ],
            "{rows:#?}"
        );
        let flagged: Vec<bool> = rows.iter().map(|r| r.starts_with(FLAGGED_ROW)).collect();
        assert_eq!(flagged, [false, false, false, true], "{rows:#?}");
        let entry: Vec<bool> = cells.iter().map(|c| c.3.contains("work")).collect();
        assert_eq!(entry, flagged, "{rows:#?}");
    }

    /// A zone nothing resolves is said as stored, an offset no clock has in
    /// minutes, and the columns stay.
    #[test]
    fn around_says_what_a_row_stored() {
        let db = Db::open_in_memory().unwrap();
        at(&db, "2024-01-15T00:00:00+00:00", "Asia/Tokyo");
        at(&db, "2024-01-15T01:00:00+00:00", "Europe/Berlin");
        db.conn()
            .execute(
                "UPDATE entries SET occur_time_tz = 'Mars/Base', occur_time_offset = 2000
                  WHERE occur_time_tz = 'Europe/Berlin'",
                [],
            )
            .unwrap();
        let rows = block_of(&db, "unknown_timezone");
        assert_eq!(rows.len(), 2, "{rows:#?}");
        let tokyo = "2024-01-15 09:00:00  +09:00  Asia/Tokyo";
        assert!(rows[0].starts_with(tokyo), "{rows:#?}");
        let mars = "> 2024-01-15 01:00:00  +2000m  Mars/Base ";
        assert!(rows[1].starts_with(mars), "{rows:#?}");
    }

    /// A newline in the flagged entry's state stays on its row.
    #[test]
    fn around_keeps_a_state_on_its_row() {
        let db = Db::open_in_memory().unwrap();
        let occur =
            DatumTime::from_utc_and_tz(t_at("2024-01-15T00:00:00+00:00").utc, "Europe/Berlin")
                .unwrap();
        let state = serde_json::json!("a\nb");
        let entry = Entry::new_legacy(
            "work",
            Verb::PointSwitch,
            Some(state),
            None,
            Some(occur),
            None,
            "d1",
        );
        db.insert_entry(&entry).unwrap();
        db.conn()
            .execute("UPDATE entries SET occur_time_offset = 0", [])
            .unwrap();
        let rows = block_of(&db, "tz_offset_mismatch");
        assert_eq!(rows.len(), 1, "{rows:#?}");
        assert!(rows[0].contains("a\\nb"), "{rows:#?}");
    }
}
