//! The `tz` checker: a stored offset its zone disagrees with, and a zone
//! name nothing resolves.

use anyhow::Result;
use datum_core::db::Db;
use datum_core::entry::Entry;

use super::{render_flagged_block, Domain, FlaggedEntry, Issue, IssueExample, Severity};

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
pub(super) fn collect_tz_drift(db: &Db, entries: &[Entry]) -> Result<Vec<Issue>> {
    // Two faults with two fixes (da-kvosn): an offset its zone disagrees
    // with, and a zone name nothing resolves, whose offset may be right.
    let mut mismatched: Vec<(Entry, String)> = Vec::new();
    let mut unknown: Vec<(Entry, String)> = Vec::new();
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
            Ok(Some(expected)) => mismatched.push((
                e.clone(),
                format!("tz={tz}  stored_offset={stored}  expected_offset={expected}"),
            )),
            Err(err) => {
                unknown.push((e.clone(), format!("tz={tz}  stored_offset={stored}  {err}")))
            }
        }
    }

    let issue = |rows: Vec<(Entry, String)>, category, says: &str| {
        let (flagged, details): (Vec<Entry>, Vec<String>) = rows.into_iter().unzip();
        let first = flagged.first()?;
        Some(Issue {
            domain: Domain::Entries,
            severity: Severity::Error,
            category,
            message: match flagged.len() {
                1 => format!("1 entry has {says}"),
                n => format!("{n} entries have {says}"),
            },
            count: flagged.len(),
            example: Some(IssueExample {
                id: Some(first.slug.to_string()),
                field: Some(first.field.clone()),
                detail: details.into_iter().next(),
            }),
            entries: flagged
                .iter()
                .map(|e| FlaggedEntry {
                    hid: e.short_uid(),
                    field: e.field.clone(),
                    // The stored→expected offset, or the name that failed, so
                    // the row's problem is legible without re-reading the DB.
                    state: e
                        .occur_time
                        .as_ref()
                        .map(|t| match t.expected_offset_minutes() {
                            Ok(Some(exp)) => format!("{}→{exp}", t.offset_minutes),
                            _ => format!("tz={}", t.timezone.as_deref().unwrap_or("—")),
                        })
                        .unwrap_or_default(),
                })
                .collect(),
            rendered_block: render_flagged_block(db, &flagged),
        })
    };
    let names: std::collections::BTreeSet<&str> = unknown
        .iter()
        .filter_map(|(e, _)| e.occur_time.as_ref()?.timezone.as_deref())
        .collect();
    let names = names.into_iter().collect::<Vec<_>>().join(", ");
    Ok([
        issue(
            mismatched,
            "tz_offset_mismatch",
            "a stored UTC offset that disagrees with their timezone at that instant",
        ),
        issue(
            unknown,
            "unknown_timezone",
            &format!(
                "a timezone name datum cannot resolve ({names}), so the offset cannot be checked"
            ),
        ),
    ]
    .into_iter()
    .flatten()
    .collect())
}

#[cfg(test)]
mod tests {
    use super::super::tests::{all, t_at};
    use super::*;
    use datum_core::entry::Verb;
    use datum_core::time::DatumTime;

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

        let issues = collect_tz_drift(&db, &all(&db)).unwrap();
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
        assert!(collect_tz_drift(&db, &all(&db)).unwrap().is_empty());
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
        let issues = collect_tz_drift(&db, &all(&db)).unwrap();
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
            assert!(collect_tz_drift(&db, &all(&db)).unwrap().is_empty(), "{tz}");
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
            collect_tz_drift(&db, &all(&db)).unwrap().is_empty(),
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
            collect_tz_drift(&db, &all(&db)).unwrap().is_empty(),
            "desynced tombstone must not be flagged",
        );
    }
}
